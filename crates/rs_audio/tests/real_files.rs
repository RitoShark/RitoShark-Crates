use std::path::{Path, PathBuf};

use rs_audio::{Bnk, Wpk};
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

/* Banks pulled from the live game across both header revisions in circulation. Version 134 is
older shipped content and 145 is current; the init bank is the only one carrying STID/STMG/ENVS/
PLAT, so it is the one file that proves unknown sections survive a round-trip untouched. */

#[test]
fn bank_v145_audio_round_trips() {
    check_round_trip("bank_v145_audio.bnk");
}

#[test]
fn bank_v134_audio_round_trips() {
    check_round_trip("bank_v134_audio.bnk");
}

#[test]
fn bank_v145_events_round_trips() {
    check_round_trip("bank_v145_events.bnk");
}

#[test]
fn bank_v134_events_round_trips() {
    check_round_trip("bank_v134_events.bnk");
}

#[test]
fn bank_v145_bare_round_trips() {
    check_round_trip("bank_v145_bare.bnk");
}

#[test]
fn bank_v134_bare_round_trips() {
    check_round_trip("bank_v134_bare.bnk");
}

#[test]
fn bank_v145_init_round_trips() {
    let Some(path) = sample("bank_v145_init.bnk") else {
        eprintln!("skipping: bank_v145_init.bnk missing");
        return;
    };
    check_round_trip("bank_v145_init.bnk");

    let bnk = Bnk::from_path(&path).unwrap();
    let section_tags = tags(&bnk);
    for expected in ["BKHD", "INIT", "STMG", "ENVS", "PLAT"] {
        assert!(
            section_tags.iter().any(|t| t == expected),
            "init bank should carry {expected}, got {section_tags:?}"
        );
    }
}

/* Real `.wpk` packages. Until these landed the WPK writer was validated by synthetic data only,
and it was wrong: Riot pads the offset table, every entry record and every audio blob up to an
eight-byte boundary, which the previous per-entry alignment model mis-attributed. */

fn check_wpk_round_trip(name: &str) {
    let Some(path) = sample(name) else {
        eprintln!("skipping {name}: sample file missing");
        return;
    };

    let original = std::fs::read(&path).expect("read sample bytes");
    let wpk = Wpk::from_path(&path).expect("parse wpk");

    let written = wpk.to_bytes().expect("serialize wpk");
    assert_eq!(
        written.len(),
        original.len(),
        "{name}: re-serialized length differs ({} entries, {} dead slots)",
        wpk.entries.len(),
        wpk.dead_slots.len()
    );
    assert!(
        written == original,
        "{name}: round-trip not byte-exact at offset {:?}",
        written.iter().zip(&original).position(|(a, b)| a != b)
    );

    for (id, entry_name, bytes) in wpk.wems() {
        assert!(
            !bytes.is_empty(),
            "{name}: entry {entry_name} (id {id:?}) has an empty body"
        );
    }

    eprintln!(
        "{name}: entries={} dead_slots={} round_trip=OK",
        wpk.entries.len(),
        wpk.dead_slots.len()
    );
}

#[test]
fn real_wpk_37_entries_round_trips() {
    check_wpk_round_trip("audio_package_37.wpk");
}

#[test]
fn real_wpk_4_entries_round_trips() {
    check_wpk_round_trip("audio_package_4.wpk");
}

#[test]
fn real_wpk_entries_are_wem_riff_payloads() {
    let Some(path) = sample("audio_package_4.wpk") else {
        eprintln!("skipping: audio_package_4.wpk missing");
        return;
    };
    let wpk = Wpk::from_path(&path).unwrap();
    assert!(!wpk.entries.is_empty());

    for (id, name, bytes) in wpk.wems() {
        assert!(
            id.is_some(),
            "League names package entries '<id>.wem'; got {name:?}"
        );
        assert_eq!(
            &bytes[..4],
            b"RIFF",
            "{name}: package payload should be a RIFF wem"
        );
    }
}
