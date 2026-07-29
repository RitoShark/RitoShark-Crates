use std::path::PathBuf;

use rs_io::Serialize;
use rs_troybin::{
    BucketValues, Error, Inibin, InibinFlags, ScalarValue, TroybinBody, fixed_point_from_f64,
    fixed_point_to_f64,
};

fn sample_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sample-files");
    dir.is_dir().then_some(dir)
}

const INIBIN_FILES: &[&str] = &[
    "ahribotsummonerspellspracticetool.inibin",
    "alistarbotsummonerspellsclassic.inibin",
    "slime_environmentminion_idle.troybin",
    "sru_baron_spawn_sound.troybin",
    "sru_airdragon_ba_impact.troybin",
];

/// The headline contract: real inibin/cfgbin/troybin files read through the inibin entry point and
/// write back byte-for-byte.
#[test]
fn real_files_round_trip_byte_exact_through_inibin_api() {
    let Some(dir) = sample_dir() else {
        eprintln!("sample-files directory missing; skipping real inibin tests");
        return;
    };

    let mut checked = 0;
    for name in INIBIN_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("missing sample {name}; skipping");
            continue;
        }

        let original = std::fs::read(&path).expect("read sample");
        let parsed = Inibin::from_slice(&original).expect("parse inibin");
        let written = parsed.to_bytes().expect("write inibin");
        assert!(
            written == original,
            "{name}: round-trip is not byte-exact ({} vs {} bytes)",
            written.len(),
            original.len()
        );

        let flags = parsed.flags().expect("every bucket bit is a known type");
        assert!(!flags.is_empty(), "{name}: no value types reported");
        eprintln!("{name}: v{} flags {flags:?}", parsed.version);
        checked += 1;
    }
    assert!(checked > 0, "no inibin samples were exercised");
}

/// Every named flag survives the trip through its raw byte and back, and the raw values are exactly
/// the fourteen bucket bits plus 255 for the version-1 body.
#[test]
fn named_flags_round_trip_through_raw_bits() {
    assert_eq!(InibinFlags::ALL.len(), 15);
    for flags in InibinFlags::ALL {
        let raw = flags.to_u8();
        assert_eq!(InibinFlags::from_u8(raw).unwrap(), flags);
        assert_eq!(InibinFlags::try_from(raw).unwrap(), flags);
        assert_eq!(u8::from(flags), raw);
        assert_eq!(InibinFlags::from_str_name(flags.as_str()), Some(flags));
        assert_eq!(flags.to_string(), flags.as_str());
    }

    let raws: Vec<u8> = InibinFlags::ALL.iter().map(|f| f.to_u8()).collect();
    assert_eq!(
        raws,
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 255]
    );

    for bit in 14u8..255 {
        assert!(
            matches!(InibinFlags::from_u8(bit), Err(Error::UnsupportedBucket(b)) if b == bit),
            "bit {bit} must not name a bucket"
        );
    }
    assert!(InibinFlags::from_str_name("NotABucket").is_none());
}

#[test]
fn fixed_point_flags_are_the_u8_vector_buckets() {
    let fixed: Vec<InibinFlags> = InibinFlags::ALL
        .into_iter()
        .filter(|f| f.is_fixed_point())
        .collect();
    assert_eq!(
        fixed,
        vec![
            InibinFlags::FixedPointFloatList,
            InibinFlags::FixedPointFloatListVec3,
            InibinFlags::FixedPointFloatListVec2,
            InibinFlags::FixedPointFloatListVec4,
        ]
    );
}

/// The fixed-point helpers are a display view: the raw byte stays the stored value, and converting
/// back from the float is documented-lossy (rounded and clamped), never the source of truth.
#[test]
fn fixed_point_view_is_derived_and_never_stored() {
    assert_eq!(fixed_point_to_f64(0), 0.0);
    assert!((fixed_point_to_f64(10) - 1.0).abs() < 1e-9);
    assert!((fixed_point_to_f64(255) - 25.5).abs() < 1e-9);

    for raw in 0u8..=255 {
        assert_eq!(
            fixed_point_from_f64(fixed_point_to_f64(raw)),
            raw,
            "raw {raw} must survive the display round-trip"
        );
    }
    assert_eq!(fixed_point_from_f64(-4.0), 0);
    assert_eq!(fixed_point_from_f64(1e9), 255);

    // A fixed-point bucket keeps the raw byte in the model, not the scaled float.
    let mut parsed = Inibin::from_slice(&fixed_point_bytes()).unwrap();
    let body = parsed.v2().unwrap();
    assert_eq!(
        body.get_from(InibinFlags::FixedPointFloatList, 0xF0),
        Some(ScalarValue::U8(25))
    );
    assert!(matches!(
        body.bucket(InibinFlags::FixedPointFloatList).unwrap().values,
        BucketValues::U8(ref v) if v == &[25]
    ));

    // Storing a display value goes through the explicit lossy conversion the caller opted into.
    parsed
        .v2_mut()
        .unwrap()
        .insert_into(
            InibinFlags::FixedPointFloatList,
            0xF0,
            ScalarValue::U8(fixed_point_from_f64(3.7)),
        )
        .unwrap();
    assert_eq!(parsed.get_hash(0xF0), Some(ScalarValue::U8(37)));
    assert_eq!(
        Inibin::from_slice(&parsed.to_bytes().unwrap())
            .unwrap()
            .get_hash(0xF0),
        Some(ScalarValue::U8(37))
    );
}

#[test]
fn buckets_are_addressable_by_named_type() {
    let mut parsed = Inibin::from_slice(&synthetic_v2_bytes()).unwrap();

    assert_eq!(
        parsed.flags().unwrap(),
        vec![InibinFlags::Int32List, InibinFlags::StringList]
    );

    let body = parsed.v2().unwrap();
    assert_eq!(body.len(), 3);
    assert!(!body.is_empty());
    assert!(body.contains(0x1111_2222));
    assert!(!body.contains(0xDEAD_BEEF));
    assert_eq!(
        body.bucket(InibinFlags::Int32List)
            .unwrap()
            .flags()
            .unwrap(),
        InibinFlags::Int32List
    );
    assert!(body.bucket(InibinFlags::Float32List).is_none());
    assert!(body.bucket(InibinFlags::OldFormat).is_none());

    assert_eq!(
        body.get_from(InibinFlags::Int32List, 0x1111_2222),
        Some(ScalarValue::I32(-5))
    );
    assert_eq!(
        body.get_from(InibinFlags::StringList, 0xAAAA_0001),
        Some(ScalarValue::String(b"hi".to_vec()))
    );
    // Right hash, wrong bucket.
    assert_eq!(body.get_from(InibinFlags::StringList, 0x1111_2222), None);

    assert_eq!(parsed.get_hash(0x1111_2222), Some(ScalarValue::I32(-5)));
    assert_eq!(parsed.entries().len(), 3);

    parsed
        .v2_mut()
        .unwrap()
        .bucket_mut(InibinFlags::Int32List)
        .unwrap()
        .hashes[0] = 0x3333_4444;
    assert_eq!(parsed.get_hash(0x3333_4444), Some(ScalarValue::I32(-5)));
}

/// `insert_into` targets a bit explicitly, which is the only way to reach the bits that share a
/// layout with another (4 shares `u8` with 2; 13 shares `i32` with 0).
#[test]
fn insert_into_places_values_under_a_chosen_bit() {
    let mut parsed = Inibin::from_slice(&synthetic_v2_bytes()).unwrap();
    let body = parsed.v2_mut().unwrap();

    body.insert_into(InibinFlags::Int8List, 0x0000_0011, ScalarValue::U8(9))
        .unwrap();
    body.insert_into(
        InibinFlags::Int32LongList,
        0x0000_0012,
        ScalarValue::I32(123),
    )
    .unwrap();

    assert_eq!(
        body.bucket_flags().unwrap(),
        vec![
            InibinFlags::Int32List,
            InibinFlags::Int8List,
            InibinFlags::StringList,
            InibinFlags::Int32LongList,
        ],
        "new buckets land in ascending bit order"
    );
    assert_eq!(
        body.get_from(InibinFlags::Int8List, 0x0000_0011),
        Some(ScalarValue::U8(9))
    );
    assert_eq!(
        body.get_from(InibinFlags::Int32LongList, 0x0000_0012),
        Some(ScalarValue::I32(123))
    );
    assert_eq!(
        body.get_from(InibinFlags::Int32List, 0x0000_0012),
        None,
        "the bit-13 value did not fall back into the canonical i32 bucket"
    );

    // Re-targeting an existing hash moves it rather than duplicating it.
    body.insert_into(InibinFlags::Int32List, 0x0000_0012, ScalarValue::I32(7))
        .unwrap();
    assert_eq!(
        body.get_from(InibinFlags::Int32List, 0x0000_0012),
        Some(ScalarValue::I32(7))
    );
    assert!(body.bucket(InibinFlags::Int32LongList).is_none());
    assert_eq!(
        body.buckets
            .iter()
            .filter(|b| b.hashes.contains(&0x0000_0012))
            .count(),
        1
    );

    // Same bit, existing hash: an in-place overwrite.
    body.insert_into(InibinFlags::Int32List, 0x0000_0012, ScalarValue::I32(8))
        .unwrap();
    assert_eq!(
        body.get_from(InibinFlags::Int32List, 0x0000_0012),
        Some(ScalarValue::I32(8))
    );

    assert!(matches!(
        body.insert_into(InibinFlags::Float32List, 0x0000_0013, ScalarValue::I32(1)),
        Err(Error::ValueTypeMismatch(1))
    ));
    assert!(matches!(
        body.insert_into(InibinFlags::OldFormat, 0x0000_0014, ScalarValue::I32(1)),
        Err(Error::UnsupportedBucket(255))
    ));

    // Everything the edits produced re-parses and is byte-stable on the next pass.
    let written = parsed.to_bytes().unwrap();
    let reparsed = Inibin::from_slice(&written).unwrap();
    assert_eq!(reparsed.to_bytes().unwrap(), written);
}

#[test]
fn set_and_remove_by_hash_through_the_inibin_entry_point() {
    let mut parsed = Inibin::from_slice(&synthetic_v2_bytes()).unwrap();

    parsed.set_hash(0x1111_2222, ScalarValue::I32(77)).unwrap();
    assert_eq!(parsed.get_hash(0x1111_2222), Some(ScalarValue::I32(77)));

    parsed.set_hash(0x0000_00FF, ScalarValue::F32(2.5)).unwrap();
    assert_eq!(parsed.get_hash(0x0000_00FF), Some(ScalarValue::F32(2.5)));
    assert_eq!(
        parsed.flags().unwrap(),
        vec![
            InibinFlags::Int32List,
            InibinFlags::Float32List,
            InibinFlags::StringList
        ]
    );

    assert_eq!(
        parsed.remove_hash(0xAAAA_0001).unwrap(),
        Some(ScalarValue::String(b"hi".to_vec()))
    );
    assert_eq!(parsed.get_hash(0xAAAA_0001), None);
    assert_eq!(parsed.remove_hash(0xDEAD_BEEF).unwrap(), None);

    let written = parsed.to_bytes().unwrap();
    assert_eq!(
        Inibin::from_slice(&written).unwrap().to_bytes().unwrap(),
        written
    );
}

/// A version-1 file keeps the crate's existing v1 semantics through the inibin surface: its body is
/// preserved verbatim and byte-exact, it reports as `OldFormat`, and it stays read-only.
#[test]
fn old_format_v1_is_read_only_and_preserved_verbatim() {
    let bytes = synthetic_v1_bytes();
    let mut parsed = Inibin::from_slice(&bytes).unwrap();

    assert_eq!(parsed.version, 1);
    assert_eq!(parsed.flags().unwrap(), vec![InibinFlags::OldFormat]);
    assert!(parsed.v2().is_none());
    assert!(parsed.v2_mut().is_none());

    let TroybinBody::V1(body) = &parsed.body else {
        panic!("expected a v1 body");
    };
    assert_eq!(body.header, [0xDE, 0xAD, 0xBE]);
    assert_eq!(body.entries.len(), 2);
    assert_eq!(body.data, b"value_a\0value_b\0".to_vec());

    assert_eq!(parsed.get_hash(0x0000_0001), None);
    assert!(parsed.entries().is_empty());
    assert_eq!(parsed.remove_hash(0x0000_0001).unwrap(), None);
    assert!(matches!(
        parsed.set_hash(0x0000_0001, ScalarValue::I32(1)),
        Err(Error::UnsupportedVersion(1))
    ));

    assert_eq!(
        parsed.to_bytes().unwrap(),
        bytes,
        "the v1 body must round-trip byte-exact and unmodified"
    );
}

#[test]
fn from_slice_matches_the_shared_parse_entry_point() {
    use rs_io::Parse;
    let bytes = synthetic_v2_bytes();
    assert_eq!(
        Inibin::from_slice(&bytes).unwrap(),
        rs_troybin::Troybin::from_bytes(&bytes).unwrap(),
        "the inibin and troybin entry points are one parser"
    );
    assert!(matches!(
        Inibin::from_slice(&[9u8, 0, 0, 0]),
        Err(Error::UnsupportedVersion(9))
    ));
}

/// A v2 file with one i32 bucket (bit 0) and one strings bucket (bit 12).
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

/// A v2 file with a single fixed-point bucket (bit 2) holding the raw byte 25.
fn fixed_point_bytes() -> Vec<u8> {
    let mut bytes = vec![2u8];
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&(1u16 << 2).to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&0x0000_00F0u32.to_le_bytes());
    bytes.push(25);
    bytes
}

/// A v1 file: three header bytes, a `(hash, offset)` table, and the NUL-terminated blob.
fn synthetic_v1_bytes() -> Vec<u8> {
    let mut bytes = vec![1u8];
    bytes.extend_from_slice(&[0xDE, 0xAD, 0xBE]);
    let data = b"value_a\0value_b\0";
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0x0000_0001u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0x0000_0002u32.to_le_bytes());
    bytes.extend_from_slice(&8u32.to_le_bytes());
    bytes.extend_from_slice(data);
    bytes
}
