use std::path::Path;

use ddsfile::{
    AlphaMode, Caps2, D3D10ResourceDimension, D3DFormat, Dds, DxgiFormat, MiscFlag, NewD3dParams,
    NewDxgiParams,
};
use image::RgbaImage;

use crate::decode::decode_block_format;
use crate::error::{Error, Result};
use crate::texture::{TexFormat, Texture};

/// A DDS pixel layout reduced to the subset this crate can decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DdsKind {
    Bc1,
    Bc2,
    Bc3,
    Bc5,
    Bc7,
    Bgra8,
    Rgba8,
}

fn classify(dds: &Dds) -> Result<DdsKind> {
    if let Some(dxgi) = dds.get_dxgi_format() {
        return match dxgi {
            DxgiFormat::BC1_Typeless | DxgiFormat::BC1_UNorm | DxgiFormat::BC1_UNorm_sRGB => {
                Ok(DdsKind::Bc1)
            }
            DxgiFormat::BC2_Typeless | DxgiFormat::BC2_UNorm | DxgiFormat::BC2_UNorm_sRGB => {
                Ok(DdsKind::Bc2)
            }
            DxgiFormat::BC3_Typeless | DxgiFormat::BC3_UNorm | DxgiFormat::BC3_UNorm_sRGB => {
                Ok(DdsKind::Bc3)
            }
            DxgiFormat::BC5_Typeless | DxgiFormat::BC5_UNorm | DxgiFormat::BC5_SNorm => {
                Ok(DdsKind::Bc5)
            }
            DxgiFormat::BC7_Typeless | DxgiFormat::BC7_UNorm | DxgiFormat::BC7_UNorm_sRGB => {
                Ok(DdsKind::Bc7)
            }
            DxgiFormat::R8G8B8A8_Typeless
            | DxgiFormat::R8G8B8A8_UNorm
            | DxgiFormat::R8G8B8A8_UNorm_sRGB
            | DxgiFormat::R8G8B8A8_UInt
            | DxgiFormat::R8G8B8A8_SNorm
            | DxgiFormat::R8G8B8A8_SInt => Ok(DdsKind::Rgba8),
            DxgiFormat::B8G8R8A8_Typeless
            | DxgiFormat::B8G8R8A8_UNorm
            | DxgiFormat::B8G8R8A8_UNorm_sRGB
            | DxgiFormat::B8G8R8X8_Typeless
            | DxgiFormat::B8G8R8X8_UNorm
            | DxgiFormat::B8G8R8X8_UNorm_sRGB => Ok(DdsKind::Bgra8),
            other => Err(Error::UnsupportedFormat(format!("dxgi {other:?}"))),
        };
    }
    if let Some(d3d) = dds.get_d3d_format() {
        return match d3d {
            D3DFormat::DXT1 => Ok(DdsKind::Bc1),
            D3DFormat::DXT2 | D3DFormat::DXT3 => Ok(DdsKind::Bc2),
            D3DFormat::DXT4 | D3DFormat::DXT5 => Ok(DdsKind::Bc3),
            D3DFormat::A8R8G8B8 | D3DFormat::X8R8G8B8 => Ok(DdsKind::Bgra8),
            D3DFormat::A8B8G8R8 | D3DFormat::X8B8G8R8 => Ok(DdsKind::Rgba8),
            other => Err(Error::UnsupportedFormat(format!("d3d {other:?}"))),
        };
    }
    Err(Error::UnsupportedFormat("dds: unknown pixel format".into()))
}

fn decode_dds_surface(dds: &Dds, data: &[u8]) -> Result<RgbaImage> {
    let width = dds.get_width();
    let height = dds.get_height();
    let (w, h) = (width as usize, height as usize);

    match classify(dds)? {
        DdsKind::Bc1 => decode_block_format(TexFormat::Bc1, width, height, data),
        DdsKind::Bc3 => decode_block_format(TexFormat::Bc3, width, height, data),
        DdsKind::Bc5 => decode_block_format(TexFormat::Bc5, width, height, data),
        DdsKind::Bgra8 => decode_block_format(TexFormat::Bgra8, width, height, data),
        DdsKind::Rgba8 => {
            let expected = w * h * 4;
            if data.len() < expected {
                return Err(Error::Decode(format!(
                    "rgba8 dds payload too small: have {}, need {expected}",
                    data.len()
                )));
            }
            RgbaImage::from_raw(width, height, data[..expected].to_vec())
                .ok_or_else(|| Error::Decode("rgba8 dds buffer mismatch".into()))
        }
        DdsKind::Bc2 => {
            let mut out = vec![0u32; w.max(1) * h.max(1)];
            texture2ddecoder::decode_bc2(data, w, h, &mut out)
                .map_err(|e| Error::Decode(e.to_string()))?;
            u32_bgra_to_image(width, height, &out)
        }
        DdsKind::Bc7 => {
            let mut out = vec![0u32; w.max(1) * h.max(1)];
            texture2ddecoder::decode_bc7(data, w, h, &mut out)
                .map_err(|e| Error::Decode(e.to_string()))?;
            u32_bgra_to_image(width, height, &out)
        }
    }
}

fn u32_bgra_to_image(width: u32, height: u32, pixels: &[u32]) -> Result<RgbaImage> {
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for &px in pixels {
        let [b, g, r, a] = px.to_le_bytes();
        rgba.extend_from_slice(&[r, g, b, a]);
    }
    RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| Error::Decode("dds buffer mismatch".into()))
}

impl Texture {
    /// Parse a DDS buffer into a [`Texture`], mapping its pixel format onto [`TexFormat`] and
    /// carrying the main image data as the single full-resolution mip. Formats with no League
    /// `.tex` equivalent (for example BC7) are rejected; decode those with [`read_dds_bytes`].
    pub fn from_dds_bytes(bytes: &[u8]) -> Result<Texture> {
        let dds = Dds::read(bytes)?;
        let width = dds.get_width();
        let height = dds.get_height();
        let format = match classify(&dds)? {
            DdsKind::Bc1 => TexFormat::Bc1,
            DdsKind::Bc3 => TexFormat::Bc3,
            DdsKind::Bc7 => TexFormat::Bc7,
            DdsKind::Bgra8 => TexFormat::Bgra8,
            other => {
                return Err(Error::UnsupportedFormat(format!(
                    "dds {other:?} has no tex equivalent"
                )));
            }
        };
        Ok(Texture::new(width, height, format, dds.data))
    }

    /// Serialize this texture's full-resolution mip into a standalone `.dds` byte buffer. The
    /// payload is decoded to RGBA8 and written as an uncompressed `A8R8G8B8` surface, so the
    /// output is a lossless representation of the decoded image that any DDS reader accepts.
    pub fn to_dds_bytes(&self) -> Result<Vec<u8>> {
        let img = self.decode_rgba()?;
        rgba_to_dds(&img)?.to_bytes()
    }

    /// Write this texture as a `.dds` file at `path` (see [`Texture::to_dds_bytes`]).
    pub fn save_dds(&self, path: impl AsRef<Path>) -> Result<()> {
        std::fs::write(path, self.to_dds_bytes()?).map_err(rs_io::Error::from)?;
        Ok(())
    }

    /// Serialize this texture's full-resolution mip into a block-compressed `.dds` byte buffer of
    /// the given `format` (BC1/BC3/BC5/BC7). The payload is decoded to RGBA8 and re-compressed into
    /// the requested format, producing a single-surface, single-mip compressed DDS that any DDS
    /// reader accepts. For lossless re-export prefer matching the texture's own format.
    pub fn to_dds_bytes_bc(&self, format: TexFormat) -> Result<Vec<u8>> {
        let img = self.decode_rgba()?;
        rgba_to_dds_bc(&img, format)?.to_bytes()
    }

    /// Write this texture as a block-compressed `.dds` file at `path` (see
    /// [`Texture::to_dds_bytes_bc`]).
    pub fn save_dds_bc(&self, path: impl AsRef<Path>, format: TexFormat) -> Result<()> {
        std::fs::write(path, self.to_dds_bytes_bc(format)?).map_err(rs_io::Error::from)?;
        Ok(())
    }
}

/** The legacy D3D9 pixel format a BC [`TexFormat`] is stored under, if it has one.

A DDS written through the DX10 extension carries the `DX10` FourCC and a 20-byte header the
D3D9-era loaders in shipping games do not parse — they read the pixel data 20 bytes short and
fail on the unknown FourCC. BC1/BC3 predate the extension and have legacy FourCCs, so they are
written the old way; BC5 and BC7 exist only in the extended table. */
fn bc_d3d_format(format: TexFormat) -> Option<D3DFormat> {
    match format {
        TexFormat::Bc1 | TexFormat::Bc1Alt => Some(D3DFormat::DXT1),
        TexFormat::Bc3 => Some(D3DFormat::DXT5),
        _ => None,
    }
}

/// Map a block-compressed [`TexFormat`] onto its DXGI equivalent for the DDS writer.
fn bc_dxgi_format(format: TexFormat) -> Result<DxgiFormat> {
    Ok(match format {
        TexFormat::Bc1 | TexFormat::Bc1Alt => DxgiFormat::BC1_UNorm,
        TexFormat::Bc3 => DxgiFormat::BC3_UNorm,
        TexFormat::Bc5 => DxgiFormat::BC5_UNorm,
        TexFormat::Bc7 => DxgiFormat::BC7_UNorm,
        other => {
            return Err(Error::UnsupportedFormat(format!(
                "compressed dds is only supported for BC1/BC3/BC5/BC7, not {other:?}"
            )));
        }
    })
}

fn fill_surface(dds: &mut Dds, payload: &[u8]) {
    if payload.len() > dds.data.len() {
        dds.data.resize(payload.len(), 0);
    }
    dds.data[..payload.len()].copy_from_slice(payload);
}

/// Build a single-surface block-compressed DDS from an RGBA8 image and a BC [`TexFormat`].
fn rgba_to_dds_bc(img: &RgbaImage, format: TexFormat) -> Result<Dds> {
    let blocks = crate::encode::compress_surface(format, img.as_raw(), img.width(), img.height())?;

    let mut dds = match bc_d3d_format(format) {
        Some(d3d) => Dds::new_d3d(NewD3dParams {
            height: img.height(),
            width: img.width(),
            depth: None,
            format: d3d,
            mipmap_levels: None,
            caps2: None,
        })?,
        None => Dds::new_dxgi(NewDxgiParams {
            height: img.height(),
            width: img.width(),
            depth: None,
            format: bc_dxgi_format(format)?,
            mipmap_levels: None,
            array_layers: None,
            caps2: None,
            is_cubemap: false,
            resource_dimension: D3D10ResourceDimension::Texture2D,
            alpha_mode: AlphaMode::Straight,
        })?,
    };
    fill_surface(&mut dds, &blocks);
    Ok(dds)
}

/// Serialize an [`RgbaImage`] to a block-compressed `.dds` byte buffer of the given BC `format`.
pub fn write_dds_bytes_bc(img: &RgbaImage, format: TexFormat) -> Result<Vec<u8>> {
    rgba_to_dds_bc(img, format)?.to_bytes()
}

/// Write an [`RgbaImage`] to a block-compressed `.dds` file at `path`.
pub fn save_dds_bc(img: &RgbaImage, path: impl AsRef<Path>, format: TexFormat) -> Result<()> {
    std::fs::write(path, write_dds_bytes_bc(img, format)?).map_err(rs_io::Error::from)?;
    Ok(())
}

trait DdsBytes {
    fn to_bytes(&self) -> Result<Vec<u8>>;
}

impl DdsBytes for Dds {
    fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.write(&mut buf)?;
        Ok(buf)
    }
}

/// Build an uncompressed 32-bit `A8R8G8B8` DDS surface from an RGBA8 image. The legacy layout
/// stores the channels little-endian, so the payload is BGRA — the same order a `.tex` holds
/// [`TexFormat::Bgra8`] in.
fn rgba_to_dds(img: &RgbaImage) -> Result<Dds> {
    let mut dds = Dds::new_d3d(NewD3dParams {
        height: img.height(),
        width: img.width(),
        depth: None,
        format: D3DFormat::A8R8G8B8,
        mipmap_levels: None,
        caps2: None,
    })?;
    let bgra: Vec<u8> = img
        .pixels()
        .flat_map(|p| {
            let [r, g, b, a] = p.0;
            [b, g, r, a]
        })
        .collect();
    fill_surface(&mut dds, &bgra);
    Ok(dds)
}

/// Serialize an [`RgbaImage`] to a standalone uncompressed `.dds` byte buffer.
pub fn write_dds_bytes(img: &RgbaImage) -> Result<Vec<u8>> {
    rgba_to_dds(img)?.to_bytes()
}

/// Write an [`RgbaImage`] to a `.dds` file at `path`.
pub fn save_dds(img: &RgbaImage, path: impl AsRef<Path>) -> Result<()> {
    std::fs::write(path, write_dds_bytes(img)?).map_err(rs_io::Error::from)?;
    Ok(())
}

fn surface_count(dds: &Dds) -> u32 {
    let layers = dds.get_num_array_layers().max(1);
    let dx10_cube = dds
        .header10
        .as_ref()
        .is_some_and(|h| h.misc_flag.contains(MiscFlag::TEXTURECUBE));
    // A DX10 cubemap counts its six faces as ONE array slot, so the header's layer
    // count is a sixth of the surfaces actually stored.
    if dx10_cube { layers * 6 } else { layers }
}

/// Decode every surface of a DDS buffer to RGBA8: a 2D texture yields one image; a cubemap
/// yields its six faces (+X, -X, +Y, -Y, +Z, -Z) and an array texture yields one image per
/// layer. The full-resolution mip of each layer is decoded.
pub fn read_dds_faces_bytes(bytes: &[u8]) -> Result<Vec<RgbaImage>> {
    let dds = Dds::read(bytes)?;
    let faces = surface_count(&dds);
    let stride = dds.data.len() / faces.max(1) as usize;

    let mut images = Vec::with_capacity(faces as usize);
    for face in 0..faces {
        let data = match dds.get_data(face) {
            Ok(data) => data,
            // The DX10 face indices above the array count are not addressable through the
            // header; the payload still holds them back to back at an even stride.
            Err(_) => {
                let start = stride * face as usize;
                dds.data
                    .get(start..start + stride)
                    .ok_or_else(|| Error::Decode(format!("dds face {face} out of bounds")))?
            }
        };
        images.push(decode_dds_surface(&dds, data)?);
    }
    Ok(images)
}

/// Decode every surface of a DDS file at `path` (see [`read_dds_faces_bytes`]).
pub fn read_dds_faces(path: impl AsRef<Path>) -> Result<Vec<RgbaImage>> {
    let bytes = std::fs::read(path).map_err(rs_io::Error::from)?;
    read_dds_faces_bytes(&bytes)
}

/** True when the DDS buffer describes a cubemap (six-face) surface.

A legacy file marks itself in `caps2`; one written through the DX10 extension marks itself in
the extended header's `TEXTURECUBE` misc flag instead and may leave `caps2` empty, so both have
to be checked or a DX10 cubemap reads as an ordinary 2D texture. */
pub fn dds_is_cubemap(bytes: &[u8]) -> Result<bool> {
    let dds = Dds::read(bytes)?;
    let dx10_cube = dds
        .header10
        .as_ref()
        .is_some_and(|h| h.misc_flag.contains(MiscFlag::TEXTURECUBE));
    Ok(dds.header.caps2.contains(Caps2::CUBEMAP) || dx10_cube)
}

/// The number of surfaces a DDS buffer holds: 6 for a cubemap, one per layer for an array
/// texture, 1 for an ordinary 2D texture. Anything above 1 cannot be represented by a single
/// image, so callers that edit pixels in place must refuse it.
pub fn dds_surface_count(bytes: &[u8]) -> Result<u32> {
    Ok(surface_count(&Dds::read(bytes)?))
}

/// Decode a DDS byte buffer straight to an RGBA8 image, including formats with no `.tex`
/// equivalent such as BC2 and BC7. For multi-surface DDS (cubemaps, arrays) this returns only
/// the first surface; use [`read_dds_faces_bytes`] for all of them.
pub fn read_dds_bytes(bytes: &[u8]) -> Result<RgbaImage> {
    let dds = Dds::read(bytes)?;
    let data = dds
        .get_data(0)
        .map_err(|e| Error::Decode(format!("dds layer 0: {e}")))?;
    decode_dds_surface(&dds, data)
}

/// Decode a DDS file at `path` to an RGBA8 image (first surface only).
pub fn read_dds(path: impl AsRef<Path>) -> Result<RgbaImage> {
    let bytes = std::fs::read(path).map_err(rs_io::Error::from)?;
    read_dds_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOURCC_OFFSET: usize = 84;
    const LEGACY_HEADER_LEN: usize = 128;

    fn fourcc(bytes: &[u8]) -> [u8; 4] {
        bytes[FOURCC_OFFSET..FOURCC_OFFSET + 4].try_into().unwrap()
    }

    fn sample() -> RgbaImage {
        RgbaImage::from_fn(8, 8, |x, y| {
            image::Rgba([(x * 32) as u8, (y * 32) as u8, 0x40, 0xff])
        })
    }

    #[test]
    fn bc1_and_bc3_write_a_legacy_header() {
        for (format, expected) in [(TexFormat::Bc1, *b"DXT1"), (TexFormat::Bc3, *b"DXT5")] {
            let bytes = write_dds_bytes_bc(&sample(), format).expect("write");
            assert_eq!(fourcc(&bytes), expected, "{format:?}");
            assert_ne!(
                fourcc(&bytes),
                *b"DX10",
                "{format:?} must not use the extension"
            );
        }
    }

    #[test]
    fn an_uncompressed_surface_writes_a8r8g8b8_in_bgra_order() {
        let img = RgbaImage::from_pixel(4, 4, image::Rgba([0x11, 0x22, 0x33, 0x44]));
        let bytes = write_dds_bytes(&img).expect("write");
        assert_ne!(fourcc(&bytes), *b"DX10");
        assert_eq!(
            &bytes[LEGACY_HEADER_LEN..LEGACY_HEADER_LEN + 4],
            &[0x33, 0x22, 0x11, 0x44]
        );
    }

    #[test]
    fn a_legacy_surface_starts_at_byte_128() {
        let bytes = write_dds_bytes_bc(&sample(), TexFormat::Bc1).expect("write");
        // 8x8 BC1 = 4 blocks of 8 bytes; anything longer means a DX10 header slipped in.
        assert_eq!(bytes.len(), LEGACY_HEADER_LEN + 32);
    }

    #[test]
    fn formats_with_no_legacy_pixel_format_keep_the_extension() {
        let bytes = write_dds_bytes_bc(&sample(), TexFormat::Bc7).expect("write");
        assert_eq!(fourcc(&bytes), *b"DX10");
    }

    #[test]
    fn a_dx10_cubemap_is_detected_without_caps2() {
        let mut dds = Dds::new_dxgi(ddsfile::NewDxgiParams {
            height: 4,
            width: 4,
            depth: None,
            format: DxgiFormat::BC1_UNorm,
            mipmap_levels: None,
            array_layers: Some(6),
            caps2: None,
            is_cubemap: true,
            resource_dimension: D3D10ResourceDimension::Texture2D,
            alpha_mode: AlphaMode::Straight,
        })
        .expect("build");
        dds.header.caps2 = Caps2::empty();

        let mut bytes = Vec::new();
        dds.write(&mut bytes).expect("write");
        assert!(dds_is_cubemap(&bytes).expect("classify"));
        assert_eq!(dds_surface_count(&bytes).expect("count"), 6);
    }

    #[test]
    fn a_plain_2d_texture_is_one_surface_and_not_a_cubemap() {
        let bytes = write_dds_bytes_bc(&sample(), TexFormat::Bc1).expect("write");
        assert!(!dds_is_cubemap(&bytes).expect("classify"));
        assert_eq!(dds_surface_count(&bytes).expect("count"), 1);
    }

    #[test]
    fn every_written_shape_reads_back() {
        for format in [TexFormat::Bc1, TexFormat::Bc3, TexFormat::Bc7] {
            let bytes = write_dds_bytes_bc(&sample(), format).expect("write");
            let back = read_dds_bytes(&bytes).unwrap_or_else(|e| panic!("{format:?}: {e}"));
            assert_eq!(back.dimensions(), (8, 8), "{format:?}");
        }
        let img = sample();
        let back = read_dds_bytes(&write_dds_bytes(&img).expect("write")).expect("read");
        assert_eq!(back.as_raw(), img.as_raw());
    }
}
