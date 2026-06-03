use image::RgbaImage;

use crate::error::{Error, Result};
use crate::texture::{TexFormat, Texture};

/// Reorder a decoded BGRA `u32` buffer into an RGBA8 image of the given size.
fn bgra_u32_to_rgba_image(width: u32, height: u32, pixels: &[u32]) -> Result<RgbaImage> {
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for &px in pixels {
        let [b, g, r, a] = px.to_le_bytes();
        rgba.extend_from_slice(&[r, g, b, a]);
    }
    RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| Error::Decode("decoded buffer does not match dimensions".into()))
}

/// Reorder a tightly packed BGRA8 byte buffer into an RGBA8 image of the given size.
fn bgra_bytes_to_rgba_image(width: u32, height: u32, data: &[u8]) -> Result<RgbaImage> {
    let expected = (width as usize) * (height as usize) * 4;
    if data.len() < expected {
        return Err(Error::Decode(format!(
            "bgra8 payload too small: have {}, need {expected}",
            data.len()
        )));
    }
    let mut rgba = Vec::with_capacity(expected);
    for px in data[..expected].chunks_exact(4) {
        rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| Error::Decode("bgra8 buffer does not match dimensions".into()))
}

/// Decode block-compressed bytes of the given format into an RGBA8 image.
pub(crate) fn decode_block_format(
    format: TexFormat,
    width: u32,
    height: u32,
    data: &[u8],
) -> Result<RgbaImage> {
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0u32; w.max(1) * h.max(1)];
    let res: core::result::Result<(), &'static str> = match format {
        TexFormat::Bc1 | TexFormat::Bc1Alt => texture2ddecoder::decode_bc1(data, w, h, &mut out),
        TexFormat::Bc3 => texture2ddecoder::decode_bc3(data, w, h, &mut out),
        TexFormat::Bc7 => texture2ddecoder::decode_bc7(data, w, h, &mut out),
        TexFormat::Bc5 => texture2ddecoder::decode_bc5(data, w, h, &mut out),
        TexFormat::Etc1 => texture2ddecoder::decode_etc1(data, w, h, &mut out),
        TexFormat::Etc2 => texture2ddecoder::decode_etc2_rgb(data, w, h, &mut out),
        TexFormat::Etc2Eac => texture2ddecoder::decode_etc2_rgba8(data, w, h, &mut out),
        TexFormat::Bgra8 => return bgra_bytes_to_rgba_image(width, height, data),
    };
    res.map_err(|e| Error::Decode(e.to_string()))?;
    bgra_u32_to_rgba_image(width, height, &out)
}

impl Texture {
    /// Decode the full-resolution mip into an RGBA8 image, decompressing BC/ETC payloads and
    /// reordering uncompressed BGRA8 in place.
    pub fn decode_rgba(&self) -> Result<RgbaImage> {
        let data = self
            .largest_mip()
            .ok_or_else(|| Error::Decode("texture has no mip data".into()))?;
        decode_block_format(self.format, self.width, self.height, data)
    }
}
