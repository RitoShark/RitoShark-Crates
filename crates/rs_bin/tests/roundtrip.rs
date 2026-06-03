use indexmap::IndexMap;
use rs_bin::{Bin, BinEntry, BinType, BinValue};
use rs_io::{Parse, Serialize};

/// A hand-built PROP buffer: version 3, one linked file, two entries exercising u32, a u16 list,
/// a nested embed (with an f32 field), and a string. Sizes below are computed by hand so the test
/// also pins the exact on-disk layout, not just self-consistency.
fn sample_prop() -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"PROP");
    b.extend_from_slice(&3u32.to_le_bytes()); // version

    // linked files: count 1
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&(10u16).to_le_bytes());
    b.extend_from_slice(b"data/x.bin");

    // entry type list: 2 classes
    b.extend_from_slice(&2u32.to_le_bytes());
    b.extend_from_slice(&0x1111_1111u32.to_le_bytes());
    b.extend_from_slice(&0x2222_2222u32.to_le_bytes());

    // --- entry 1 ---
    b.extend_from_slice(&35u32.to_le_bytes()); // length
    b.extend_from_slice(&0x0A0A_0A0Au32.to_le_bytes()); // path hash
    b.extend_from_slice(&2u16.to_le_bytes()); // field count
    // field a: u32 = 0xDEADBEEF
    b.extend_from_slice(&0xAAAA_AAAAu32.to_le_bytes());
    b.push(BinType::U32.to_u8());
    b.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
    // field b: list[u16] { 1, 2, 3 }
    b.extend_from_slice(&0xBBBB_BBBBu32.to_le_bytes());
    b.push(BinType::List.to_u8());
    b.push(BinType::U16.to_u8());
    b.extend_from_slice(&10u32.to_le_bytes()); // list size (count + items)
    b.extend_from_slice(&3u32.to_le_bytes()); // list count
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());

    // --- entry 2 ---
    b.extend_from_slice(&39u32.to_le_bytes()); // length
    b.extend_from_slice(&0x0B0B_0B0Bu32.to_le_bytes()); // path hash
    b.extend_from_slice(&2u16.to_le_bytes()); // field count
    // field c: embed 0x33333333 { d: f32 = 1.5 }
    b.extend_from_slice(&0xCCCC_CCCCu32.to_le_bytes());
    b.push(BinType::Embed.to_u8());
    b.extend_from_slice(&0x3333_3333u32.to_le_bytes()); // class
    b.extend_from_slice(&11u32.to_le_bytes()); // embed size (fieldcount + field)
    b.extend_from_slice(&1u16.to_le_bytes()); // field count
    b.extend_from_slice(&0xDDDD_DDDDu32.to_le_bytes());
    b.push(BinType::F32.to_u8());
    b.extend_from_slice(&1.5f32.to_le_bytes());
    // field e: string "hi"
    b.extend_from_slice(&0xEEEE_EEEEu32.to_le_bytes());
    b.push(BinType::String.to_u8());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(b"hi");

    b
}

#[test]
fn binary_round_trip_is_byte_exact() {
    let bytes = sample_prop();
    let bin = Bin::from_bytes(&bytes).expect("parse");
    let out = bin.to_bytes().expect("serialize");
    assert_eq!(out, bytes, "round-trip must be byte-identical");
}

#[test]
fn parsed_structure_matches_expectations() {
    let bin = Bin::from_bytes(&sample_prop()).expect("parse");
    assert!(!bin.is_patch);
    assert_eq!(bin.version, 3);
    assert_eq!(bin.linked, vec!["data/x.bin".to_string()]);
    assert_eq!(bin.entries.len(), 2);

    let e0 = &bin.entries[0];
    assert_eq!(e0.path_hash, 0x0A0A_0A0A);
    assert_eq!(e0.class_hash, 0x1111_1111);
    assert_eq!(e0.fields.get(&0xAAAA_AAAA), Some(&BinValue::U32(0xDEAD_BEEF)));
    match e0.fields.get(&0xBBBB_BBBB) {
        Some(BinValue::List {
            is_list2,
            item,
            items,
        }) => {
            assert!(!is_list2);
            assert_eq!(*item, BinType::U16);
            assert_eq!(
                items,
                &vec![BinValue::U16(1), BinValue::U16(2), BinValue::U16(3)]
            );
        }
        other => panic!("expected list, got {other:?}"),
    }

    let e1 = &bin.entries[1];
    match e1.fields.get(&0xCCCC_CCCC) {
        Some(BinValue::Embed { class, fields }) => {
            assert_eq!(*class, 0x3333_3333);
            assert_eq!(fields.get(&0xDDDD_DDDD), Some(&BinValue::F32(1.5)));
        }
        other => panic!("expected embed, got {other:?}"),
    }
    assert_eq!(
        e1.fields.get(&0xEEEE_EEEE),
        Some(&BinValue::String("hi".to_string()))
    );
}

#[test]
fn patch_header_round_trips() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PTCH");
    bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    bytes.extend_from_slice(b"PROP");
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes()); // linked count
    bytes.extend_from_slice(&0u32.to_le_bytes()); // entry count

    let bin = Bin::from_bytes(&bytes).expect("parse");
    assert!(bin.is_patch);
    assert_eq!(bin.patch_header, [1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(bin.to_bytes().expect("serialize"), bytes);
}

#[test]
fn null_pointer_round_trips() {
    let mut fields = IndexMap::new();
    fields.insert(
        0x1234_5678u32,
        BinValue::Pointer {
            class: 0,
            fields: IndexMap::new(),
        },
    );
    let bin = Bin {
        is_patch: false,
        patch_header: [0; 8],
        version: 3,
        linked: Vec::new(),
        entries: vec![BinEntry {
            path_hash: 1,
            class_hash: 2,
            fields,
        }],
    };
    let bytes = bin.to_bytes().expect("serialize");
    let reparsed = Bin::from_bytes(&bytes).expect("parse");
    assert_eq!(reparsed, bin);
    assert_eq!(reparsed.to_bytes().expect("serialize"), bytes);
}

#[test]
fn text_printer_emits_header_and_fields() {
    let bin = Bin::from_bytes(&sample_prop()).expect("parse");
    let text = rs_bin::to_text(&bin, None);
    assert!(text.starts_with("#PROP_text\n"));
    assert!(text.contains("version: 3"));
    assert!(text.contains("0xaaaaaaaa: u32 = 3735928559"));
    assert!(text.contains("list[u16]"));
}
