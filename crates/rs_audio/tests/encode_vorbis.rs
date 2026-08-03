use std::io::Cursor;
use std::path::{Path, PathBuf};

use rs_audio::{Bnk, PcmAudio, Wem, WemCodec, encode_vorbis, encode_vorbis_like};
use rs_io::Parse;

fn sample(name: &str) -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sample-files")
        .join(name);
    p.exists().then_some(p)
}

fn decode_ogg(ogg: &[u8]) -> (Vec<i16>, u32, u8) {
    let mut r = lewton::inside_ogg::OggStreamReader::new(Cursor::new(ogg)).expect("ogg header");
    let rate = r.ident_hdr.audio_sample_rate;
    let ch = r.ident_hdr.audio_channels;
    let mut s = Vec::new();
    while let Some(p) = r.read_dec_packet_itl().expect("ogg packet") {
        s.extend_from_slice(&p);
    }
    (s, rate, ch)
}

fn oracle(wem: &[u8]) -> Vec<u8> {
    let cb = ww2ogg::CodebookLibrary::aotuv_codebooks().expect("codebooks");
    let mut c = ww2ogg::WwiseRiffVorbis::new(Cursor::new(wem), cb).expect("oracle parse");
    let mut out = Vec::new();
    c.generate_ogg(&mut out).expect("oracle convert");
    out
}

fn tone(rate: u32, channels: u16, frames: usize) -> PcmAudio {
    let mut samples = Vec::with_capacity(frames * channels as usize);
    for i in 0..frames {
        for c in 0..channels {
            let t = i as f32 / rate as f32;
            let f = 440.0 * (1.0 + c as f32 * 0.5);
            samples.push(((t * f * std::f32::consts::TAU).sin() * 10000.0) as i16);
        }
    }
    PcmAudio::new(rate, channels, samples)
}

/// Correlation between two signals, to judge a lossy round-trip without demanding bit equality.
fn correlation(a: &[i16], b: &[i16]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        let (x, y) = (a[i] as f64, b[i] as f64);
        sa += x;
        sb += y;
        saa += x * x;
        sbb += y * y;
        sab += x * y;
    }
    let (n64,) = (n as f64,);
    let num = sab - sa * sb / n64;
    let den = ((saa - sa * sa / n64) * (sbb - sb * sb / n64)).sqrt();
    if den == 0.0 { 0.0 } else { num / den }
}

#[test]
fn encoded_wem_has_the_shape_league_ships() {
    for (rate, channels) in [(44100u32, 1u16), (44100, 2), (48000, 2), (22050, 1)] {
        let pcm = tone(rate, channels, rate as usize / 2);
        let bytes = encode_vorbis(&pcm, 0.4).expect("encode");

        let wem = Wem::new(&bytes).expect("our own output must parse");
        assert_eq!(
            wem.format().codec,
            WemCodec::Vorbis,
            "{rate}/{channels}: codec"
        );
        assert_eq!(wem.format().channels, channels);
        assert_eq!(wem.format().sample_rate, rate);
        assert_eq!(
            wem.format().block_align,
            0,
            "Vorbis wems carry no block align"
        );
        assert_eq!(wem.format().bits_per_sample, 0);

        eprintln!(
            "{rate} Hz {channels}ch: {} frames -> {} bytes ({:.1} kbit/s)",
            pcm.frames(),
            bytes.len(),
            bytes.len() as f64 * 8.0 / (pcm.frames() as f64 / rate as f64) / 1000.0
        );
    }
}

/** The strongest check available without the game: an independent implementation must decode a
WEM we encoded, and agree with our own decoder sample for sample. */
#[test]
fn an_independent_implementation_decodes_what_we_encode() {
    for (rate, channels) in [(44100u32, 1u16), (44100, 2), (22050, 1), (48000, 2)] {
        let pcm = tone(rate, channels, rate as usize / 2);
        let bytes = encode_vorbis(&pcm, 0.4).expect("encode");

        let ours = Wem::new(&bytes).unwrap().to_pcm().expect("our decode");
        let (theirs, their_rate, their_channels) = decode_ogg(&oracle(&bytes));

        assert_eq!(their_rate, rate, "{rate}/{channels}: oracle rate");
        assert_eq!(u16::from(their_channels), channels, "oracle channels");
        assert_eq!(
            ours.samples.len(),
            theirs.len(),
            "{rate}/{channels}: sample count"
        );
        assert!(
            ours.samples == theirs,
            "{rate}/{channels}: oracle disagrees"
        );

        eprintln!(
            "{rate} Hz {channels}ch: oracle agrees on {} samples",
            theirs.len()
        );
    }
}

#[test]
fn encoded_audio_reconstructs_the_original_signal() {
    for (rate, channels) in [(44100u32, 1u16), (44100, 2)] {
        let pcm = tone(rate, channels, rate as usize);
        let bytes = encode_vorbis(&pcm, 0.6).expect("encode");
        let back = Wem::new(&bytes).unwrap().to_pcm().expect("decode");

        assert_eq!(back.sample_rate, rate);
        assert_eq!(back.channels, channels);

        let ratio = back.frames() as f64 / pcm.frames() as f64;
        assert!((0.95..=1.05).contains(&ratio), "length drifted: {ratio}");

        // Vorbis delays the signal; align on the best offset before correlating.
        let best = (0..2048)
            .step_by(8)
            .map(|off| {
                let shifted: Vec<i16> = back.samples.iter().skip(off).copied().collect();
                correlation(&pcm.samples, &shifted)
            })
            .fold(f64::MIN, f64::max);

        eprintln!("{rate} Hz {channels}ch: best correlation {best:.4}");
        assert!(
            best > 0.9,
            "{rate}/{channels}: signal not reconstructed ({best:.4})"
        );
    }
}

#[test]
fn a_real_wem_can_be_re_encoded_using_its_own_header_template() {
    let Some(path) = sample("bank_v145_audio.bnk") else {
        eprintln!("skipping: bank_v145_audio.bnk missing");
        return;
    };
    let bank = Bnk::from_path(&path).unwrap();
    let (id, original) = bank.wems()[0];

    let pcm = Wem::new(original).unwrap().to_pcm().unwrap();
    let re_encoded = encode_vorbis_like(original, &pcm, 0.5).expect("re-encode");

    let wem = Wem::new(&re_encoded).expect("parse re-encoded");
    assert_eq!(wem.format().sample_rate, pcm.sample_rate);
    assert_eq!(wem.format().channels, pcm.channels);

    let ours = wem.to_pcm().expect("decode re-encoded");
    let (theirs, _, _) = decode_ogg(&oracle(&re_encoded));
    assert_eq!(
        ours.samples, theirs,
        "oracle must agree on the re-encoded wem"
    );

    eprintln!(
        "wem {id}: {} bytes -> {} bytes re-encoded ({} frames)",
        original.len(),
        re_encoded.len(),
        ours.frames()
    );
}

#[test]
fn degenerate_input_is_rejected() {
    assert!(encode_vorbis(&PcmAudio::new(44100, 0, vec![]), 0.5).is_err());
    assert!(encode_vorbis(&PcmAudio::new(44100, 2, vec![1, 2, 3]), 0.5).is_err());
    assert!(encode_vorbis(&PcmAudio::new(44100, 7, vec![0; 7]), 0.5).is_err());
}
