use std::io::Cursor;

use rs_io::{Parse, Serialize};
use rs_tex::{TexFormat, Texture};

#[test]
fn tex_header_roundtrip_uncompressed() {
    let width = 2u32;
    let height = 2u32;
    let pixels: Vec<u8> = vec![
        0x10, 0x20, 0x30, 0x40, 0x11, 0x21, 0x31, 0x41, 0x12, 0x22, 0x32, 0x42, 0x13, 0x23, 0x33,
        0x43,
    ];
    let tex = Texture::new(width, height, TexFormat::Bgra8, pixels.clone());

    let bytes = tex.to_bytes().expect("serialize");
    assert_eq!(&bytes[0..4], &[0x54, 0x45, 0x58, 0x00]);

    let back = Texture::from_reader(&mut Cursor::new(&bytes)).expect("parse");
    assert_eq!(back.width, width);
    assert_eq!(back.height, height);
    assert_eq!(back.format, TexFormat::Bgra8);
    assert!(!back.has_mipmaps);
    assert_eq!(back.mips.len(), 1);
    assert_eq!(back.mips[0], pixels);

    let reserialized = back.to_bytes().expect("reserialize");
    assert_eq!(reserialized, bytes);
}

#[test]
fn bad_magic_is_err() {
    let bytes = [0u8; 12];
    let res = Texture::from_reader(&mut Cursor::new(&bytes));
    assert!(res.is_err());
}

#[test]
fn decode_uncompressed_reorders_channels() {
    let tex = Texture::new(1, 1, TexFormat::Bgra8, vec![0x01, 0x02, 0x03, 0x04]);
    let img = tex.decode_rgba().expect("decode");
    assert_eq!(img.dimensions(), (1, 1));
    let px = img.get_pixel(0, 0).0;
    assert_eq!(px, [0x03, 0x02, 0x01, 0x04]);
}

#[test]
fn decode_bc1_solid_block() {
    let color0: u16 = 0xF800;
    let mut block = Vec::with_capacity(8);
    block.extend_from_slice(&color0.to_le_bytes());
    block.extend_from_slice(&color0.to_le_bytes());
    block.extend_from_slice(&[0u8; 4]);

    let tex = Texture::new(4, 4, TexFormat::Bc1, block);
    let img = tex.decode_rgba().expect("decode bc1");
    assert_eq!(img.dimensions(), (4, 4));

    for px in img.pixels() {
        let [r, g, b, a] = px.0;
        assert_eq!(a, 255, "bc1 opaque alpha");
        assert!(r > 200, "expected dominant red, got {r}");
        assert!(g < 60 && b < 60, "expected low green/blue, got {g}/{b}");
    }
}
