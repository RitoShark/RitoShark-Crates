use std::path::PathBuf;

use rs_io::{Parse, Serialize};
use rs_troybin::{BucketValues, Troybin, TroybinBody};

fn sample_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sample-files");
    dir.is_dir().then_some(dir)
}

const TROYBIN_FILES: &[&str] = &[
    "sru_baron_spawn_sound.troybin",
    "sru_airdragon_ba_impact.troybin",
];

#[test]
fn real_files_round_trip_byte_exact() {
    let Some(dir) = sample_dir() else {
        eprintln!("sample-files directory missing; skipping real .troybin tests");
        return;
    };

    for name in TROYBIN_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("missing sample {name}; skipping");
            continue;
        }

        let original = std::fs::read(&path).expect("read sample");
        let parsed = Troybin::from_bytes(&original).expect("parse troybin");
        let written = parsed.to_bytes().expect("write troybin");
        assert!(
            written == original,
            "{name}: round-trip is not byte-exact ({} vs {} bytes)",
            written.len(),
            original.len()
        );

        let TroybinBody::V2(body) = &parsed.body else {
            panic!("{name}: expected a v2 body");
        };
        eprintln!("{name}: v2, {} buckets", body.buckets.len());
    }
}

/// A hand-built v2 file exercising one scalar bucket (bit 0, i32) and one string bucket (bit 12),
/// confirming the bucket layout and the strings blob round-trip without any real fixture present.
#[test]
fn synthetic_v2_round_trips() {
    let mut bytes = Vec::new();
    bytes.push(2u8); // version
    let blob = b"hi\0bye\0";
    bytes.extend_from_slice(&(blob.len() as u16).to_le_bytes()); // strings_length
    let flags: u16 = (1 << 0) | (1 << 12);
    bytes.extend_from_slice(&flags.to_le_bytes());

    // bit 0: i32 bucket, one entry
    bytes.extend_from_slice(&1u16.to_le_bytes()); // count
    bytes.extend_from_slice(&0x1111_2222u32.to_le_bytes()); // hash
    bytes.extend_from_slice(&(-5i32).to_le_bytes()); // value

    // bit 12: strings bucket, two entries (offsets into the blob)
    bytes.extend_from_slice(&2u16.to_le_bytes()); // count
    bytes.extend_from_slice(&0xAAAA_0001u32.to_le_bytes());
    bytes.extend_from_slice(&0xAAAA_0002u32.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // offset -> "hi"
    bytes.extend_from_slice(&3u16.to_le_bytes()); // offset -> "bye"
    bytes.extend_from_slice(blob);

    let parsed = Troybin::from_bytes(&bytes).expect("parse synthetic");
    assert_eq!(parsed.version, 2);
    let TroybinBody::V2(body) = &parsed.body else {
        panic!("expected v2");
    };
    assert_eq!(body.buckets.len(), 2);
    assert_eq!(body.buckets[0].flag_bit, 0);
    assert!(matches!(body.buckets[0].values, BucketValues::I32(ref v) if v == &[-5]));
    assert_eq!(body.buckets[1].flag_bit, 12);

    let written = parsed.to_bytes().expect("write synthetic");
    assert_eq!(written, bytes, "synthetic v2 must round-trip byte-exact");
}

/// A hand-built v1 file: three header bytes, a `(hash, offset)` table, and the NUL-terminated blob.
#[test]
fn synthetic_v1_round_trips() {
    let mut bytes = Vec::new();
    bytes.push(1u8); // version
    bytes.extend_from_slice(&[0xDE, 0xAD, 0xBE]); // 3 header bytes
    let data = b"value_a\0value_b\0";
    bytes.extend_from_slice(&2u32.to_le_bytes()); // entry count
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes()); // data count
    bytes.extend_from_slice(&0x0000_0001u32.to_le_bytes()); // hash 0
    bytes.extend_from_slice(&0u32.to_le_bytes()); // offset 0
    bytes.extend_from_slice(&0x0000_0002u32.to_le_bytes()); // hash 1
    bytes.extend_from_slice(&8u32.to_le_bytes()); // offset 8
    bytes.extend_from_slice(data);

    let parsed = Troybin::from_bytes(&bytes).expect("parse v1");
    let TroybinBody::V1(body) = &parsed.body else {
        panic!("expected v1");
    };
    assert_eq!(body.header, [0xDE, 0xAD, 0xBE]);
    assert_eq!(body.entries.len(), 2);

    let written = parsed.to_bytes().expect("write v1");
    assert_eq!(written, bytes, "synthetic v1 must round-trip byte-exact");
}

#[test]
fn unknown_version_is_error() {
    assert!(matches!(
        Troybin::from_bytes(&[9u8, 0, 0, 0]),
        Err(rs_troybin::Error::UnsupportedVersion(9))
    ));
}

#[test]
fn property_hash_is_stable() {
    // Chained ihash: ihash(name, ihash("*", ihash(section))). Pin the algorithm with a manual value.
    let section_hash = rs_hash::ihash_seeded(rs_hash::ihash("System"), "*");
    let expected = rs_hash::ihash_seeded(section_hash, "p-life");
    assert_eq!(Troybin::property_hash("System", "p-life"), expected);
}
