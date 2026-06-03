use std::path::{Path, PathBuf};

use rs_io::{Parse, Serialize};
use rs_tex::{TexFormat, Texture, read_dds_bytes};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-files")
}

fn read_sample(name: &str) -> Option<Vec<u8>> {
    let path = sample_dir().join(name);
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!(
                "skipping {name}: sample file not present at {}",
                path.display()
            );
            None
        }
    }
}

fn save_png(img: &image::RgbaImage, stem: &str) {
    let out = std::env::temp_dir().join(format!("rs_tex_{stem}.png"));
    let _ = img.save(&out);
}

const TEX_SAMPLES: &[&str] = &[
    "aatrox_base_sword_tx_cm.tex",
    "aatrox_circle.tex",
    "aatrox_wings_tx_cm.tex",
];

const DDS_SAMPLES: &[&str] = &[
    "aatrox_cubemap.dds",
    "aatrox_q.dds",
    "icons_ahri_e.dds",
    "kayle_p.dds",
];

#[test]
fn tex_samples_parse_decode_and_roundtrip() {
    for name in TEX_SAMPLES {
        let Some(bytes) = read_sample(name) else {
            continue;
        };

        let tex =
            Texture::from_bytes(&bytes).unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));

        assert!(tex.width > 0, "{name}: width must be > 0");
        assert!(tex.height > 0, "{name}: height must be > 0");
        // Format byte recognized by construction (parse would have errored otherwise).
        let _: TexFormat = tex.format;

        let img = tex
            .decode_rgba()
            .unwrap_or_else(|e| panic!("{name}: decode failed: {e}"));
        assert_eq!(
            img.dimensions(),
            (tex.width, tex.height),
            "{name}: decoded dims must match header"
        );

        let stem = name.trim_end_matches(".tex");
        save_png(&img, stem);

        let reserialized = tex
            .to_bytes()
            .unwrap_or_else(|e| panic!("{name}: serialize failed: {e}"));
        assert_eq!(
            reserialized, bytes,
            "{name}: round-trip (to_bytes) must be byte-identical to the original"
        );

        eprintln!(
            "{name}: {}x{} format={:?} ({}) mipmaps={} decode=OK roundtrip=OK",
            tex.width,
            tex.height,
            tex.format,
            tex.format.to_u8(),
            tex.has_mipmaps,
        );
    }
}

#[test]
fn dds_samples_parse_and_decode() {
    for name in DDS_SAMPLES {
        let Some(bytes) = read_sample(name) else {
            continue;
        };

        let img =
            read_dds_bytes(&bytes).unwrap_or_else(|e| panic!("{name}: dds decode failed: {e}"));
        assert!(
            img.width() > 0 && img.height() > 0,
            "{name}: dims must be > 0"
        );

        let stem = name.trim_end_matches(".dds");
        save_png(&img, stem);

        eprintln!("{name}: {}x{} decode=OK", img.width(), img.height(),);
    }
}
