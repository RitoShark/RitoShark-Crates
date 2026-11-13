use std::io::{Cursor, Read, Seek, SeekFrom};

use rs_io::{Parse, Serialize};
use rs_tex::{Error, TexFormat, Texture};

fn tex(width: u16, height: u16, format: TexFormat, payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"TEX\0".to_vec();
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&[0, format.to_u8(), 0, 1]);
    bytes.extend_from_slice(payload);
    bytes
}

#[test]
fn narrow_full_chain_keeps_all_levels_and_round_trips() {
    let mut payload = Vec::new();
    for (i, size) in [8, 8, 8, 16, 32, 64, 128, 256].into_iter().enumerate() {
        payload.extend(vec![i as u8; size]);
    }
    for (w, h) in [(128, 4), (4, 128)] {
        let bytes = tex(w, h, TexFormat::Bc1, &payload);
        let parsed = Texture::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.mips.len(), 8);
        assert_eq!(parsed.largest_mip().unwrap(), &[7; 256]);
        assert_eq!(parsed.to_bytes().unwrap(), bytes);
    }
}

#[test]
fn undersized_chain_reports_exact_expected_and_actual_sizes() {
    let bytes = tex(4, 128, TexFormat::Bc3, &[0; 752]);
    assert!(matches!(
        Texture::from_bytes(&bytes),
        Err(Error::InvalidMipPayload {
            expected: 1040,
            actual: 752,
            levels: 8
        })
    ));
}

#[test]
fn size_matching_a_shorter_chain_is_not_silently_reinterpreted() {
    let bytes = tex(256, 64, TexFormat::Bc3, &[0; 21888]);
    assert!(matches!(
        Texture::from_bytes(&bytes),
        Err(Error::InvalidMipPayload {
            expected: 21904,
            actual: 21888,
            levels: 9
        })
    ));
}

#[test]
fn extra_payload_bytes_are_not_silently_discarded() {
    let bytes = tex(128, 4, TexFormat::Bc1, &[0; 521]);
    assert!(matches!(
        Texture::from_bytes(&bytes),
        Err(Error::InvalidMipPayload {
            expected: 520,
            actual: 521,
            levels: 8
        })
    ));
}

#[test]
fn largest_mip_extraction_returns_an_explicit_single_mip_texture() {
    let mut payload = vec![0x11; 240];
    payload.extend([0x77; 512]);
    let bytes = tex(4, 128, TexFormat::Bc3, &payload);
    let extracted = Texture::from_reader_largest_mip(&mut Cursor::new(&bytes)).unwrap();
    assert_eq!((extracted.width, extracted.height), (4, 128));
    assert!(!extracted.has_mipmaps);
    assert_eq!(extracted.mip_count(), 1);
    assert_eq!(extracted.largest_mip().unwrap(), &[0x77; 512]);
    let saved = extracted.to_bytes().unwrap();
    assert_eq!(saved.len(), 12 + 512);
    assert_eq!(saved[11], 0);
    assert_eq!(
        Texture::from_bytes(&saved).unwrap().largest_mip(),
        extracted.largest_mip()
    );
}

#[test]
fn extraction_skips_lower_mip_bytes_and_honors_the_initial_reader_position() {
    struct Tracked {
        cursor: Cursor<Vec<u8>>,
        read: usize,
    }
    impl Read for Tracked {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            let n = self.cursor.read(bytes)?;
            self.read += n;
            Ok(n)
        }
    }
    impl Seek for Tracked {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            self.cursor.seek(position)
        }
    }
    let mut bytes = vec![0; 5];
    bytes.extend(tex(128, 4, TexFormat::Bc1, &[0; 520]));
    let mut reader = Tracked {
        cursor: Cursor::new(bytes),
        read: 0,
    };
    reader.seek(SeekFrom::Start(5)).unwrap();
    Texture::from_reader_largest_mip(&mut reader).unwrap();
    assert_eq!(reader.read, 12 + 256);
}

#[test]
fn extraction_rejects_short_base_payloads_and_unsupported_resources() {
    let bytes = tex(4, 128, TexFormat::Bc3, &[0; 511]);
    assert!(Texture::from_reader_largest_mip(&mut Cursor::new(bytes)).is_err());
    for (offset, value) in [(8, 8), (10, 3), (4, 0)] {
        let mut bytes = tex(4, 128, TexFormat::Bc3, &[0; 1040]);
        bytes[offset] = value;
        assert!(Texture::from_reader_largest_mip(&mut Cursor::new(bytes)).is_err());
    }
}

#[test]
fn huge_declared_payload_is_rejected_before_allocation() {
    let bytes = tex(u16::MAX, u16::MAX, TexFormat::Rgba16Snorm, &[]);
    assert!(matches!(
        Texture::from_bytes(&bytes),
        Err(Error::InvalidMipPayload { actual: 0, .. })
    ));
    assert!(Texture::from_reader_largest_mip(&mut Cursor::new(bytes)).is_err());
}

#[test]
fn encoded_rectangular_textures_keep_full_mip_chains() {
    let image = image::RgbaImage::from_pixel(32, 4, image::Rgba([50, 100, 200, 255]));
    for format in [
        TexFormat::Bc1,
        TexFormat::Bc3,
        TexFormat::Bc5,
        TexFormat::Bc7,
    ] {
        let original = Texture::encode(&image, format, true).unwrap();
        let bytes = original.to_bytes().unwrap();
        let parsed = Texture::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.mips.len(), 6);
        assert_eq!(parsed.to_bytes().unwrap(), bytes);
        assert_eq!(parsed.decode_rgba().unwrap().dimensions(), (32, 4));
    }
}

#[test]
fn local_project_samples_report_bad_chains_but_allow_explicit_top_mip_extraction() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Sample-Files/tex-audit");
    let Ok(files) = std::fs::read_dir(&directory) else {
        eprintln!(
            "skipping: local TEX audit fixtures absent at {}",
            directory.display()
        );
        return;
    };
    let mut checked = 0;
    for file in files {
        let path = file.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("project-")
        {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            matches!(
                Texture::from_bytes(&bytes),
                Err(Error::InvalidMipPayload { .. })
            ),
            "{}",
            path.display()
        );
        let extracted = Texture::from_reader_largest_mip(&mut Cursor::new(&bytes)).unwrap();
        let expected_size = extracted.format.mip_size(extracted.width, extracted.height);
        assert_eq!(
            extracted.largest_mip().unwrap(),
            &bytes[bytes.len() - expected_size..]
        );
        assert_eq!(
            extracted.decode_rgba().unwrap().dimensions(),
            (extracted.width, extracted.height)
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "fixture directory exists but has no project samples"
    );
    eprintln!(
        "checked {checked} local malformed TEX layouts; extraction is not proof of original mip integrity"
    );
}
