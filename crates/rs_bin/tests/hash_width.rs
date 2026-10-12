use rs_bin::{Bin, BinEntry, BinPatch, BinType as T, BinValue as V};
use rs_io::{Parse, Serialize};

fn doc(value: V) -> Bin {
    Bin {
        version: 3,
        entries: vec![BinEntry {
            path_hash: 1,
            class_hash: 2,
            fields: [(3, value)].into_iter().collect(),
        }],
        ..Bin::default()
    }
}
fn roundtrip(bin: Bin) {
    let bytes = bin.to_bytes().unwrap();
    let read = Bin::from_bytes(&bytes).unwrap();
    assert_eq!(read, bin);
    assert_eq!(read.to_bytes().unwrap(), bytes);
    let text = rs_bin::to_text(&read, None);
    assert_eq!(
        rs_bin::from_text(&text, None).unwrap().to_bytes().unwrap(),
        bytes
    );
}
#[test]
fn both_widths_and_leading_zeroes_survive_binary_and_text() {
    for value in [
        V::Hash(0),
        V::Hash(u32::MAX),
        V::Hash64(0),
        V::Hash64(7),
        V::Hash64(0x7394ac037b93317f),
        V::Hash64(u64::MAX),
    ] {
        roundtrip(doc(value));
    }
    let text = rs_bin::to_text(&doc(V::Hash64(7)), None);
    assert!(text.contains("0x0000000000000007"));
}
#[test]
fn mixed_fields_nested_structs_lists_maps_options_and_patches() {
    let nested = V::Embed {
        class: 8,
        fields: [
            (0xfefefefe, V::Hash64(0x7394ac037b93317f)),
            (0xadadadad, V::Hash(7)),
            (
                0xcdcdcdcd,
                V::Option {
                    item: T::Hash,
                    value: Some(Box::new(V::Hash64(0))),
                },
            ),
        ]
        .into_iter()
        .collect(),
    };
    roundtrip(doc(nested.clone()));
    for is_list2 in [false, true] {
        roundtrip(doc(V::List {
            is_list2,
            item: T::Hash,
            items: vec![V::Hash64(0), V::Hash64(u64::MAX)],
        }));
        roundtrip(doc(V::List {
            is_list2,
            item: T::Embed,
            items: vec![nested.clone()],
        }));
    }
    roundtrip(doc(V::Map {
        key: T::Hash,
        value: T::Embed,
        entries: vec![(V::Hash64(4), nested)],
    }));
    roundtrip(doc(V::Map {
        key: T::U32,
        value: T::Hash,
        entries: vec![(V::U32(1), V::Hash64(5))],
    }));
    roundtrip(doc(V::Map {
        key: T::Hash,
        value: T::Hash,
        entries: vec![(V::Hash64(1), V::Hash64(2))],
    }));
    let mut patch = doc(V::Hash(1));
    patch.is_patch = true;
    patch.patch_header[0] = 1;
    patch.patches.push(BinPatch {
        key_hash: 4,
        path: "name".into(),
        value: V::Hash64(7),
    });
    roundtrip(patch);
}
#[test]
fn writer_rejects_mixed_columns_and_ambiguous_maps() {
    assert!(
        doc(V::List {
            is_list2: false,
            item: T::Hash,
            items: vec![V::Hash(1), V::Hash64(2)]
        })
        .to_bytes()
        .is_err()
    );
    assert!(
        doc(V::Map {
            key: T::Hash,
            value: T::Hash,
            entries: vec![(V::Hash(1), V::Hash64(2))]
        })
        .to_bytes()
        .is_err()
    );
}
#[test]
fn hand_encoded_wide_scalar_and_truncated_region() {
    let mut bytes = b"PROP".to_vec();
    for v in [3u32, 0, 1, 2, 19, 1] {
        bytes.extend(v.to_le_bytes());
    }
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(3u32.to_le_bytes());
    bytes.push(17);
    bytes.extend(0x7394ac037b93317fu64.to_le_bytes());
    let parsed = Bin::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.entries[0].fields[&3], V::Hash64(0x7394ac037b93317f));
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
    bytes.pop();
    assert!(Bin::from_bytes(&bytes).is_err());
}
#[test]
#[ignore = "set RS_BIN_WIDE_SAMPLE to a PBE material BIN"]
fn pbe_material_binary_and_text_roundtrip() {
    let path = std::env::var("RS_BIN_WIDE_SAMPLE").expect("RS_BIN_WIDE_SAMPLE");
    let bytes = std::fs::read(path).unwrap();
    let bin = Bin::from_bytes(&bytes).unwrap();
    let text = rs_bin::to_text(&bin, None);
    assert!(
        bin.entries
            .iter()
            .flat_map(|e| e.fields.values())
            .any(|v| matches!(v, V::Hash64(_)))
    );
    assert_eq!(bin.to_bytes().unwrap(), bytes);
    assert_eq!(
        rs_bin::from_text(&text, None).unwrap().to_bytes().unwrap(),
        bytes
    );
}
