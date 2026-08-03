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
    assert_eq!(
        e0.fields.get(&0xAAAA_AAAA),
        Some(&BinValue::U32(0xDEAD_BEEF))
    );
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
    bytes.extend_from_slice(&0u32.to_le_bytes()); // patches count (PTCH always has this section)

    let bin = Bin::from_bytes(&bytes).expect("parse");
    assert!(bin.is_patch);
    assert_eq!(bin.patch_header, [1, 2, 3, 4, 5, 6, 7, 8]);
    assert!(bin.patches.is_empty());
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
        patches: Vec::new(),
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
    assert!(text.contains("version: u32 = 3"));
    assert!(text.contains("0xaaaaaaaa: u32 = 3735928559"));
    assert!(text.contains("list[u16]"));
}

#[test]
fn text_round_trip_reconstructs_bin() {
    let bin = Bin::from_bytes(&sample_prop()).expect("parse");
    let text = rs_bin::to_text(&bin, None);
    let reparsed = rs_bin::from_text(&text, None).expect("parse text");
    assert_eq!(reparsed, bin, "bin -> text -> bin must reconstruct exactly");
    assert_eq!(
        reparsed.to_bytes().expect("serialize"),
        sample_prop(),
        "text round-trip must re-serialize byte-identically"
    );
}

#[test]
fn text_printer_barewords_names_but_quotes_keys_and_hash_values() {
    // Canonical ritobin (and ltk_ritobin) render resolved *field* and *class* names as barewords,
    // but resolved *entry keys* and *hash/link values* as quoted strings. Pin all four so the
    // printer matches the canonical format, not just its own self-consistency.
    use rs_hash::fnv1a;

    let entry_key = fnv1a("Characters/Test/Root"); // a path: not a bareword
    let class = fnv1a("TestClass");
    let f_rate = fnv1a("rate");
    let f_link = fnv1a("mLink");
    let hash_value = fnv1a("SomeIdentifier"); // identifier-shaped, but a *value*: must stay quoted

    let mut mapper = rs_hash::HashMapper::new();
    for (h, name) in [
        (entry_key, "Characters/Test/Root"),
        (class, "TestClass"),
        (f_rate, "rate"),
        (f_link, "mLink"),
        (hash_value, "SomeIdentifier"),
    ] {
        mapper.insert(h as u64, name);
    }

    let mut fields = IndexMap::new();
    fields.insert(f_rate, BinValue::F32(1.5));
    fields.insert(f_link, BinValue::Hash(hash_value));
    let bin = Bin {
        is_patch: false,
        patch_header: [0; 8],
        version: 3,
        linked: Vec::new(),
        entries: vec![BinEntry {
            path_hash: entry_key,
            class_hash: class,
            fields,
        }],
        patches: Vec::new(),
    };

    let text = rs_bin::to_text(&bin, Some(&mapper));

    // Field names: bareword.
    assert!(
        text.contains("rate: f32 = 1.5"),
        "field name must be bareword:\n{text}"
    );
    assert!(
        text.contains("mLink: hash ="),
        "field name must be bareword:\n{text}"
    );
    assert!(
        !text.contains("\"rate\""),
        "field name must not be quoted:\n{text}"
    );
    // Class name: bareword.
    assert!(
        text.contains("TestClass {"),
        "class name must be bareword:\n{text}"
    );
    // Entry key: quoted (it is a path, not an identifier).
    assert!(
        text.contains("\"Characters/Test/Root\" = TestClass"),
        "entry key must be quoted:\n{text}"
    );
    // Hash value: quoted even though its name is identifier-shaped.
    assert!(
        text.contains("mLink: hash = \"SomeIdentifier\""),
        "hash value must be quoted:\n{text}"
    );

    // And it still round-trips back to the same bin (parser hashes the barewords).
    let reparsed = rs_bin::from_text(&text, None).expect("parse text");
    assert_eq!(reparsed, bin, "bin -> text(mapped) -> bin must reconstruct");
}

#[test]
fn text_round_trip_is_idempotent() {
    let bin = Bin::from_bytes(&sample_prop()).expect("parse");
    let text1 = rs_bin::to_text(&bin, None);
    let bin2 = rs_bin::from_text(&text1, None).expect("parse text");
    let text2 = rs_bin::to_text(&bin2, None);
    assert_eq!(text1, text2, "text -> bin -> text must be stable");
}

#[test]
fn text_parser_accepts_comments_and_barewords() {
    let text = "\
#PROP_text
# a comment line
version: u32 = 3
entries: map[hash,embed] = {
    0x0a0a0a0a = SomeClass {  # trailing comment
        someField: u32 = 7
        flagField: flag = true
        nested: pointer = null
    }
}
";
    let bin = rs_bin::from_text(text, None).expect("parse");
    assert_eq!(bin.version, 3);
    assert_eq!(bin.entries.len(), 1);
    let e = &bin.entries[0];
    assert_eq!(e.path_hash, 0x0a0a_0a0a);
    assert_eq!(e.class_hash, rs_hash::fnv1a("SomeClass"));
    assert_eq!(
        e.fields.get(&rs_hash::fnv1a("someField")),
        Some(&BinValue::U32(7))
    );
    assert_eq!(
        e.fields.get(&rs_hash::fnv1a("flagField")),
        Some(&BinValue::Flag(true))
    );
    assert_eq!(
        e.fields.get(&rs_hash::fnv1a("nested")),
        Some(&BinValue::Pointer {
            class: 0,
            fields: IndexMap::new()
        })
    );
}

#[test]
fn mtx44_text_round_trips() {
    // The printer emits an mtx44 as one brace holding 16 bare floats, four per
    // line (ritobin's canonical form). It must round-trip. The reader also
    // tolerates the legacy per-row-brace form a broken writer once produced.
    let m: [f32; 16] = [
        -0.9999999,
        -0.00000008742277,
        0.0,
        0.0,
        0.00000008742277,
        -0.9999999,
        0.0,
        0.0,
        0.0,
        0.0,
        0.9999999,
        0.0,
        0.0,
        -100.0,
        0.0,
        1.0,
    ];
    let mut fields = IndexMap::new();
    fields.insert(rs_hash::fnv1a("Transform"), BinValue::Mtx44(m));
    let bin = Bin {
        is_patch: false,
        patch_header: [0; 8],
        version: 3,
        linked: Vec::new(),
        entries: vec![BinEntry {
            path_hash: 0x0a0a_0a0a,
            class_hash: rs_hash::fnv1a("SomeClass"),
            fields,
        }],
        patches: Vec::new(),
    };

    let text = rs_bin::to_text(&bin, None);
    // Sanity: the printer emits the flat form (bare floats, no per-row braces).
    assert!(
        text.contains("mtx44 = {"),
        "printer should emit mtx44:\n{text}"
    );
    // The first row is bare floats, not wrapped in its own braces.
    assert!(
        text.contains("-0.9999999, -0.00000008742277, 0, 0"),
        "printer should emit flat rows:\n{text}"
    );
    assert!(
        !text.contains("{ -0.9999999"),
        "printer must NOT wrap rows in braces:\n{text}"
    );

    let reparsed = rs_bin::from_text(&text, None).expect("mtx44 text must re-parse");
    match reparsed.entries[0].fields.get(&rs_hash::fnv1a("Transform")) {
        Some(BinValue::Mtx44(got)) => assert_eq!(*got, m, "mtx44 values must survive round-trip"),
        other => panic!("expected mtx44, got {other:?}"),
    }
    assert_eq!(
        reparsed, bin,
        "bin -> text -> bin must reconstruct the matrix"
    );

    // Also accept a FLAT 16-float matrix (backward/forward compatible input).
    let flat = "\
#PROP_text
version: u32 = 3
entries: map[hash,embed] = {
    0x0a0a0a0a = SomeClass {
        Transform: mtx44 = { 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1 }
    }
}
";
    let flat_bin = rs_bin::from_text(flat, None).expect("flat mtx44 must parse");
    let identity: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    match flat_bin.entries[0].fields.get(&rs_hash::fnv1a("Transform")) {
        Some(BinValue::Mtx44(got)) => assert_eq!(*got, identity),
        other => panic!("expected mtx44, got {other:?}"),
    }
}

#[test]
fn ptch_patches_round_trip_binary_and_text() {
    // PTCH with one trailing patch record exercising the override section + its text form.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PTCH");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(b"PROP");
    bytes.extend_from_slice(&3u32.to_le_bytes()); // version
    bytes.extend_from_slice(&0u32.to_le_bytes()); // linked count
    bytes.extend_from_slice(&0u32.to_le_bytes()); // entry count
    // patches: count 1
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0xCAFE_BABEu32.to_le_bytes()); // patch key
    // body: type u32, path "a.b", value 42
    let path = b"a.b";
    let mut body = Vec::new();
    body.push(BinType::U32.to_u8());
    body.extend_from_slice(&(path.len() as u16).to_le_bytes());
    body.extend_from_slice(path);
    body.extend_from_slice(&42u32.to_le_bytes());
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&body);

    let bin = Bin::from_bytes(&bytes).expect("parse ptch");
    assert!(bin.is_patch);
    assert_eq!(bin.patches.len(), 1);
    assert_eq!(bin.patches[0].key_hash, 0xCAFE_BABE);
    assert_eq!(bin.patches[0].path, "a.b");
    assert_eq!(bin.patches[0].value, BinValue::U32(42));
    assert_eq!(
        bin.to_bytes().expect("serialize"),
        bytes,
        "ptch binary round-trip"
    );

    let text = rs_bin::to_text(&bin, None);
    assert!(text.starts_with("#PTCH_text\n"));
    assert!(text.contains("patches: map[hash,embed] = {"));
    let reparsed = rs_bin::from_text(&text, None).expect("parse ptch text");
    assert_eq!(reparsed, bin, "ptch text round-trip");
    assert_eq!(reparsed.to_bytes().expect("serialize"), bytes);
}

/// Regression test for the `read_string` O(n²) text-parse bug.
///
/// The old `next_char` called `str::from_utf8` on the *entire remaining input*
/// to decode a single character, making a text full of long strings quadratic in
/// file size — a ~5 MB VFX bin took ~20 s to text-parse. The fixed `read_string`
/// bulk-copies each run of plain bytes and only pays per-char cost on escapes.
///
/// This builds a string-heavy `Bin` (2000 long string fields, each carrying an
/// escaped quote and a `\u` unicode escape), round-trips it through
/// `to_text`/`from_text`, and asserts the result is exact. On the fixed parser it
/// finishes effectively instantly; the pre-fix parser would spin for seconds.
#[test]
fn text_parser_handles_many_and_long_strings() {
    let base = "assets/characters/foo/skins/skin99/particles/some_long_effect_name_v2";
    let mut fields = IndexMap::new();
    for i in 0..2000u32 {
        // The runtime string value (what the printer will re-emit and the parser
        // must read back): a real embedded quote and an é, plus a long path.
        let value = format!("{base}_{i}/with a \"quote\" and \u{e9} accent.dds");
        fields.insert(0xAAAA_0000 + i, BinValue::String(value));
    }
    let bin = Bin {
        version: 3,
        entries: vec![BinEntry {
            path_hash: 0x1234_5678,
            class_hash: 0x9ABC_DEF0,
            fields,
        }],
        ..Bin::default()
    };

    let text = rs_bin::to_text(&bin, None);
    let reparsed = rs_bin::from_text(&text, None).expect("parse string-heavy text");
    assert_eq!(reparsed, bin, "string-heavy text round-trip must be exact");
}

/// A representative nested emitter value: an embed holding a pointer holding a `list[pointer]` of
/// structs that each hold a `list[f32]`, plus flag/string/u8/vec3 leaves. This is the shape the
/// per-node text view is built for (see the `value_to_text` docs), so it is the shape the
/// value-level round-trip has to survive.
fn sample_emitter() -> BinValue {
    use rs_hash::fnv1a;

    let probability_table = BinValue::Pointer {
        class: fnv1a("VfxProbabilityTableData"),
        fields: IndexMap::from([(
            fnv1a("keyTimes"),
            BinValue::List {
                is_list2: false,
                item: BinType::F32,
                items: vec![BinValue::F32(0.0), BinValue::F32(1.0)],
            },
        )]),
    };
    let dynamics = BinValue::Pointer {
        class: fnv1a("VfxAnimatedVector3fVariableData"),
        fields: IndexMap::from([(
            fnv1a("probabilityTables"),
            BinValue::List {
                is_list2: false,
                item: BinType::Pointer,
                items: vec![probability_table],
            },
        )]),
    };
    let birth_velocity = BinValue::Embed {
        class: fnv1a("ValueVector3"),
        fields: IndexMap::from([
            (fnv1a("constantValue"), BinValue::Vec3([10.0, 0.0, 10.0])),
            (fnv1a("dynamics"), dynamics),
        ]),
    };
    let rate = BinValue::Embed {
        class: fnv1a("ValueFloat"),
        fields: IndexMap::from([(fnv1a("constantValue"), BinValue::F32(1.0))]),
    };

    BinValue::Pointer {
        class: fnv1a("VfxEmitterDefinitionData"),
        fields: IndexMap::from([
            (fnv1a("rate"), rate),
            (fnv1a("isSingleParticle"), BinValue::Flag(true)),
            (
                fnv1a("emitterName"),
                BinValue::String("Fresnel".to_string()),
            ),
            (fnv1a("importance"), BinValue::U8(3)),
            (fnv1a("birthVelocity"), birth_velocity),
        ]),
    }
}

/// A mapper resolving every name `sample_emitter` uses, so the printed text is the readable form the
/// editor actually shows rather than a wall of hex.
fn emitter_mapper() -> rs_hash::HashMapper {
    use rs_hash::fnv1a;
    let mut mapper = rs_hash::HashMapper::new();
    for name in [
        "VfxEmitterDefinitionData",
        "VfxAnimatedVector3fVariableData",
        "VfxProbabilityTableData",
        "ValueFloat",
        "ValueVector3",
        "rate",
        "constantValue",
        "isSingleParticle",
        "emitterName",
        "importance",
        "birthVelocity",
        "dynamics",
        "probabilityTables",
        "keyTimes",
    ] {
        mapper.insert(fnv1a(name) as u64, name);
    }
    mapper
}

#[test]
fn value_text_round_trips_a_nested_emitter() {
    let value = sample_emitter();
    let mapper = emitter_mapper();
    let text = rs_bin::value_to_text(&value, Some(&mapper));

    // The root prints its class header with no type tag and no leading indent, exactly as the
    // per-node view's mockup shows.
    assert!(
        text.starts_with("VfxEmitterDefinitionData {\n"),
        "root must print its class name header:\n{text}"
    );
    // Nested fields keep the whole-file formatting: `name: type = value`, four-space indents.
    assert!(
        text.contains("    rate: embed = ValueFloat {\n        constantValue: f32 = 1\n    }"),
        "nested embed must match the whole-file formatting:\n{text}"
    );
    assert!(
        text.contains("        dynamics: pointer = VfxAnimatedVector3fVariableData {"),
        "nested pointer must match the whole-file formatting:\n{text}"
    );
    assert!(
        text.contains("keyTimes: list[f32] = {"),
        "nested f32 list must carry its element tag:\n{text}"
    );

    // The actual round-trip. Untagged, so inference must land on pointer.
    let back = rs_bin::value_from_text(&text, Some(&mapper)).expect("value text must re-parse");
    assert_eq!(
        back, value,
        "value -> text -> value must reconstruct exactly"
    );

    // And with the root type stated explicitly, which is the path the editor takes.
    let back_as = rs_bin::value_from_text_as(&text, BinType::Pointer, Some(&mapper))
        .expect("value text must re-parse with an expected type");
    assert_eq!(back_as, value);

    // Printing is stable: text -> value -> text is a fixed point.
    assert_eq!(
        rs_bin::value_to_text(&back, Some(&mapper)),
        text,
        "value text printing must be idempotent"
    );

    // Unresolved hashes fall back to `0x%08x` and still round-trip, since the parser hashes
    // barewords and reads hex directly.
    let hex_text = rs_bin::value_to_text(&value, None);
    assert!(
        hex_text.starts_with("0x"),
        "unmapped class must print as hex:\n{hex_text}"
    );
    assert_eq!(
        rs_bin::value_from_text(&hex_text, None).expect("hex value text must re-parse"),
        value
    );
}

#[test]
fn value_from_text_reads_scalars_and_explicit_annotations() {
    // Scalar roots infer from the first token.
    assert_eq!(
        rs_bin::value_from_text("\"Fresnel\"", None).unwrap(),
        BinValue::String("Fresnel".to_string())
    );
    assert_eq!(
        rs_bin::value_from_text("true", None).unwrap(),
        BinValue::Bool(true)
    );
    assert_eq!(
        rs_bin::value_from_text("1.5", None).unwrap(),
        BinValue::F32(1.5)
    );
    assert_eq!(
        rs_bin::value_from_text("7", None).unwrap(),
        BinValue::I32(7)
    );
    assert_eq!(
        rs_bin::value_from_text("null", None).unwrap(),
        BinValue::Pointer {
            class: 0,
            fields: IndexMap::new()
        }
    );

    // The expected type wins over inference for a narrow scalar a bare literal cannot signal.
    assert_eq!(
        rs_bin::value_from_text_as("3", BinType::U8, None).unwrap(),
        BinValue::U8(3)
    );
    // An embed node stays an embed instead of being inferred as a pointer.
    let embed = rs_bin::value_from_text_as(
        "ValueFloat { constantValue: f32 = 1 }",
        BinType::Embed,
        None,
    )
    .unwrap();
    assert!(
        matches!(embed, BinValue::Embed { .. }),
        "expected type must pick embed over pointer"
    );

    // A leading annotation supplies the container element tags inference cannot recover.
    assert_eq!(
        rs_bin::value_from_text("list[f32] = { 0\n1 }", None).unwrap(),
        BinValue::List {
            is_list2: false,
            item: BinType::F32,
            items: vec![BinValue::F32(0.0), BinValue::F32(1.0)]
        }
    );
    assert_eq!(
        rs_bin::value_from_text("vec3 = { 1, 2, 3 }", None).unwrap(),
        BinValue::Vec3([1.0, 2.0, 3.0])
    );
}

#[test]
fn value_from_text_rejects_ambiguous_and_malformed_input() {
    // A brace-only root is undecidable; guessing would rewrite the node's type.
    assert!(rs_bin::value_from_text("{ 1, 2, 3 }", None).is_err());
    // Containers need their element tags even when the expected type is known.
    assert!(rs_bin::value_from_text_as("{ 1 }", BinType::List, None).is_err());
    // A declared tag contradicting the expected one is a conflict, not a silent reinterpretation.
    assert!(rs_bin::value_from_text_as("f32 = 1.5", BinType::U8, None).is_err());
    // Trailing junk must not be silently dropped: a truncated paste would apply as a valid node.
    assert!(rs_bin::value_from_text("Foo { a: f32 = 1 } leftover", None).is_err());
    // Unbalanced braces (text mid-edit) fail rather than applying a partial subtree.
    assert!(rs_bin::value_from_text("Foo { a: f32 = 1", None).is_err());
    assert!(rs_bin::value_from_text("", None).is_err());
}
