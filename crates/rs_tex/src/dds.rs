use std::path::Path;

use ddsfile::{
    AlphaMode, Caps2, D3D10ResourceDimension, D3DFormat, Dds, DxgiFormat, NewDxgiParams,
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

fn decode_dds_surface(dds: &Dds, data: &[u8]) -> Result<RgbaImage> {
    let width = dds.get_width();
    let height = dds.get_height();
    let (w, h) = (width as usize, height as usize);

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

    /// Serialize this texture's full-resolution mip into a standalone `.dds` byte buffer. The
    /// payload is decoded to RGBA8 and written as an uncompressed `R8G8B8A8_UNorm` surface, so
    /// the output is a lossless representation of the decoded image that any DDS reader accepts.
    pub fn to_dds_bytes(&self) -> Result<Vec<u8>> {
        let img = self.decode_rgba()?;
        rgba_to_dds(&img)?.to_bytes()
    }

    /// Write this texture as a `.dds` file at `path` (see [`Texture::to_dds_bytes`]).
    pub fn save_dds(&self, path: impl AsRef<Path>) -> Result<()> {
        std::fs::write(path, self.to_dds_bytes()?).map_err(rs_io::Error::from)?;
        Ok(())
    }
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

/// Build an uncompressed `R8G8B8A8_UNorm` DDS surface from an RGBA8 image.
fn rgba_to_dds(img: &RgbaImage) -> Result<Dds> {
    let mut dds = Dds::new_dxgi(NewDxgiParams {
        height: img.height(),
        width: img.width(),
        depth: None,
        format: DxgiFormat::R8G8B8A8_UNorm,
        mipmap_levels: None,
        array_layers: None,
        caps2: None,
        is_cubemap: false,
        resource_dimension: D3D10ResourceDimension::Texture2D,
        alpha_mode: AlphaMode::Straight,
    })?;
    let raw = img.as_raw();
    if raw.len() > dds.data.len() {
        dds.data.resize(raw.len(), 0);
    }
    dds.data[..raw.len()].copy_from_slice(raw);
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

/// Decode every surface of a DDS buffer to RGBA8: a 2D texture yields one image; a cubemap
/// yields its six faces (+X, -X, +Y, -Y, +Z, -Z) and an array texture yields one image per
/// layer. The full-resolution mip of each layer is decoded.
pub fn read_dds_faces_bytes(bytes: &[u8]) -> Result<Vec<RgbaImage>> {
    let dds = Dds::read(bytes)?;
    let layers = dds.get_num_array_layers().max(1);
    let mut images = Vec::with_capacity(layers as usize);
    for layer in 0..layers {
        let data = dds
            .get_data(layer)
            .map_err(|e| Error::Decode(format!("dds layer {layer}: {e}")))?;
        images.push(decode_dds_surface(&dds, data)?);
    }
    Ok(images)
}

/// Decode every surface of a DDS file at `path` (see [`read_dds_faces_bytes`]).
pub fn read_dds_faces(path: impl AsRef<Path>) -> Result<Vec<RgbaImage>> {
    let bytes = std::fs::read(path).map_err(rs_io::Error::from)?;
    read_dds_faces_bytes(&bytes)
}

/// True when the DDS buffer describes a cubemap (six-face) surface.
pub fn dds_is_cubemap(bytes: &[u8]) -> Result<bool> {
    let dds = Dds::read(bytes)?;
    Ok(dds.header.caps2.contains(Caps2::CUBEMAP))
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
