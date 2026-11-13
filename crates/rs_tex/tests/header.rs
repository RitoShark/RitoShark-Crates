use std::io::{Cursor, Read, Seek, SeekFrom};

use rs_io::{Parse, Serialize};
use rs_tex::{TexFormat, Texture, TextureContainer, TextureHeader};

fn tex(format: u8, mips: bool) -> Vec<u8> {
    let mut bytes = b"TEX\0".to_vec();
    bytes.extend_from_slice(&1024u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&[1, format, 0, u8::from(mips)]);
    bytes
}

fn dds(fourcc: [u8; 4]) -> Vec<u8> {
    let mut bytes = vec![0; 128];
    for (offset, value) in [
        (4, 124u32),
        (8, 0x21007),
        (12, 64),
        (16, 128),
        (28, 4),
        (76, 32),
        (80, 4),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[..4].copy_from_slice(b"DDS ");
    bytes[84..88].copy_from_slice(&fourcc);
    bytes
}

fn dx10(code: u32) -> Vec<u8> {
    let mut bytes = dds(*b"DX10");
    for value in [code, 3, 0, 1, 0] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

struct HeaderOnlyReader(Cursor<Vec<u8>>);

impl Read for HeaderOnlyReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        assert!(
            self.0.position() + out.len() as u64 <= self.0.get_ref().len() as u64,
            "attempt to read beyond the header"
        );
        self.0.read(out)
    }
}

impl Seek for HeaderOnlyReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(position)
    }
}

#[test]
fn reads_only_the_header_for_all_three_layouts() {
    for bytes in [tex(12, true), dds(*b"DXT5"), dx10(98)] {
        let len = bytes.len();
        let mut reader = HeaderOnlyReader(Cursor::new(bytes));
        TextureHeader::from_reader(&mut reader).unwrap();
        assert_eq!(reader.0.position(), len as u64);
    }
}

#[test]
fn tex_metadata_does_not_guess_the_mip_chain() {
    let header = TextureHeader::from_bytes(&tex(12, true)).unwrap();
    assert_eq!(header.container, TextureContainer::Tex);
    assert_eq!((header.width, header.height), (1024, 32));
    assert_eq!(header.format, "Bc3");
    assert!(header.has_mipmaps);
    assert_eq!(header.mip_count, None);
    assert!(header.block_compressed);
    assert!(!header.unknown_format);
    let header = TextureHeader::from_bytes(&tex(20, false)).unwrap();
    assert!(!header.block_compressed);
    assert_eq!(header.mip_count, Some(1));
}

#[test]
fn unknown_formats_remain_inspectable() {
    let header = TextureHeader::from_bytes(&tex(99, false)).unwrap();
    assert!(header.unknown_format);
    assert_eq!(header.format, "format byte 99");
    let header = TextureHeader::from_bytes(&dds(*b"ABCD")).unwrap();
    assert!(header.unknown_format);
    assert_eq!(header.format, "ABCD");
    for code in [0, 123456, u32::MAX] {
        let header = TextureHeader::from_bytes(&dx10(code)).unwrap();
        assert!(header.unknown_format);
        assert_eq!(header.dxgi_format, Some(code));
        assert!(!header.block_compressed);
    }
}

#[test]
fn dds_reports_legacy_and_dx10_formats() {
    let header = TextureHeader::from_bytes(&dds(*b"DXT5")).unwrap();
    assert_eq!(header.container, TextureContainer::Dds);
    assert_eq!((header.width, header.height), (128, 64));
    assert_eq!(header.format, "DXT5");
    assert_eq!(header.mip_count, Some(4));
    assert!(header.has_mipmaps && header.block_compressed);
    for code in [70, 71, 84, 94, 99] {
        let header = TextureHeader::from_bytes(&dx10(code)).unwrap();
        assert!(header.dx10 && header.block_compressed && !header.unknown_format);
        assert_eq!(header.dxgi_format, Some(code));
    }
    assert!(
        !TextureHeader::from_bytes(&dx10(28))
            .unwrap()
            .block_compressed
    );
}

#[test]
fn rejects_every_truncated_header_including_partial_dx10_extensions() {
    for bytes in [tex(12, true), dds(*b"DXT1"), dx10(98)] {
        for end in 0..bytes.len() {
            assert!(
                TextureHeader::from_bytes(&bytes[..end]).is_err(),
                "accepted prefix {end}"
            );
        }
    }
    assert!(TextureHeader::from_bytes(b"not a texture").is_err());
}

#[test]
fn validates_dds_structure_sizes() {
    for offset in [4, 76] {
        let mut bytes = dds(*b"DXT1");
        bytes[offset..offset + 4].fill(0);
        assert!(TextureHeader::from_bytes(&bytes).is_err());
    }
}

#[test]
fn dds_mip_count_defaults_to_one_when_absent_or_zero() {
    for offset in [8, 28] {
        let mut bytes = dds(*b"DXT1");
        bytes[offset..offset + 4].fill(0);
        let header = TextureHeader::from_bytes(&bytes).unwrap();
        assert_eq!(header.mip_count, Some(1));
        assert!(!header.has_mipmaps);
    }
}

#[test]
fn dds_ignores_fourcc_bytes_when_the_fourcc_flag_is_absent() {
    let mut bytes = dds(*b"DX10");
    bytes[80..84].copy_from_slice(&0x41u32.to_le_bytes());
    for (offset, value) in [
        (88, 32u32),
        (92, 0xff0000),
        (96, 0xff00),
        (100, 0xff),
        (104, 0xff000000),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    let header = TextureHeader::from_bytes(&bytes).unwrap();
    assert!(!header.dx10 && !header.block_compressed && !header.unknown_format);
}

#[test]
fn metadata_matches_library_generated_files() {
    let texture = Texture::new(4, 2, TexFormat::Bgra8, vec![0; 32]);
    let header = TextureHeader::from_bytes(&texture.to_bytes().unwrap()).unwrap();
    assert_eq!((header.width, header.height), (4, 2));
    let image = image::RgbaImage::new(4, 2);
    let header = TextureHeader::from_bytes(&rs_tex::write_dds_bytes(&image).unwrap()).unwrap();
    assert_eq!((header.width, header.height), (4, 2));
    assert_eq!(header.mip_count, Some(1));
    assert!(!header.block_compressed && !header.unknown_format);
}
