use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rs_audio::{Bnk, HircBody};
use rs_io::Parse;

fn sample(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sample-files")
        .join(name);
    path.exists().then_some(path)
}

fn bank(name: &str) -> Option<Bnk> {
    Some(Bnk::from_path(sample(name)?).expect("parse bank"))
}

#[test]
fn real_event_banks_expose_their_hierarchy() {
    for name in [
        "aatrox_base_sfx_events.bnk",
        "bank_v145_events.bnk",
        "bank_v134_events.bnk",
        "olaf_base_vo_events.bnk",
    ] {
        let Some(bnk) = bank(name) else {
            eprintln!("skipping {name}: sample missing");
            continue;
        };

        let hirc = bnk
            .hirc()
            .expect("hierarchy must parse")
            .expect("an events bank has a HIRC section");

        assert!(!hirc.objects.is_empty(), "{name}: no objects decoded");

        let events = hirc.events().count();
        let sounds = hirc.sounds().count();
        let decoded = hirc.objects.len() - hirc.opaque_count();

        assert!(events > 0, "{name}: an events bank should contain events");
        assert!(
            decoded * 2 >= hirc.objects.len(),
            "{name}: only {decoded}/{} objects decoded — the version layout is likely wrong",
            hirc.objects.len()
        );

        eprintln!(
            "{name}: version={:?} objects={} decoded={decoded} events={events} sounds={sounds}",
            bnk.version(),
            hirc.objects.len()
        );
    }
}

/** Resolves every event in a real events bank and checks the answers against the companion audio
bank that actually holds the payloads.

This is the test that proves the traversal is right rather than merely non-crashing: the ids come
from walking one file's object graph, and they have to line up with a completely separate file's
DIDX index. */
#[test]
fn events_resolve_to_wems_that_exist_in_the_companion_audio_bank() {
    let (Some(events_bank), Some(audio_bank)) = (
        bank("aatrox_base_sfx_events.bnk"),
        bank("aatrox_base_sfx_audio.bnk"),
    ) else {
        eprintln!("skipping: aatrox sample pair missing");
        return;
    };

    let embedded: HashSet<u32> = audio_bank.wem_ids().into_iter().collect();
    assert!(!embedded.is_empty(), "audio bank should hold payloads");

    let hirc = events_bank.hirc().unwrap().unwrap();
    let map = hirc.event_wem_map();
    assert!(!map.is_empty(), "expected events");

    let mut resolved_events = 0usize;
    let mut total_ids = 0usize;
    let mut matched_ids = 0usize;

    for (event_id, wem_ids) in &map {
        if wem_ids.is_empty() {
            continue;
        }
        resolved_events += 1;
        for id in wem_ids {
            total_ids += 1;
            if embedded.contains(id) {
                matched_ids += 1;
            }
        }
        assert!(
            wem_ids.iter().collect::<HashSet<_>>().len() == wem_ids.len(),
            "event {event_id}: resolved ids must be deduplicated"
        );
    }

    assert!(
        resolved_events > 0,
        "no event resolved to any wem — the action or container walk is broken"
    );
    assert!(
        matched_ids * 2 >= total_ids,
        "only {matched_ids}/{total_ids} resolved ids exist in the companion bank — \
         the traversal is reading the wrong fields"
    );

    eprintln!(
        "aatrox: {}/{} events resolved, {matched_ids}/{total_ids} ids found in the audio bank",
        resolved_events,
        map.len()
    );
}

#[test]
fn sounds_point_at_payloads_that_exist() {
    let (Some(events_bank), Some(audio_bank)) = (
        bank("aatrox_base_sfx_events.bnk"),
        bank("aatrox_base_sfx_audio.bnk"),
    ) else {
        eprintln!("skipping: aatrox sample pair missing");
        return;
    };

    let embedded: HashSet<u32> = audio_bank.wem_ids().into_iter().collect();
    let hirc = events_bank.hirc().unwrap().unwrap();

    let sounds: Vec<_> = hirc.sounds().collect();
    assert!(!sounds.is_empty(), "expected sound objects");

    let known = sounds
        .iter()
        .filter(|s| embedded.contains(&s.source_id))
        .count();
    assert!(
        known * 2 >= sounds.len(),
        "only {known}/{} sounds name a payload in the companion bank",
        sounds.len()
    );
}

#[test]
fn a_bank_without_a_hierarchy_reports_none() {
    let Some(bnk) = bank("bank_v145_bare.bnk") else {
        eprintln!("skipping: bank_v145_bare.bnk missing");
        return;
    };
    assert!(bnk.hirc().unwrap().is_none());
}

#[test]
fn the_init_bank_hierarchy_parses() {
    let Some(bnk) = bank("bank_v145_init.bnk") else {
        eprintln!("skipping: bank_v145_init.bnk missing");
        return;
    };
    let hirc = bnk.hirc().unwrap().expect("init bank carries a HIRC");
    eprintln!(
        "init bank: objects={} opaque={}",
        hirc.objects.len(),
        hirc.opaque_count()
    );
}

#[test]
fn header_accessors_report_the_real_version_and_bank_id() {
    for (name, expected) in [
        ("bank_v145_audio.bnk", 145u32),
        ("bank_v134_audio.bnk", 134),
        ("aatrox_base_sfx_audio.bnk", 145),
    ] {
        let Some(bnk) = bank(name) else {
            eprintln!("skipping {name}: sample missing");
            continue;
        };
        assert_eq!(bnk.version(), Some(expected), "{name}: version");
        assert!(
            bnk.bank_id().is_some(),
            "{name}: bank id should be readable"
        );
        assert_ne!(bnk.bank_id(), Some(0), "{name}: bank id should be non-zero");
    }
}

#[test]
fn malformed_hierarchy_input_errs_or_degrades_instead_of_panicking() {
    use rs_audio::HircSection;

    assert!(HircSection::parse(&[], 145).is_err());
    assert!(HircSection::parse(&[0, 0], 145).is_err());

    // A count far larger than the body: parsing stops when objects run out.
    let mut hostile = Vec::new();
    hostile.extend_from_slice(&u32::MAX.to_le_bytes());
    hostile.push(2);
    hostile.extend_from_slice(&u32::MAX.to_le_bytes());
    let parsed = HircSection::parse(&hostile, 145).expect("should degrade, not fail");
    assert!(parsed.objects.is_empty());

    // A well-framed object whose body is nonsense must land as opaque, not panic.
    let mut noisy = Vec::new();
    noisy.extend_from_slice(&1u32.to_le_bytes());
    noisy.push(6); // switch container
    noisy.extend_from_slice(&8u32.to_le_bytes());
    noisy.extend_from_slice(&[0xFF; 8]);
    let parsed = HircSection::parse(&noisy, 145).unwrap();
    assert_eq!(parsed.objects.len(), 1);
    assert_eq!(parsed.objects[0].body, HircBody::Opaque);
}
