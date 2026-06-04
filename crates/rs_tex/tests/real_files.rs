use std::path::{Path, PathBuf};

use rs_io::{Parse, Serialize};
use rs_tex::{
    TexFormat, Texture, dds_is_cubemap, read_dds_bytes, read_dds_faces_bytes, write_dds_bytes,
    write_dds_bytes_bc,
};

/// Mean absolute per-channel difference between two equally sized RGBA images.
fn mean_abs_diff(a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions());
    let (ar, br) = (a.as_raw(), b.as_raw());
    let total: u64 = ar.iter().zip(br).map(|(x, y)| x.abs_diff(*y) as u64).sum();
    total as f64 / ar.len() as f64
}

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
fn tex_samples_reencode_same_format_stays_close() {
    for name in TEX_SAMPLES {
        let Some(bytes) = read_sample(name) else {
            continue;
        };
        let tex = Texture::from_bytes(&bytes).unwrap_or_else(|e| panic!("{name}: parse: {e}"));
        let original = tex
            .decode_rgba()
            .unwrap_or_else(|e| panic!("{name}: decode: {e}"));

        // Re-encode with the SAME BC format the file already uses, then decode again.
        let format = tex.format;
        if !matches!(format, TexFormat::Bc1 | TexFormat::Bc1Alt | TexFormat::Bc3) {
            continue;
        }
        let reencoded = Texture::encode(&original, format, tex.has_mipmaps)
            .unwrap_or_else(|e| panic!("{name}: encode: {e}"));

        // The encoder must produce a structurally valid .tex our own reader parses back, with the
        // header fields and (for mipmapped inputs) the same mip count as the original chain.
        let tex_bytes = reencoded
            .to_bytes()
            .unwrap_or_else(|e| panic!("{name}: encode to_bytes: {e}"));
        let parsed =
            Texture::from_bytes(&tex_bytes).unwrap_or_else(|e| panic!("{name}: reparse: {e}"));
        assert_eq!(parsed.width, tex.width, "{name}: width");
        assert_eq!(parsed.height, tex.height, "{name}: height");
        assert_eq!(parsed.format, format, "{name}: format");
        assert_eq!(parsed.has_mipmaps, tex.has_mipmaps, "{name}: mip flag");
        if tex.has_mipmaps {
            assert_eq!(parsed.mips.len(), tex.mips.len(), "{name}: mip count");
        }

        let roundtripped = parsed
            .decode_rgba()
            .unwrap_or_else(|e| panic!("{name}: redecode: {e}"));
        let diff = mean_abs_diff(&original, &roundtripped);
        assert!(
            diff < 12.0,
            "{name}: re-encode drift too high (mean abs diff {diff:.3})"
        );
        eprintln!("{name}: re-encode {format:?} mean-abs-diff={diff:.3} OK");
    }
}

#[test]
fn encode_synthetic_gradient_roundtrips_close() {
    // A smooth gradient is friendly to BC. BC1 carries no alpha, so use an opaque source for it;
    // BC3 carries a separate alpha channel, so exercise a gradient alpha there.
    let opaque = {
        let mut img = image::RgbaImage::new(64, 64);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = image::Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255]);
        }
        img
    };
    let alpha = {
        let mut img = image::RgbaImage::new(64, 64);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = image::Rgba([(x * 4) as u8, (y * 4) as u8, 128, (y * 4) as u8]);
        }
        img
    };

    for (fmt, img, thresh) in [
        (TexFormat::Bc1, &opaque, 12.0),
        (TexFormat::Bc3, &alpha, 10.0),
    ] {
        let tex = Texture::encode(img, fmt, true).expect("encode");
        let bytes = tex.to_bytes().expect("to_bytes");
        let parsed = Texture::from_bytes(&bytes).expect("reparse");
        assert_eq!(parsed.format, fmt);
        assert!(parsed.has_mipmaps);
        assert_eq!(parsed.mips.len(), 7, "64px -> 7 mip levels");
        let decoded = parsed.decode_rgba().expect("decode");
        let diff = mean_abs_diff(img, &decoded);
        assert!(diff < thresh, "{fmt:?}: gradient drift {diff:.3}");
    }
}

#[test]
fn real_tex_encodes_to_bc7_and_roundtrips_close() {
    for name in TEX_SAMPLES {
        let Some(bytes) = read_sample(name) else {
            continue;
        };
        let tex = Texture::from_bytes(&bytes).unwrap_or_else(|e| panic!("{name}: parse: {e}"));
        let original = tex
            .decode_rgba()
            .unwrap_or_else(|e| panic!("{name}: decode: {e}"));

        let encoded = Texture::encode_bc7(&original, tex.has_mipmaps)
            .unwrap_or_else(|e| panic!("{name}: bc7 encode: {e}"));
        assert_eq!(encoded.format, TexFormat::Bc7, "{name}: bc7 format byte");

        let tex_bytes = encoded
            .to_bytes()
            .unwrap_or_else(|e| panic!("{name}: bc7 to_bytes: {e}"));
        let parsed =
            Texture::from_bytes(&tex_bytes).unwrap_or_else(|e| panic!("{name}: bc7 reparse: {e}"));
        assert_eq!(parsed.width, tex.width, "{name}: bc7 width");
        assert_eq!(parsed.height, tex.height, "{name}: bc7 height");
        assert_eq!(parsed.format, TexFormat::Bc7, "{name}: bc7 reparse format");
        if tex.has_mipmaps {
            assert_eq!(parsed.mips.len(), tex.mips.len(), "{name}: bc7 mip count");
        }

        let roundtripped = parsed
            .decode_rgba()
            .unwrap_or_else(|e| panic!("{name}: bc7 redecode: {e}"));
        let diff = mean_abs_diff(&original, &roundtripped);
        assert!(
            diff < 8.0,
            "{name}: bc7 re-encode drift too high (mean abs diff {diff:.3})"
        );
        eprintln!("{name}: BC7 encode mean-abs-diff={diff:.3} OK");
    }
}

#[test]
fn encode_synthetic_gradient_bc7_roundtrips_close() {
    let mut img = image::RgbaImage::new(64, 64);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = image::Rgba([(x * 4) as u8, (y * 4) as u8, 128, (x * 4) as u8]);
    }
    let tex = Texture::encode_bc7(&img, true).expect("bc7 encode");
    let bytes = tex.to_bytes().expect("to_bytes");
    let parsed = Texture::from_bytes(&bytes).expect("reparse");
    assert_eq!(parsed.format, TexFormat::Bc7);
    assert!(parsed.has_mipmaps);
    assert_eq!(parsed.mips.len(), 7, "64px -> 7 mip levels");
    let decoded = parsed.decode_rgba().expect("decode");
    let diff = mean_abs_diff(&img, &decoded);
    assert!(diff < 6.0, "bc7 gradient drift {diff:.3}");
}

#[test]
fn compressed_dds_writes_and_reads_back() {
    let mut img = image::RgbaImage::new(32, 32);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = image::Rgba([(x * 8) as u8, (y * 8) as u8, 96, 255]);
    }
    // Each BC format must write a compressed DDS our own reader can decode back to dimensions,
    // with the decoded result close to the source (BC is lossy). BC5 is two-channel, so only its
    // R/G survive; compare just those for it.
    for (fmt, thresh, rg_only) in [
        (TexFormat::Bc1, 12.0, false),
        (TexFormat::Bc3, 12.0, false),
        (TexFormat::Bc7, 8.0, false),
        (TexFormat::Bc5, 12.0, true),
    ] {
        let dds_bytes = write_dds_bytes_bc(&img, fmt)
            .unwrap_or_else(|e| panic!("{fmt:?}: compressed dds write: {e}"));
        let back = read_dds_bytes(&dds_bytes)
            .unwrap_or_else(|e| panic!("{fmt:?}: compressed dds read: {e}"));
        assert_eq!(back.dimensions(), img.dimensions(), "{fmt:?}: dims");

        let diff = if rg_only {
            let (ar, br) = (img.as_raw(), back.as_raw());
            let mut total = 0u64;
            let mut count = 0u64;
            for (i, (a, b)) in ar.iter().zip(br).enumerate() {
                if i % 4 < 2 {
                    total += a.abs_diff(*b) as u64;
                    count += 1;
                }
            }
            total as f64 / count as f64
        } else {
            mean_abs_diff(&img, &back)
        };
        assert!(diff < thresh, "{fmt:?}: compressed dds drift {diff:.3}");
        eprintln!("{fmt:?}: compressed DDS round-trip mean-abs-diff={diff:.3} OK");
    }
}

#[test]
fn dds_writer_roundtrips_losslessly() {
    let mut img = image::RgbaImage::new(8, 8);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = image::Rgba([x as u8 * 16, y as u8 * 16, 7, 200]);
    }
    let dds_bytes = write_dds_bytes(&img).expect("write dds");
    let back = read_dds_bytes(&dds_bytes).expect("read dds");
    assert_eq!(back.dimensions(), img.dimensions());
    // Uncompressed RGBA8 DDS must round-trip exactly.
    assert_eq!(back.as_raw(), img.as_raw(), "dds writer must be lossless");
}

#[test]
fn cubemap_decodes_all_faces() {
    let Some(bytes) = read_sample("aatrox_cubemap.dds") else {
        return;
    };
    assert!(
        dds_is_cubemap(&bytes).expect("classify"),
        "aatrox_cubemap.dds must be detected as a cubemap"
    );
    let faces = read_dds_faces_bytes(&bytes).expect("decode faces");
    assert_eq!(faces.len(), 6, "a cubemap must expose six faces");
    for (i, face) in faces.iter().enumerate() {
        assert!(face.width() > 0 && face.height() > 0, "face {i} dims");
        save_png(face, &format!("aatrox_cubemap_face{i}"));
    }
    eprintln!("aatrox_cubemap.dds: decoded {} faces", faces.len());
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
