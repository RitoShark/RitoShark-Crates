use std::path::Path;

use ddsfile::{D3DFormat, Dds, DxgiFormat};
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

fn decode_dds_rgba(dds: &Dds) -> Result<RgbaImage> {
    let width = dds.get_width();
    let height = dds.get_height();
    let (w, h) = (width as usize, height as usize);
    let data = dds.data.as_slice();

    match classify(dds)? {
        DdsKind::Bc1 => decode_block_format(TexFormat::Bc1, width, height, data),
        DdsKind::Bc3 => decode_block_format(TexFormat::Bc3, width, height, data),
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
}

/// Decode a DDS byte buffer straight to an RGBA8 image, including formats with no `.tex`
/// equivalent such as BC2 and BC7.
pub fn read_dds_bytes(bytes: &[u8]) -> Result<RgbaImage> {
    let dds = Dds::read(bytes)?;
    decode_dds_rgba(&dds)
}

/// Decode a DDS file at `path` to an RGBA8 image.
pub fn read_dds(path: impl AsRef<Path>) -> Result<RgbaImage> {
    let bytes = std::fs::read(path).map_err(rs_io::Error::from)?;
    read_dds_bytes(&bytes)
}
