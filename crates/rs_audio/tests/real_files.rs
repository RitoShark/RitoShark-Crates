use std::path::{Path, PathBuf};

use rs_audio::Bnk;
use rs_io::{Parse, Serialize};

fn sample(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sample-files")
        .join(name);
    path.exists().then_some(path)
}

fn tags(bnk: &Bnk) -> Vec<String> {
    bnk.sections
        .iter()
        .map(|s| String::from_utf8_lossy(&s.tag).into_owned())
        .collect()
}

fn check_round_trip(name: &str) {
    let Some(path) = sample(name) else {
        eprintln!("skipping {name}: sample file missing");
        return;
    };

    let original = std::fs::read(&path).expect("read sample bytes");
    let bnk = Bnk::from_path(&path).expect("parse bnk");

    let section_tags = tags(&bnk);
    assert!(
        section_tags.first().map(String::as_str) == Some("BKHD"),
        "{name}: first section should be BKHD, got {section_tags:?}"
    );

    let written = bnk.to_bytes().expect("serialize bnk");
    assert_eq!(
        written,
        original,
        "{name}: round-trip not byte-exact ({} sections: {:?})",
        section_tags.len(),
        section_tags
    );

    eprintln!(
        "{name}: sections={section_tags:?} round_trip=OK wems={}",
        bnk.wems().len()
    );
}

#[test]
fn aatrox_sfx_audio_round_trips() {
    let Some(path) = sample("aatrox_base_sfx_audio.bnk") else {
        eprintln!("skipping: aatrox_base_sfx_audio.bnk missing");
        return;
    };
    check_round_trip("aatrox_base_sfx_audio.bnk");

    let bnk = Bnk::from_path(&path).unwrap();
    let wems = bnk.wems();
    assert!(!wems.is_empty(), "audio bnk should expose embedded wems");
    for (id, bytes) in &wems {
        assert!(!bytes.is_empty(), "wem {id} should have a non-empty body");
    }
}

#[test]
fn aatrox_sfx_events_round_trips() {
    check_round_trip("aatrox_base_sfx_events.bnk");
}

#[test]
fn olaf_vo_audio_round_trips() {
    check_round_trip("olaf_base_vo_audio.bnk");
}

#[test]
fn olaf_vo_events_round_trips() {
    check_round_trip("olaf_base_vo_events.bnk");
}
