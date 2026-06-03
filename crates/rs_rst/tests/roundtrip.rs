use rs_io::{Parse, Serialize};
use rs_rst::Rst;

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
