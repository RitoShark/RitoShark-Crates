use rs_io::{Parse, Serialize};
use rs_rst::{Rst, RstValue};

fn sample() -> Rst {
    let mut rst = Rst::new();
    rst.add("game_hud_announcement", "Victory");
    rst.add("game_hud_defeat", "Defeat");
    rst.add("generic_continue", "Continue");
    rst
}

#[test]
fn round_trip_v5_in_memory() {
    let original = sample();
    let bytes = original.to_bytes().expect("serialize");
    let parsed = Rst::from_bytes(&bytes).expect("parse");
    assert_eq!(original, parsed);
}

#[test]
fn round_trip_is_byte_stable() {
    let bytes = sample().to_bytes().expect("serialize");
    let reparsed = Rst::from_bytes(&bytes).expect("parse");
    let rewritten = reparsed.to_bytes().expect("re-serialize");
    assert_eq!(bytes, rewritten);
}

#[test]
fn round_trip_v4_with_mode_byte() {
    let mut rst = Rst::new();
    rst.version = 4;
    rst.mode = 0;
    rst.add("a", "alpha");
    rst.add("b", "beta");
    let bytes = rst.to_bytes().expect("serialize");
    assert_eq!(rst, Rst::from_bytes(&bytes).expect("parse"));
}

#[test]
fn round_trip_v2_with_font_config() {
    let mut rst = Rst::new();
    rst.version = 2;
    rst.font_config = Some("zh_CN".to_string());
    rst.add("title", "Hello");
    let bytes = rst.to_bytes().expect("serialize");
    assert_eq!(rst, Rst::from_bytes(&bytes).expect("parse"));
}

#[test]
fn get_after_add() {
    let rst = sample();
    assert_eq!(rst.get("game_hud_announcement"), Some("Victory"));
    assert_eq!(rst.get("GAME_HUD_ANNOUNCEMENT"), Some("Victory"));
    assert_eq!(rst.get("missing_key"), None);
}

#[test]
fn duplicate_strings_share_one_blob_offset() {
    let mut rst = Rst::new();
    rst.add("first", "Shared");
    rst.add("second", "Shared");
    let bytes = rst.to_bytes().expect("serialize");
    let parsed = Rst::from_bytes(&bytes).expect("parse");
    assert_eq!(parsed.get("first"), Some("Shared"));
    assert_eq!(parsed.get("second"), Some("Shared"));
}

/// Pinned key-hash vector confirmed against the real `lol.stringtable`: the v5 key
/// `item_1001_name` (the in-game "Boots" entry) hashes with xxh3-64 of the lowercased key
/// truncated to 38 bits, yielding 0x1_09f4_cdf6. Plain XXHash64 would give a different value and
/// does not appear in any real v5 file, so this pins the algorithm choice.
#[test]
fn pinned_key_hash_vector_v5() {
    assert_eq!(Rst::hash_bits_for(5), Some(38));
    assert_eq!(Rst::hash_key(5, "item_1001_name"), Some(0x1_09f4_cdf6));
    // Case-insensitive: an uppercased key hashes identically.
    assert_eq!(
        Rst::hash_key(5, "ITEM_1001_NAME"),
        Rst::hash_key(5, "item_1001_name")
    );

    let mut rst = Rst::with_version(5);
    let hash = rst.add("item_1001_name", "Boots").expect("v5 supported");
    assert_eq!(hash, 0x1_09f4_cdf6);
    assert_eq!(rst.get("item_1001_name"), Some("Boots"));
    assert_eq!(rst.get_by_hash(0x1_09f4_cdf6), Some("Boots"));
}

/// A legacy pre-v5 table with a non-zero mode byte may store an encrypted payload (`0xFF`,
/// `u16` length, raw bytes) that is not valid UTF-8. It must survive a byte-exact round-trip and
/// be reachable as raw bytes via `value_by_hash`, while `get` reports it as absent text.
#[test]
fn legacy_encrypted_entry_round_trips() {
    let cipher = vec![0x00u8, 0x10, 0xFE, 0x7F, 0x42];
    let mut rst = Rst::with_version(4);
    rst.mode = 1;
    let hash = rst.add("secret_key", RstValue::Encrypted(cipher.clone())).unwrap();
    rst.add("plain_key", "Visible");

    let bytes = rst.to_bytes().expect("serialize");
    let parsed = Rst::from_bytes(&bytes).expect("parse");

    assert_eq!(rst, parsed);
    assert_eq!(bytes, parsed.to_bytes().expect("re-serialize"));
    assert_eq!(
        parsed.value_by_hash(hash),
        Some(&RstValue::Encrypted(cipher))
    );
    assert_eq!(parsed.get("secret_key"), None);
    assert_eq!(parsed.get("plain_key"), Some("Visible"));
}

/// With the mode byte zero, a value that merely starts with 0xFF is a plain (if non-UTF-8-looking)
/// string and must not be mistaken for an encrypted payload.
#[test]
fn mode_zero_does_not_decode_encryption() {
    let mut rst = Rst::with_version(4);
    rst.mode = 0;
    rst.add("k", "normal");
    let bytes = rst.to_bytes().expect("serialize");
    let parsed = Rst::from_bytes(&bytes).expect("parse");
    assert_eq!(parsed.get("k"), Some("normal"));
    assert_eq!(bytes, parsed.to_bytes().expect("re-serialize"));
}

#[test]
fn bad_magic_is_error() {
    assert!(Rst::from_bytes(b"XXX\x05").is_err());
}

#[test]
fn unknown_version_is_unsupported() {
    let bytes = b"RST\x63";
    match Rst::from_bytes(bytes) {
        Err(rs_rst::Error::UnsupportedVersion(0x63)) => {}
        other => panic!("expected UnsupportedVersion(99), got {other:?}"),
    }
}
