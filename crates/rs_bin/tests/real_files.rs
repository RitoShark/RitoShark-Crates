use rs_bin::Bin;
use rs_io::{Parse, Serialize};

fn sample(name: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sample-files")
        .join(name);
    p.exists().then_some(p)
}

const SAMPLES: &[&str] = &[
    "aatrox.bin",
    "aatrox_multi_skins_skin0_skins_skin2_skins_skin20_skins_skin3_skins_skin4_skins_skin40_skins_skin5_skins_skin6_skins_skin7_skins_skin8.bin",
    "aatrox_multi_skins_skin33_skins_skin34_skins_skin35_skins_skin36_skins_skin37_skins_skin38_skins_skin39.bin",
];

fn round_trip(name: &str) {
    let Some(p) = sample(name) else {
        eprintln!("skip {name}: sample file missing");
        return;
    };
    let original = std::fs::read(&p).expect("read sample bytes");
    let bin = Bin::from_path(&p)
        .unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));
    let out = bin
        .to_bytes()
        .unwrap_or_else(|e| panic!("{name}: serialize failed: {e}"));
    assert_eq!(
        out.len(),
        original.len(),
        "{name}: serialized length {} != original {}",
        out.len(),
        original.len()
    );
    if out != original {
        let first = out
            .iter()
            .zip(original.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(out.len());
        panic!("{name}: round-trip differs first at byte offset {first}");
    }
}

#[test]
fn aatrox_round_trips() {
    round_trip("aatrox.bin");
}

#[test]
fn aatrox_multi_low_round_trips() {
    round_trip(SAMPLES[1]);
}

#[test]
fn aatrox_multi_high_round_trips() {
    round_trip(SAMPLES[2]);
}

#[test]
fn all_samples_parse_and_print_text() {
    for name in SAMPLES {
        let Some(p) = sample(name) else {
            eprintln!("skip {name}: sample file missing");
            continue;
        };
        let bin = Bin::from_path(&p)
            .unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));
        let text = rs_bin::to_text(&bin, None);
        assert!(text.starts_with("#PROP_text\n"), "{name}: bad text header");
        assert!(
            text.contains(&format!("version: {}", bin.version)),
            "{name}: text missing version line"
        );
        assert!(
            text.contains("entries: map[hash,embed] = {"),
            "{name}: text missing entries section"
        );
        assert!(
            !bin.entries.is_empty(),
            "{name}: expected at least one entry"
        );
    }
}
