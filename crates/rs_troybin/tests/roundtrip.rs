use std::path::PathBuf;

use rs_io::{Parse, Serialize};
use rs_troybin::{BucketValues, ScalarValue, Troybin, TroybinBody};

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

/// Build the synthetic v2 file (bit-0 i32 + bit-12 strings) used by several edit tests.
fn synthetic_v2_bytes() -> Vec<u8> {
    let mut bytes = vec![2u8];
    let blob = b"hi\0bye\0";
    bytes.extend_from_slice(&(blob.len() as u16).to_le_bytes());
    let flags: u16 = (1 << 0) | (1 << 12);
    bytes.extend_from_slice(&flags.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&0x1111_2222u32.to_le_bytes());
    bytes.extend_from_slice(&(-5i32).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&0xAAAA_0001u32.to_le_bytes());
    bytes.extend_from_slice(&0xAAAA_0002u32.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes());
    bytes.extend_from_slice(blob);
    bytes
}

fn v2(parsed: &mut Troybin) -> &mut rs_troybin::TroybinV2 {
    match &mut parsed.body {
        TroybinBody::V2(b) => b,
        _ => panic!("expected v2"),
    }
}

#[test]
fn flat_get_iter_and_scalar_set() {
    let mut parsed = Troybin::from_bytes(&synthetic_v2_bytes()).unwrap();
    let body = v2(&mut parsed);

    assert_eq!(body.get(0x1111_2222), Some(ScalarValue::I32(-5)));
    assert_eq!(
        body.get(0xAAAA_0001),
        Some(ScalarValue::String(b"hi".to_vec()))
    );
    assert_eq!(
        body.get(0xAAAA_0002),
        Some(ScalarValue::String(b"bye".to_vec()))
    );
    assert_eq!(body.get(0xDEAD_BEEF), None);
    assert_eq!(body.iter().count(), 3);

    // Overwrite a scalar in place; type mismatch is a hard error, not a silent no-op.
    assert!(body.set(0x1111_2222, ScalarValue::I32(7)).unwrap());
    assert_eq!(body.get(0x1111_2222), Some(ScalarValue::I32(7)));
    assert!(body.set(0x1111_2222, ScalarValue::F32(1.0)).is_err());
    assert!(!body.set(0xDEAD_BEEF, ScalarValue::I32(1)).unwrap());
}

#[test]
fn insert_and_remove_keep_buckets_consistent() {
    let mut parsed = Troybin::from_bytes(&synthetic_v2_bytes()).unwrap();
    let body = v2(&mut parsed);

    // New i32 joins the existing bit-0 bucket; a brand-new type creates its bucket in bit order.
    assert_eq!(body.insert(0x0000_0009, ScalarValue::I32(42)).unwrap(), 0);
    assert_eq!(body.insert(0x0000_000A, ScalarValue::F32(1.5)).unwrap(), 1);
    assert_eq!(body.get(0x0000_0009), Some(ScalarValue::I32(42)));
    assert_eq!(body.get(0x0000_000A), Some(ScalarValue::F32(1.5)));
    assert!(
        body.buckets
            .windows(2)
            .all(|w| w[0].flag_bit < w[1].flag_bit)
    );
    for b in &body.buckets {
        assert_eq!(
            b.hashes.len(),
            b.decoded().len(),
            "parallel columns aligned"
        );
    }

    assert_eq!(
        body.remove(0xAAAA_0002).unwrap(),
        Some(ScalarValue::String(b"bye".to_vec()))
    );
    assert_eq!(body.get(0xAAAA_0002), None);
    assert_eq!(
        body.remove(0xAAAA_0001).unwrap(),
        Some(ScalarValue::String(b"hi".to_vec()))
    );
    // Strings bucket emptied → dropped entirely.
    assert!(body.buckets.iter().all(|b| b.flag_bit != 12));
}

/// Editing a string to a different length rebuilds the blob/offsets and `strings_length` so the
/// result re-parses and is itself byte-stable.
#[test]
fn managed_string_edit_reparses_and_is_stable() {
    let mut parsed = Troybin::from_bytes(&synthetic_v2_bytes()).unwrap();
    assert!(
        v2(&mut parsed)
            .set(
                0xAAAA_0001,
                ScalarValue::String(b"a-much-longer-value".to_vec())
            )
            .unwrap()
    );

    let written = parsed.to_bytes().expect("write edited");
    let reparsed = Troybin::from_bytes(&written).expect("reparse edited");
    let mut reparsed_mut = reparsed.clone();
    assert_eq!(
        v2(&mut reparsed_mut).get(0xAAAA_0001),
        Some(ScalarValue::String(b"a-much-longer-value".to_vec()))
    );
    assert_eq!(
        v2(&mut reparsed_mut).get(0xAAAA_0002),
        Some(ScalarValue::String(b"bye".to_vec())),
        "the untouched sibling string survives the rebuild"
    );
    assert_eq!(
        reparsed.to_bytes().unwrap(),
        written,
        "an edited file round-trips byte-exact on the next pass"
    );
}

#[test]
fn bool_values_decode_and_pack() {
    // bit 5: three booleans 1,0,1 packed into one byte (0b101 = 5).
    let mut bytes = vec![2u8];
    bytes.extend_from_slice(&0u16.to_le_bytes()); // strings_length
    bytes.extend_from_slice(&(1u16 << 5).to_le_bytes()); // flags: bit 5
    bytes.extend_from_slice(&3u16.to_le_bytes()); // count
    for h in [0xA1u32, 0xA2, 0xA3] {
        bytes.extend_from_slice(&h.to_le_bytes());
    }
    bytes.push(0b0000_0101);

    let mut parsed = Troybin::from_bytes(&bytes).unwrap();
    let body = v2(&mut parsed);
    assert_eq!(body.get(0xA1), Some(ScalarValue::Bool(true)));
    assert_eq!(body.get(0xA2), Some(ScalarValue::Bool(false)));
    assert_eq!(body.get(0xA3), Some(ScalarValue::Bool(true)));

    assert!(body.set(0xA2, ScalarValue::Bool(true)).unwrap());
    assert_eq!(parsed.to_bytes().unwrap().last(), Some(&0b0000_0111));
}

#[test]
fn troybin_section_name_accessors() {
    let mut parsed = Troybin::from_bytes(&synthetic_v2_bytes()).unwrap();
    let hash = Troybin::property_hash("Demo", "p-life");
    assert!(parsed.get("Demo", "p-life").is_none());
    parsed.set("Demo", "p-life", ScalarValue::F32(2.0)).unwrap();
    assert_eq!(parsed.get("Demo", "p-life"), Some(ScalarValue::F32(2.0)));
    assert_eq!(v2(&mut parsed).get(hash), Some(ScalarValue::F32(2.0)));
}

#[test]
fn resolver_names_real_file_hashes() {
    let Some(dir) = sample_dir() else {
        return;
    };
    for name in TROYBIN_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let parsed = Troybin::from_bytes(&std::fs::read(&path).unwrap()).unwrap();
        let resolver = parsed.resolver();
        assert!(!resolver.is_empty());

        let TroybinBody::V2(body) = &parsed.body else {
            continue;
        };
        let (mut total, mut resolved) = (0usize, 0usize);
        for (hash, _) in body.iter() {
            total += 1;
            if resolver.name(hash).is_some() {
                resolved += 1;
            }
        }
        eprintln!("{name}: resolved {resolved}/{total} property hashes");
        assert!(resolved > 0, "{name}: resolver matched nothing");
    }
}

#[test]
fn property_hash_is_stable() {
    // Chained ihash: ihash(name, ihash("*", ihash(section))). Pin the algorithm with a manual value.
    let section_hash = rs_hash::ihash_seeded(rs_hash::ihash("System"), "*");
    let expected = rs_hash::ihash_seeded(section_hash, "p-life");
    assert_eq!(Troybin::property_hash("System", "p-life"), expected);
}
