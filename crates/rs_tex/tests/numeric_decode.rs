use ddsfile::{AlphaMode, D3D10ResourceDimension, Dds, DxgiFormat, NewDxgiParams};
use rs_io::{Parse, Serialize};
use rs_tex::{TexFormat, Texture, read_dds_bytes, read_dds_faces_bytes};

fn dds(format: DxgiFormat, width: u32, height: u32, blocks: &[u8]) -> Vec<u8> {
    let mut dds = Dds::new_dxgi(NewDxgiParams {
        height,
        width,
        depth: None,
        format,
        mipmap_levels: None,
        array_layers: None,
        caps2: None,
        is_cubemap: false,
        resource_dimension: D3D10ResourceDimension::Texture2D,
        alpha_mode: AlphaMode::Opaque,
    })
    .unwrap();
    dds.data = blocks.to_vec();
    let mut bytes = Vec::new();
    dds.write(&mut bytes).unwrap();
    bytes
}

fn constant_block(x: i8, y: i8) -> [u8; 16] {
    let mut block = [0; 16];
    block[..2].fill(x as u8);
    block[8..10].fill(y as u8);
    block
}

#[test]
fn dds_signed_zero_is_a_flat_normal_and_unsigned_zero_is_not() {
    let block = [0; 16];
    let signed = dds(DxgiFormat::BC5_SNorm, 4, 4, &block);
    let image = read_dds_bytes(&signed).unwrap();
    assert!(image.pixels().all(|p| p.0 == [128, 128, 255, 255]));
    assert_eq!(read_dds_faces_bytes(&signed).unwrap(), [image]);
    for format in [DxgiFormat::BC5_UNorm, DxgiFormat::BC5_Typeless] {
        let image = read_dds_bytes(&dds(format, 4, 4, &block)).unwrap();
        assert!(image.pixels().all(|p| p.0 == [0, 0, 127, 255]));
    }
    assert!(Texture::from_dds_bytes(&signed).is_err());
}

#[test]
fn signed_endpoints_clamp_minus_128_and_preserve_channel_order() {
    for (x, y, expected) in [
        (-128, 0, [0, 128, 128, 255]),
        (-127, 0, [0, 128, 128, 255]),
        (0, 127, [128, 255, 128, 255]),
        (127, -127, [255, 0, 128, 255]),
    ] {
        let bytes = dds(DxgiFormat::BC5_SNorm, 4, 4, &constant_block(x, y));
        assert!(
            read_dds_bytes(&bytes)
                .unwrap()
                .pixels()
                .all(|p| p.0 == expected)
        );
    }
}

#[test]
fn signed_palette_uses_signed_ordering_and_both_interpolation_modes() {
    for (endpoints, values) in [
        ([35i8, -35], [35, -35, 25, 15, 5, -5, -15, -25]),
        ([-25, 25], [-25, 25, -15, -5, 5, 15, -127, 127]),
    ] {
        let mut block = constant_block(0, 0);
        block[0] = endpoints[0] as u8;
        block[1] = endpoints[1] as u8;
        let indices = (0..16).fold(0u64, |bits, i| bits | ((i % 8) << (i * 3)));
        block[2..8].copy_from_slice(&indices.to_le_bytes()[..6]);
        let image = read_dds_bytes(&dds(DxgiFormat::BC5_SNorm, 4, 4, &block)).unwrap();
        for (i, pixel) in image.pixels().enumerate() {
            let x = values[i % 8] as f64 / 127.0;
            let r = ((x + 1.0) * 127.5).round() as u8;
            let b = (((1.0 - x * x).sqrt() + 1.0) * 127.5).round() as u8;
            assert_eq!(pixel[0], r, "index {i}");
            assert_eq!(pixel[1], 128);
            assert!((pixel[2] as i16 - b as i16).abs() <= 1);
            assert_eq!(pixel[3], 255);
        }
    }
}

#[test]
fn signed_blocks_crop_at_partial_edges_in_both_dimensions() {
    let blocks: Vec<_> = [(0, 0), (127, 0), (0, -127), (-127, 127)]
        .into_iter()
        .flat_map(|(x, y)| constant_block(x, y))
        .collect();
    let image = read_dds_bytes(&dds(DxgiFormat::BC5_SNorm, 5, 7, &blocks)).unwrap();
    assert_eq!(image.dimensions(), (5, 7));
    for (x, y, pixel) in image.enumerate_pixels() {
        let expected = match (x / 4, y / 4) {
            (0, 0) => [128, 128, 255, 255],
            (1, 0) => [255, 128, 128, 255],
            (0, 1) => [128, 0, 128, 255],
            (1, 1) => [0, 255, 128, 255],
            _ => unreachable!(),
        };
        assert_eq!(pixel.0, expected);
    }
}

#[test]
fn signed_tiny_mips_decode_and_truncated_blocks_fail() {
    for (width, height) in [(1u32, 1u32), (2, 3), (4, 4), (5, 7)] {
        let size = width.div_ceil(4) * height.div_ceil(4) * 16;
        let blocks = vec![0; size as usize];
        let bytes = dds(DxgiFormat::BC5_SNorm, width, height, &blocks);
        assert_eq!(
            read_dds_bytes(&bytes).unwrap().dimensions(),
            (width, height)
        );
        assert!(read_dds_bytes(&bytes[..bytes.len() - 1]).is_err());
    }
}

#[test]
fn tex_bc5_keeps_unsigned_decoding_and_exact_payload_roundtrip() {
    let texture = Texture::new(4, 4, TexFormat::Bc5, constant_block(-128, -128).to_vec());
    let bytes = texture.to_bytes().unwrap();
    let parsed = Texture::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
    assert!(
        parsed
            .decode_rgba()
            .unwrap()
            .pixels()
            .all(|p| p.0 == [128, 128, 254, 255])
    );
}

#[test]
fn format21_preserves_bytes_and_current_snorm_behavior() {
    let data: Vec<_> = [i16::MIN, -i16::MAX, 0, i16::MAX]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect();
    let texture = Texture::new(1, 1, TexFormat::Rgba16Snorm, data);
    let bytes = texture.to_bytes().unwrap();
    let parsed = Texture::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
    assert_eq!(
        parsed.decode_rgba().unwrap().get_pixel(0, 0).0,
        [0, 0, 128, 255]
    );
}
