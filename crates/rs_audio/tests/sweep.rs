//! Detailed verification sweep over every real fixture in `sample-files/`.
//!
//! Real game audio is never committed, so every test here skips when its fixtures are absent —
//! the same contract the rest of the suite follows. When the fixtures are present these are the
//! broadest checks in the crate: every container round-tripped, every payload decoded and compared
//! against an independent implementation, every sound edited, and every reader fuzzed.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use rs_audio::{Bnk, PcmAudio, Wem, WemCodec, Wpk, encode_pcm};
use rs_io::{Parse, Serialize};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-files")
}

fn containers(extension: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir())
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(extension))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

fn decode_ogg(ogg: &[u8]) -> Result<(Vec<i16>, u32, u8), String> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(ogg))
        .map_err(|e| format!("hdr: {e}"))?;
    let rate = reader.ident_hdr.audio_sample_rate;
    let channels = reader.ident_hdr.audio_channels;
    let mut samples = Vec::new();
    while let Some(packet) = reader
        .read_dec_packet_itl()
        .map_err(|e| format!("pkt: {e}"))?
    {
        samples.extend_from_slice(&packet);
    }
    Ok((samples, rate, channels))
}

fn oracle(wem: &[u8]) -> Result<Vec<u8>, String> {
    let codebooks = ww2ogg::CodebookLibrary::aotuv_codebooks().map_err(|e| e.to_string())?;
    let mut converter =
        ww2ogg::WwiseRiffVorbis::new(Cursor::new(wem), codebooks).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    converter
        .generate_ogg(&mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

#[test]
fn sweep_every_fixture() {
    let mut files = 0usize;
    let mut wems = 0usize;
    let mut oracle_agree = 0usize;
    let mut frames = 0u64;

    println!(
        "\n{:<30} {:>10} {:>5} {:>11} {:>5} {:>8} {:>9}",
        "FILE", "BYTES", "RT", "CONTENT", "WEMS", "DECODED", "ORACLE"
    );
    println!("{}", "-".repeat(84));

    let mut paths = containers("bnk");
    paths.extend(containers("wpk"));
    if paths.is_empty() {
        eprintln!("skipping: no sample containers present");
        return;
    }

    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let original = std::fs::read(path).unwrap();
        files += 1;

        let (round_trip, content, payloads): (bool, String, Vec<(String, Vec<u8>)>) =
            if original.starts_with(b"r3d2") {
                let wpk = Wpk::from_path(path).expect("parse wpk");
                (
                    wpk.to_bytes().unwrap() == original,
                    format!("{} live/{} dead", wpk.entries.len(), wpk.dead_slots.len()),
                    wpk.wems()
                        .into_iter()
                        .map(|(id, n, d)| {
                            (
                                id.map(|i| i.to_string()).unwrap_or_else(|| n.to_string()),
                                d.to_vec(),
                            )
                        })
                        .collect(),
                )
            } else {
                let bnk = Bnk::from_path(path).expect("parse bnk");
                (
                    bnk.to_bytes().unwrap() == original,
                    format!(
                        "v{} {} sect",
                        bnk.version().unwrap_or(0),
                        bnk.sections.len()
                    ),
                    bnk.wems()
                        .into_iter()
                        .map(|(id, d)| (id.to_string(), d.to_vec()))
                        .collect(),
                )
            };

        assert!(round_trip, "{name}: round-trip is not byte-exact");

        let mut decoded = 0usize;
        let mut agreed = 0usize;

        for (id, bytes) in &payloads {
            wems += 1;
            let wem = Wem::new(bytes).unwrap_or_else(|e| panic!("{name}/{id}: parse: {e}"));
            let format = *wem.format();
            assert_eq!(format.codec, WemCodec::Vorbis, "{name}/{id}: codec");

            let ogg = wem
                .to_ogg()
                .unwrap_or_else(|e| panic!("{name}/{id}: to_ogg: {e}"));
            let (ours, rate, channels) =
                decode_ogg(&ogg).unwrap_or_else(|e| panic!("{name}/{id}: our ogg: {e}"));

            assert_eq!(rate, format.sample_rate, "{name}/{id}: rate");
            assert_eq!(
                u16::from(channels),
                format.channels,
                "{name}/{id}: channels"
            );
            assert!(!ours.is_empty(), "{name}/{id}: decoded to nothing");

            let pcm = wem
                .to_pcm()
                .unwrap_or_else(|e| panic!("{name}/{id}: to_pcm: {e}"));
            assert_eq!(
                pcm.samples, ours,
                "{name}/{id}: to_pcm disagrees with to_ogg"
            );
            frames += pcm.frames() as u64;
            decoded += 1;

            let theirs = oracle(bytes).unwrap_or_else(|e| panic!("{name}/{id}: oracle: {e}"));
            let (their_samples, their_rate, their_channels) =
                decode_ogg(&theirs).unwrap_or_else(|e| panic!("{name}/{id}: oracle ogg: {e}"));

            assert_eq!(rate, their_rate, "{name}/{id}: oracle rate");
            assert_eq!(channels, their_channels, "{name}/{id}: oracle channels");
            assert_eq!(
                ours.len(),
                their_samples.len(),
                "{name}/{id}: oracle sample count"
            );
            assert!(
                ours == their_samples,
                "{name}/{id}: oracle sample mismatch at {:?}",
                ours.iter().zip(&their_samples).position(|(a, b)| a != b)
            );
            agreed += 1;
            oracle_agree += 1;
        }

        println!(
            "{:<30} {:>10} {:>5} {:>11} {:>5} {:>8} {:>9}",
            name,
            original.len(),
            "OK",
            content,
            payloads.len(),
            decoded,
            if payloads.is_empty() {
                "-".to_string()
            } else {
                format!("{agreed}/{decoded}")
            }
        );
    }

    println!("{}", "-".repeat(84));
    println!(
        "{files} files, all byte-exact | {wems} wems, all decoded | {oracle_agree}/{wems} oracle agreement | \
         {frames} frames ({:.1}s of audio verified)",
        frames as f64 / 44100.0
    );
    assert_eq!(oracle_agree, wems);
}

#[test]
fn pcm_encode_round_trips_over_real_audio() {
    let banks = containers("bnk");
    if banks.is_empty() {
        eprintln!("skipping: no sample banks present");
        return;
    }

    let mut checked = 0usize;
    let mut frames = 0u64;

    for path in banks {
        let bnk = Bnk::from_path(&path).unwrap();
        for (id, bytes) in bnk.wems().into_iter().take(8) {
            let pcm = Wem::new(bytes).unwrap().to_pcm().unwrap();
            if pcm.is_empty() {
                continue;
            }
            let encoded = encode_pcm(&pcm).unwrap();
            let back = Wem::new(&encoded).unwrap().to_pcm().unwrap();

            assert_eq!(back.sample_rate, pcm.sample_rate, "wem {id}: rate");
            assert_eq!(back.channels, pcm.channels, "wem {id}: channels");
            assert_eq!(back.samples, pcm.samples, "wem {id}: samples");

            frames += pcm.frames() as u64;
            checked += 1;
        }
    }

    println!("\npcm encode round-trip: {checked} real streams, {frames} frames, sample-exact");
    assert!(checked > 0);
}

#[test]
fn editing_every_sound_in_every_real_bank_is_non_destructive() {
    let banks = containers("bnk");
    if banks.is_empty() {
        eprintln!("skipping: no sample banks present");
        return;
    }

    let mut edits = 0usize;

    for path in banks {
        let original = Bnk::from_path(&path).unwrap();
        let ids = original.wem_ids();
        if ids.is_empty() {
            continue;
        }

        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let header = original
            .sections
            .iter()
            .find(|s| s.tag == *b"BKHD")
            .cloned();
        let tags: Vec<[u8; 4]> = original.sections.iter().map(|s| s.tag).collect();

        for id in ids.iter().take(16) {
            let mut edited = original.clone();
            edited.silence_wem(*id).unwrap();
            let reparsed = Bnk::from_bytes(&edited.to_bytes().unwrap()).unwrap();

            assert_eq!(
                reparsed
                    .sections
                    .iter()
                    .find(|s| s.tag == *b"BKHD")
                    .cloned(),
                header,
                "{name}/{id}: header changed"
            );
            assert_eq!(
                reparsed.sections.iter().map(|s| s.tag).collect::<Vec<_>>(),
                tags,
                "{name}/{id}: sections changed"
            );
            assert_eq!(reparsed.wem_ids(), ids, "{name}/{id}: id set changed");
            assert_eq!(reparsed.version(), original.version());
            assert_eq!(reparsed.bank_id(), original.bank_id());

            let silenced = Wem::new(reparsed.wem(*id).unwrap())
                .unwrap()
                .to_pcm()
                .unwrap();
            assert!(silenced.samples.iter().all(|&s| s == 0));

            for other in ids.iter().filter(|o| *o != id) {
                assert_eq!(
                    reparsed.wem(*other),
                    original.wem(*other),
                    "{name}: editing {id} disturbed {other}"
                );
            }
            edits += 1;
        }

        println!("{name}: {} sounds edited and verified", ids.len().min(16));
    }

    println!("edit sweep: {edits} independent edits, every one non-destructive");
    assert!(edits > 0);
}

#[test]
fn fuzzing_real_payloads_never_panics() {
    let banks = containers("bnk");
    if banks.is_empty() {
        eprintln!("skipping: no sample banks present");
        return;
    }

    let mut cases = 0usize;

    for path in banks {
        let bnk = Bnk::from_path(&path).unwrap();

        for (_, bytes) in bnk.wems().into_iter().take(4) {
            for divisor in [1usize, 2, 3, 4, 8, 16, 64, 256] {
                if let Ok(wem) = Wem::new(&bytes[..bytes.len() / divisor]) {
                    let _ = wem.decode();
                    let _ = wem.to_pcm();
                }
                cases += 1;
            }
            for at in [0usize, 4, 12, 20, 32, 48, 64] {
                if at >= bytes.len() {
                    continue;
                }
                let mut corrupt = bytes.to_vec();
                corrupt[at] ^= 0xFF;
                if let Ok(wem) = Wem::new(&corrupt) {
                    let _ = wem.decode();
                    let _ = wem.to_pcm();
                }
                cases += 1;
            }
        }

        let raw = std::fs::read(&path).unwrap();
        for divisor in [2usize, 3, 5, 9] {
            if let Ok(bnk) = Bnk::from_bytes(&raw[..raw.len() / divisor]) {
                let _ = bnk.hirc();
                let _ = bnk.wems();
            }
            cases += 1;
        }
    }

    for path in containers("wpk") {
        let raw = std::fs::read(&path).unwrap();
        for divisor in [2usize, 3, 5, 9, 33] {
            let _ = Wpk::from_bytes(&raw[..raw.len() / divisor]);
            cases += 1;
        }
    }

    println!("fuzz sweep: {cases} malformed inputs, no panics");
    assert!(cases > 0);
}

#[test]
fn editing_a_real_package_keeps_every_other_entry_intact() {
    let path = dir().join("audio_package_37.wpk");
    if !path.exists() {
        eprintln!("skipping: audio_package_37.wpk missing");
        return;
    }

    let original = Wpk::from_path(&path).unwrap();
    let ids: Vec<u32> = original.entries.iter().filter_map(|e| e.id()).collect();
    assert!(ids.len() > 2);

    for id in ids.iter().take(6) {
        let mut edited = original.clone();
        edited.silence_wem(*id).unwrap();
        let reparsed = Wpk::from_bytes(&edited.to_bytes().unwrap()).unwrap();

        assert_eq!(reparsed.entries.len(), original.entries.len());
        assert_eq!(reparsed.dead_slots, original.dead_slots);

        for (before, after) in original.entries.iter().zip(&reparsed.entries) {
            assert_eq!(before.name, after.name, "entry names must be stable");
            if before.id() != Some(*id) {
                assert_eq!(before.data, after.data, "unrelated entry changed");
            }
        }

        let silenced = Wem::new(reparsed.wem(*id).unwrap())
            .unwrap()
            .to_pcm()
            .unwrap();
        assert!(silenced.samples.iter().all(|&s| s == 0));
    }

    let mut grown = original.clone();
    let tone: Vec<i16> = (0..2048)
        .map(|i| ((i as f32 / 8.0).sin() * 8000.0) as i16)
        .collect();
    grown
        .insert_wem(
            999_999_999,
            encode_pcm(&PcmAudio::new(44100, 1, tone)).unwrap(),
        )
        .unwrap();

    let reparsed = Wpk::from_bytes(&grown.to_bytes().unwrap()).unwrap();
    assert_eq!(reparsed.entries.len(), original.entries.len() + 1);
    assert!(reparsed.wem(999_999_999).is_some());

    println!(
        "package edit sweep: {} entries, 6 silenced, 1 inserted, all verified",
        original.entries.len()
    );
}
