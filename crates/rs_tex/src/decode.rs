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

/** Convert a `texture2ddecoder` BC5 result into an RGBA8 normal-map image. BC5 stores only two
channels (the X/Y of a tangent-space normal); the decoder leaves blue and alpha empty. We carry the
two stored channels into R/G, reconstruct the third from the unit-length constraint
`z = sqrt(1 - x² - y²)` with the centered `(z + 1) * 127.5` display mapping every normal-map tool
uses, and force alpha opaque so the result is not fully transparent. */
fn bc5_u32_to_rgba_image(width: u32, height: u32, pixels: &[u32]) -> Result<RgbaImage> {
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for &px in pixels {
        let [_, g, r, _] = px.to_le_bytes();
        let xf = (r as f32 - 127.5) / 127.5;
        let yf = (g as f32 - 127.5) / 127.5;
        let zf = (1.0 - xf * xf - yf * yf).max(0.0).sqrt();
        let b = ((zf + 1.0) * 127.5).clamp(0.0, 255.0) as u8;
        rgba.extend_from_slice(&[r, g, b, 255]);
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

/// Reorder a tightly packed RGBA16_SNORM buffer (four signed 16-bit channels per pixel mapped
/// from `[-1, 1]` to `[0, 255]`) into an RGBA8 image of the given size.
fn rgba16_snorm_to_rgba_image(width: u32, height: u32, data: &[u8]) -> Result<RgbaImage> {
    let expected = (width as usize) * (height as usize) * 8;
    if data.len() < expected {
        return Err(Error::Decode(format!(
            "rgba16_snorm payload too small: have {}, need {expected}",
            data.len()
        )));
    }
    let mut rgba = Vec::with_capacity(expected / 2);
    for chan in data[..expected].chunks_exact(2) {
        let mut s = i16::from_le_bytes([chan[0], chan[1]]);
        if s == i16::MIN {
            s = -i16::MAX;
        }
        let f = ((s as f32 / i16::MAX as f32) + 1.0) * 0.5;
        rgba.push((f * 255.0 + 0.5).clamp(0.0, 255.0) as u8);
    }
    RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| Error::Decode("rgba16_snorm buffer does not match dimensions".into()))
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
        TexFormat::Bc5 => {
            texture2ddecoder::decode_bc5(data, w, h, &mut out)
                .map_err(|e| Error::Decode(e.to_string()))?;
            return bc5_u32_to_rgba_image(width, height, &out);
        }
        TexFormat::Etc1 => texture2ddecoder::decode_etc1(data, w, h, &mut out),
        TexFormat::Etc2 => texture2ddecoder::decode_etc2_rgb(data, w, h, &mut out),
        TexFormat::Etc2Eac => texture2ddecoder::decode_etc2_rgba8(data, w, h, &mut out),
        TexFormat::Bgra8 => return bgra_bytes_to_rgba_image(width, height, data),
        TexFormat::Rgba16Snorm => return rgba16_snorm_to_rgba_image(width, height, data),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bc5_reconstructs_blue_and_is_opaque() {
        // A 4x4 BC5 block whose red and green sub-blocks both encode a constant 128 (mid-axis):
        // each BC4 sub-block is `[endpoint0, endpoint1, 6 index bytes]`; equal endpoints make every
        // texel resolve to the endpoint value regardless of the indices.
        let block = [128u8, 128, 0, 0, 0, 0, 0, 0, 128, 128, 0, 0, 0, 0, 0, 0];
        let img = decode_block_format(TexFormat::Bc5, 4, 4, &block).expect("decode bc5");
        for px in img.pixels() {
            let [r, g, b, a] = px.0;
            assert_eq!(a, 255, "BC5 alpha must be opaque, not transparent");
            assert!(b >= 250, "BC5 blue must be reconstructed (~255), got {b}");
            assert!((r as i32 - 128).abs() <= 1, "BC5 red ~128, got {r}");
            assert!((g as i32 - 128).abs() <= 1, "BC5 green ~128, got {g}");
        }
    }
}
