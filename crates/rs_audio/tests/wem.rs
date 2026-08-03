use std::io::Cursor;
use std::path::{Path, PathBuf};

use rs_audio::{AudioFormat, Bnk, Wem, WemCodec, Wpk};
use rs_io::Parse;

fn sample(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sample-files")
        .join(name);
    path.exists().then_some(path)
}

/** Decodes an Ogg Vorbis stream to interleaved samples. Comparing decoded audio rather than
container bytes is what makes the oracle check meaningful: two correct encoders may disagree on a
vendor string or page boundaries while producing identical sound. */
fn decode_ogg(ogg: &[u8]) -> Result<(Vec<i16>, u32, u8), String> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(ogg))
        .map_err(|e| format!("ogg header: {e}"))?;

    let rate = reader.ident_hdr.audio_sample_rate;
    let channels = reader.ident_hdr.audio_channels;

    let mut samples = Vec::new();
    loop {
        match reader.read_dec_packet_itl() {
            Ok(Some(packet)) => samples.extend_from_slice(&packet),
            Ok(None) => break,
            Err(e) => return Err(format!("ogg packet: {e}")),
        }
    }
    Ok((samples, rate, channels))
}

fn ww2ogg_reference(wem: &[u8]) -> Result<Vec<u8>, String> {
    let codebooks = ww2ogg::CodebookLibrary::aotuv_codebooks()
        .map_err(|e| format!("reference codebooks: {e}"))?;
    let mut converter = ww2ogg::WwiseRiffVorbis::new(Cursor::new(wem), codebooks)
        .map_err(|e| format!("reference parse: {e}"))?;
    let mut out = Vec::new();
    converter
        .generate_ogg(&mut out)
        .map_err(|e| format!("reference convert: {e}"))?;
    Ok(out)
}

fn bank_wems(name: &str) -> Option<(Bnk, usize)> {
    let path = sample(name)?;
    let bnk = Bnk::from_path(&path).expect("parse bank");
    let count = bnk.wems().len();
    Some((bnk, count))
}

#[test]
fn every_wem_in_a_real_bank_decodes_to_playable_ogg() {
    for bank in ["bank_v145_audio.bnk", "bank_v134_audio.bnk"] {
        let Some((bnk, count)) = bank_wems(bank) else {
            eprintln!("skipping {bank}: sample missing");
            continue;
        };
        assert!(count > 0, "{bank}: expected embedded wems");

        for (id, bytes) in bnk.wems() {
            let wem = Wem::new(bytes).unwrap_or_else(|e| panic!("{bank}/{id}: parse failed: {e}"));
            assert_eq!(
                wem.format().codec,
                WemCodec::Vorbis,
                "{bank}/{id}: League banks are Wwise Vorbis"
            );

            let decoded = wem
                .decode()
                .unwrap_or_else(|e| panic!("{bank}/{id}: decode failed: {e}"));
            assert_eq!(decoded.format, AudioFormat::Ogg);
            assert_eq!(
                &decoded.data[..4],
                b"OggS",
                "{bank}/{id}: not an Ogg stream"
            );

            let (samples, rate, channels) = decode_ogg(&decoded.data)
                .unwrap_or_else(|e| panic!("{bank}/{id}: rebuilt ogg does not decode: {e}"));

            assert!(!samples.is_empty(), "{bank}/{id}: decoded to no audio");
            assert_eq!(rate, wem.format().sample_rate, "{bank}/{id}: sample rate");
            assert_eq!(
                u32::from(channels),
                u32::from(wem.format().channels),
                "{bank}/{id}: channel count"
            );
        }

        eprintln!("{bank}: {count} wems decoded and verified");
    }
}

#[test]
fn every_wem_in_a_real_package_decodes_to_playable_ogg() {
    let Some(path) = sample("audio_package_4.wpk") else {
        eprintln!("skipping: audio_package_4.wpk missing");
        return;
    };
    let wpk = Wpk::from_path(&path).expect("parse package");

    for (id, name, bytes) in wpk.wems() {
        let wem = Wem::new(bytes).unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));
        let decoded = wem
            .decode()
            .unwrap_or_else(|e| panic!("{name}: decode failed: {e}"));

        let (samples, rate, _) = decode_ogg(&decoded.data)
            .unwrap_or_else(|e| panic!("{name}: rebuilt ogg does not decode: {e}"));
        assert!(
            !samples.is_empty(),
            "{name} (id {id:?}): decoded to no audio"
        );
        assert_eq!(rate, wem.format().sample_rate);
    }

    eprintln!("audio_package_4.wpk: {} wems decoded", wpk.entries.len());
}

/** Cross-validates against the published `ww2ogg` crate, the same way `rs_bin` is cross-validated
against ritobin. Decoded samples must match exactly — this is the check that would catch a
mis-rebuilt codebook or a misread setup field, which a self-consistent round-trip cannot. */
#[test]
fn decoded_audio_matches_the_ww2ogg_oracle() {
    let mut compared = 0usize;

    for bank in [
        "bank_v145_audio.bnk",
        "bank_v134_audio.bnk",
        "aatrox_base_sfx_audio.bnk",
    ] {
        let Some((bnk, _)) = bank_wems(bank) else {
            eprintln!("skipping {bank}: sample missing");
            continue;
        };

        for (id, bytes) in bnk.wems() {
            let ours = Wem::new(bytes)
                .and_then(|w| w.to_ogg())
                .unwrap_or_else(|e| panic!("{bank}/{id}: our decode failed: {e}"));

            let theirs = match ww2ogg_reference(bytes) {
                Ok(bytes) => bytes,
                Err(e) => panic!("{bank}/{id}: oracle failed on a file we decoded: {e}"),
            };

            let (our_samples, our_rate, our_channels) =
                decode_ogg(&ours).unwrap_or_else(|e| panic!("{bank}/{id}: ours: {e}"));
            let (their_samples, their_rate, their_channels) =
                decode_ogg(&theirs).unwrap_or_else(|e| panic!("{bank}/{id}: oracle: {e}"));

            assert_eq!(our_rate, their_rate, "{bank}/{id}: sample rate disagrees");
            assert_eq!(
                our_channels, their_channels,
                "{bank}/{id}: channel count disagrees"
            );
            assert_eq!(
                our_samples.len(),
                their_samples.len(),
                "{bank}/{id}: decoded sample count disagrees"
            );
            assert!(
                our_samples == their_samples,
                "{bank}/{id}: decoded audio differs from the oracle at sample {:?}",
                our_samples
                    .iter()
                    .zip(&their_samples)
                    .position(|(a, b)| a != b)
            );

            compared += 1;
        }
    }

    if compared == 0 {
        eprintln!("skipping: no bank samples present");
    } else {
        eprintln!("oracle agreement on {compared} wems, sample for sample");
    }
}

#[test]
fn malformed_input_errs_instead_of_panicking() {
    assert!(Wem::new(&[]).is_err());
    assert!(Wem::new(b"RIFF").is_err());
    assert!(
        Wem::new(b"RIFF\x00\x00\x00\x00WAVE").is_err(),
        "no fmt chunk"
    );
    assert!(Wem::new(b"RIFX\xff\xff\xff\xffWAVE").is_err(), "big endian");
    assert!(Wem::new(b"NOPE\x00\x00\x00\x00WAVE").is_err(), "bad magic");

    // A fmt chunk claiming a length far past the end must not be trusted.
    let mut hostile = Vec::from(*b"RIFF\xff\xff\xff\xffWAVEfmt ");
    hostile.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(Wem::new(&hostile).is_err());
}

#[test]
fn a_truncated_real_wem_errs_instead_of_panicking() {
    let Some((bnk, _)) = bank_wems("bank_v145_audio.bnk") else {
        eprintln!("skipping: bank_v145_audio.bnk missing");
        return;
    };
    let Some((_, full)) = bnk.wems().first().copied() else {
        return;
    };

    for cut in [1usize, 8, 32, 64, full.len() / 3, full.len() / 2] {
        let truncated = &full[..cut.min(full.len())];
        // Parsing may succeed or fail, but decoding a truncated stream must never panic.
        if let Ok(wem) = Wem::new(truncated) {
            let _ = wem.decode();
        }
    }
}
