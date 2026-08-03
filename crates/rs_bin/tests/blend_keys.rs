use std::path::PathBuf;

use rs_bin::{Bin, BinValue, BlendKey, TextOptions, from_text, to_text, to_text_with};
use rs_hash::{HashMapper, fnv1a};
use rs_io::{Parse, Serialize};

/// Clip names verified to occur in the animation fixture, so the printer has something to resolve.
const CLIPS: &[&str] = &[
    "Idle1",
    "Idle2",
    "Run",
    "Run_Base",
    "Attack1",
    "Attack2",
    "Crit",
    "Death",
    "Laugh",
    "Taunt",
    "Spell1",
    "Spell2",
    "Spell3",
    "Spell4",
    "Channel",
    "Recall",
    "Respawn",
    "Idle1_Base",
];

fn mapper() -> HashMapper {
    let mut m = HashMapper::new();
    for name in CLIPS {
        m.insert(fnv1a(name) as u64, *name);
    }
    m.insert(fnv1a("mBlendDataTable") as u64, "mBlendDataTable");
    m
}

fn fixture() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/evelynn_skin0_animations.bin");
    p.exists().then_some(p)
}

fn blend_entries(bin: &Bin) -> Vec<(BinValue, BinValue)> {
    let field = fnv1a("mBlendDataTable");
    for entry in &bin.entries {
        if let Some(BinValue::Map { entries, .. }) = entry.fields.get(&field) {
            return entries.clone();
        }
    }
    Vec::new()
}

#[test]
fn blend_keys_round_trip_through_text_on_a_real_animation_bin() {
    let Some(path) = fixture() else {
        eprintln!("skip: tests/fixtures/evelynn_skin0_animations.bin missing");
        return;
    };
    let original = std::fs::read(&path).expect("read fixture");
    let bin = Bin::from_path(&path).expect("parse fixture");
    assert!(
        !blend_entries(&bin).is_empty(),
        "fixture has no mBlendDataTable to exercise"
    );

    let mapper = mapper();
    let opts = TextOptions { blend_keys: true };
    let text = to_text_with(&bin, Some(&mapper), &opts);

    assert!(
        text.contains("\" -> \""),
        "expected at least one transition with both clips named"
    );

    let back = from_text(&text, Some(&mapper)).expect("reparse readable text");
    assert_eq!(back, bin, "readable text did not reconstruct the same tree");
    assert_eq!(
        back.to_bytes().expect("serialize"),
        original,
        "bin -> readable text -> bin was not byte-identical"
    );
}

#[test]
fn default_text_output_does_not_move() {
    let Some(path) = fixture() else {
        eprintln!("skip: tests/fixtures/evelynn_skin0_animations.bin missing");
        return;
    };
    let bin = Bin::from_path(&path).expect("parse fixture");
    let mapper = mapper();
    let plain = to_text(&bin, Some(&mapper));
    assert_eq!(
        plain,
        to_text_with(&bin, Some(&mapper), &TextOptions::default())
    );
    assert!(
        !plain.contains(" -> "),
        "default output must stay canonical ritobin"
    );
}

const ARROW: &str = "\
#PROP_text
version: u32 = 3
entries: map[hash,embed] = {
    0x0a0a0a0a = AnimationGraphData {
        mBlendDataTable: map[u64,pointer] = {
            \"Attack1\" -> \"Laugh\" = TimeBlendData {
                mTime: f32 = 0.1
            }
            0x000000ab -> \"Death\" = TimeBlendData {}
        }
    }
}
";

/// The same table exactly as `ReadWriteBlendData.exe` writes it: `To` instead of `->`, an
/// unresolved clip spelled `Hashed:0x…`, and its watermark appended to the end of the file.
const EXE_DIALECT: &str = "\
#PROP_text
version: u32 = 3
entries: map[hash,embed] = {
    0x0a0a0a0a = AnimationGraphData {
        mBlendDataTable: map[u64,pointer] = {
            \"Attack1\" To \"Laugh\" = TimeBlendData {
                mTime: f32 = 0.1
            }
            \"Hashed:0x000000ab\" To \"Death\" = TimeBlendData {}
        }
    }
}
#GuiSai Watermark for readable uwu";

const PACKED: &str = "\
#PROP_text
version: u32 = 3
entries: map[hash,embed] = {
    0x0a0a0a0a = AnimationGraphData {
        mBlendDataTable: map[u64,pointer] = {
            6247030502030953662 = TimeBlendData {
                mTime: f32 = 0.1
            }
            737612971341 = TimeBlendData {}
        }
    }
}
";

#[test]
fn readable_packed_and_exe_dialects_all_parse_to_the_same_bin() {
    let arrow = from_text(ARROW, None).expect("parse arrow form");
    let exe = from_text(EXE_DIALECT, None).expect("parse exe form");
    let packed = from_text(PACKED, None).expect("parse packed form");
    assert_eq!(arrow, exe, "exe dialect diverged from the arrow form");
    assert_eq!(
        arrow, packed,
        "readable form diverged from the raw u64 form"
    );
}

#[test]
fn readable_keys_pack_to_the_right_word() {
    let bin = from_text(ARROW, None).expect("parse arrow form");
    let entries = blend_entries(&bin);
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0].0,
        BinValue::U64(BlendKey::from_names("Attack1", "Laugh").to_u64())
    );
    assert_eq!(
        entries[1].0,
        BinValue::U64(
            BlendKey {
                from: 0x0000_00ab,
                to: fnv1a("Death"),
            }
            .to_u64()
        )
    );
}

#[test]
fn a_plain_u64_map_key_outside_a_blend_table_still_parses() {
    let text = "\
#PROP_text
version: u32 = 3
entries: map[hash,embed] = {
    0x0a0a0a0a = SomeClass {
        someTable: map[u64,u32] = {
            18446744073709551615 = 1
            0 = 2
        }
    }
}
";
    let bin = from_text(text, None).expect("parse plain u64 keys");
    let field = fnv1a("someTable");
    let BinValue::Map { entries, .. } = &bin.entries[0].fields[&field] else {
        panic!("expected a map");
    };
    assert_eq!(entries[0].0, BinValue::U64(u64::MAX));
    assert_eq!(entries[1].0, BinValue::U64(0));
}
