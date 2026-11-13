use ddsfile::{AlphaMode, D3D10ResourceDimension, Dds, DxgiFormat, NewDxgiParams};
use rs_io::Serialize;
use rs_tex::{TexFormat, Texture, dds_surface_count, read_dds_bytes, read_dds_faces_bytes};

fn small_dds() -> Dds {
    Dds::new_dxgi(NewDxgiParams {
        width: 4,
        height: 4,
        depth: None,
        format: DxgiFormat::BC1_UNorm,
        mipmap_levels: None,
        array_layers: None,
        caps2: None,
        is_cubemap: false,
        resource_dimension: D3D10ResourceDimension::Texture2D,
        alpha_mode: AlphaMode::Opaque,
    })
    .unwrap()
}

fn bytes(dds: &Dds) -> Vec<u8> {
    let mut bytes = Vec::new();
    dds.write(&mut bytes).unwrap();
    let mut flags = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if dds.header.mip_map_count.is_some() {
        flags |= 0x20000;
    }
    if dds.header.depth.is_some() {
        flags |= 0x800000;
    }
    bytes[8..12].copy_from_slice(&flags.to_le_bytes());
    bytes
}

fn color_block(rgb565: u16) -> [u8; 8] {
    let [lo, hi] = rgb565.to_le_bytes();
    [lo, hi, lo, hi, 0, 0, 0, 0]
}

fn mipmapped_array(cubes: bool) -> Dds {
    let mut dds = small_dds();
    dds.header.width = 5;
    dds.header.height = if cubes { 5 } else { 7 };
    dds.header.mip_map_count = Some(3);
    let header = dds.header10.as_mut().unwrap();
    header.array_size = 2;
    if cubes {
        header.misc_flag = ddsfile::MiscFlag::TEXTURECUBE;
    }
    dds.data.clear();
    for index in 0..if cubes { 12 } else { 2 } {
        let color = if index % 2 == 0 { 0xf800 } else { 0x07e0 };
        for _ in 0..4 {
            dds.data.extend_from_slice(&color_block(color));
        }
        for _ in 0..2 {
            dds.data.extend_from_slice(&color_block(0x001f));
        }
    }
    dds
}

#[test]
fn rectangular_mips_and_cube_arrays_use_exact_surface_offsets() {
    for cubes in [false, true] {
        let mut dds = mipmapped_array(cubes);
        for trailer in [false, true] {
            if trailer {
                dds.data.extend_from_slice(&[0xee; 19]);
            }
            let bytes = bytes(&dds);
            let images = read_dds_faces_bytes(&bytes).unwrap();
            assert_eq!(images.len(), if cubes { 12 } else { 2 });
            assert_eq!(dds_surface_count(&bytes).unwrap() as usize, images.len());
            assert_eq!(read_dds_bytes(&bytes).unwrap(), images[0]);
            for (index, image) in images.iter().enumerate() {
                let expected = if index % 2 == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                };
                assert!(
                    image.pixels().all(|pixel| pixel.0 == expected),
                    "surface {index}"
                );
            }
        }
    }
}

#[test]
fn missing_last_mip_or_face_is_rejected_even_by_first_surface_reader() {
    for cubes in [false, true] {
        let dds = mipmapped_array(cubes);
        let bytes = bytes(&dds);
        for missing in [1, 8, 48] {
            let truncated = &bytes[..bytes.len() - missing];
            assert!(read_dds_bytes(truncated).is_err());
            assert!(read_dds_faces_bytes(truncated).is_err());
        }
    }
}

#[test]
fn dds_adoption_extracts_only_the_top_mip_and_rejects_multiple_surfaces() {
    let mut dds = mipmapped_array(false);
    assert!(Texture::from_dds_bytes(&bytes(&dds)).is_err());
    dds.header10.as_mut().unwrap().array_size = 1;
    dds.data.truncate(48);
    let texture = Texture::from_dds_bytes(&bytes(&dds)).unwrap();
    assert_eq!(texture.mips, [dds.data[..32].to_vec()]);
    assert!(!texture.has_mipmaps);
    assert_eq!(texture.to_bytes().unwrap().len(), 12 + 32);
    assert!(
        texture
            .decode_rgba()
            .unwrap()
            .pixels()
            .all(|p| p.0 == [255, 0, 0, 255])
    );
}

#[test]
fn dds_extreme_header_fields_return_errors_without_panics_or_large_allocations() {
    for case in 0..10 {
        let mut dds = small_dds();
        match case {
            0 => dds.header.width = 0,
            1 => dds.header.height = 0,
            2 => dds.header.width = u32::MAX,
            3 => dds.header.height = u32::MAX,
            4 => dds.header.mip_map_count = Some(u32::MAX),
            5 => dds.header10.as_mut().unwrap().array_size = u32::MAX,
            6 => {
                let header = dds.header10.as_mut().unwrap();
                header.array_size = u32::MAX;
                header.misc_flag = ddsfile::MiscFlag::TEXTURECUBE;
                assert!(dds_surface_count(&bytes(&dds)).is_err());
            }
            7 => dds.header10.as_mut().unwrap().array_size = 0,
            8 => dds.header.depth = Some(2),
            9 => {
                dds.header10.as_mut().unwrap().resource_dimension =
                    D3D10ResourceDimension::Texture3D
            }
            _ => unreachable!(),
        }
        let bytes = bytes(&dds);
        assert!(read_dds_bytes(&bytes).is_err(), "case {case}");
        assert!(read_dds_faces_bytes(&bytes).is_err(), "case {case}");
        assert!(Texture::from_dds_bytes(&bytes).is_err(), "case {case}");
    }
}

#[test]
fn all_tex_decoders_check_dimensions_and_payload_before_allocating() {
    for format in [
        TexFormat::Bc1,
        TexFormat::Bc1Alt,
        TexFormat::Bc3,
        TexFormat::Bc5,
        TexFormat::Bc7,
        TexFormat::Etc1,
        TexFormat::Etc2,
        TexFormat::Etc2Eac,
        TexFormat::Bgra8,
        TexFormat::Rgba16Snorm,
    ] {
        for (width, height) in [(0, 4), (4, 0), (u32::MAX, u32::MAX)] {
            assert!(
                Texture::new(width, height, format, vec![0; 16])
                    .decode_rgba()
                    .is_err(),
                "{format:?}"
            );
        }
        let expected = format.mip_size(5, 7);
        for length in [0, expected - 1] {
            assert!(
                Texture::new(5, 7, format, vec![0; length])
                    .decode_rgba()
                    .is_err(),
                "{format:?}"
            );
        }
    }
}

#[test]
fn etc2_rgb_uses_eight_byte_blocks_without_requiring_alpha_data() {
    assert_eq!(TexFormat::Etc2.bytes_per_block(), 8);
    assert_eq!(TexFormat::Etc2.mip_size(5, 7), 32);
    let texture = Texture::new(4, 4, TexFormat::Etc2, vec![0; 8]);
    let image = texture.decode_rgba().unwrap();
    assert_eq!(image.dimensions(), (4, 4));
    assert!(image.pixels().all(|pixel| pixel[3] == 255));
}

#[test]
fn dds_header_mutations_and_every_truncated_prefix_do_not_panic() {
    let valid = bytes(&mipmapped_array(true));
    for end in 0..valid.len() {
        assert!(read_dds_faces_bytes(&valid[..end]).is_err(), "prefix {end}");
    }
    for offset in 0..148 {
        for value in [0, 1, 127, 255] {
            let mut mutated = valid.clone();
            mutated[offset] = value;
            let _ = read_dds_bytes(&mutated);
            let _ = read_dds_faces_bytes(&mutated);
            let _ = Texture::from_dds_bytes(&mutated);
        }
    }
}
