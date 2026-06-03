/*!
Builds a League `.tex` from raw RGBA pixels: it block-compresses the full-resolution image (and,
when requested, every mip level down to 1x1) into the on-disk BC layout and assembles a
[`Texture`] whose mip chain is stored smallest-first, exactly as the reader expects. Mip levels
are produced with a separable Lanczos-3 resample so the generated chain matches the quality of
the reference tooling.
*/

use image::RgbaImage;
use texpresso::{Format, Params};

use crate::error::{Error, Result};
use crate::texture::{TexFormat, Texture};

fn lanczos(x: f64, a: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    if x <= -a || x >= a {
        return 0.0;
    }
    let pix = std::f64::consts::PI * x;
    (pix.sin() / pix) * ((pix / a).sin() / (pix / a))
}

/// Resample an RGBA8 buffer to a new size with a Lanczos-3 kernel, matching the reference
/// downsampler so generated mip chains line up with production tooling.
fn downsample_rgba(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    const A: f64 = 3.0;
    let (src_w, src_h, dst_w, dst_h) = (
        src_w as usize,
        src_h as usize,
        dst_w as usize,
        dst_h as usize,
    );
    let mut dst = vec![0u8; dst_w * dst_h * 4];
    let scale_x = src_w as f64 / dst_w as f64;
    let scale_y = src_h as f64 / dst_h as f64;

    for y in 0..dst_h {
        let src_y = (y as f64 + 0.5) * scale_y - 0.5;
        let y0 = (src_y - A).floor().max(0.0) as usize;
        let y1 = ((src_y + A).ceil() as i64).clamp(0, src_h as i64 - 1) as usize;
        for x in 0..dst_w {
            let src_x = (x as f64 + 0.5) * scale_x - 0.5;
            let x0 = (src_x - A).floor().max(0.0) as usize;
            let x1 = ((src_x + A).ceil() as i64).clamp(0, src_w as i64 - 1) as usize;

            let mut acc = [0.0f64; 4];
            let mut weight_sum = 0.0;
            for sy in y0..=y1 {
                let wy = lanczos(sy as f64 - src_y, A);
                for sx in x0..=x1 {
                    let w = wy * lanczos(sx as f64 - src_x, A);
                    let idx = (sy * src_w + sx) * 4;
                    for (c, a) in acc.iter_mut().enumerate() {
                        *a += src[idx + c] as f64 * w;
                    }
                    weight_sum += w;
                }
            }
            let didx = (y * dst_w + x) * 4;
            if weight_sum > 0.0 {
                for c in 0..4 {
                    dst[didx + c] = (acc[c] / weight_sum + 0.5).clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    dst
}

fn compress(format: Format, rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0u8; format.compressed_size(w, h)];
    format.compress(rgba, w, h, Params::default(), &mut out);
    out
}

/// Number of mip levels in a full chain down to 1x1 for the given base dimensions.
fn mip_levels(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

impl Texture {
    /// Encode an [`RgbaImage`] into a `.tex` of the given block-compressed `format`, optionally
    /// generating the full mip chain (Lanczos-3 downsampled, stored smallest-first). Only the
    /// BC block formats are accepted here; uncompressed `Bgra8` textures are built directly via
    /// [`Texture::new`] / [`Texture::from_rgba_bgra8`].
    pub fn encode(image: &RgbaImage, format: TexFormat, mipmaps: bool) -> Result<Texture> {
        let codec = match format {
            TexFormat::Bc1 | TexFormat::Bc1Alt => Format::Bc1,
            TexFormat::Bc3 => Format::Bc3,
            TexFormat::Bc5 => Format::Bc5,
            other => {
                return Err(Error::UnsupportedFormat(format!(
                    "encode is only implemented for BC1/BC3/BC5, not {other:?}"
                )));
            }
        };

        let width = image.width();
        let height = image.height();
        if width == 0 || height == 0 {
            return Err(Error::Encode("cannot encode a zero-sized image".into()));
        }

        let mut mips = Vec::new();
        if mipmaps {
            let count = mip_levels(width, height);
            let mut level_rgba = image.as_raw().clone();
            let mut level_w = width;
            let mut level_h = height;
            let mut encoded = Vec::with_capacity(count as usize);
            for _ in 0..count {
                encoded.push(compress(codec, &level_rgba, level_w, level_h));
                let next_w = (level_w / 2).max(1);
                let next_h = (level_h / 2).max(1);
                if next_w != level_w || next_h != level_h {
                    level_rgba = downsample_rgba(&level_rgba, level_w, level_h, next_w, next_h);
                    level_w = next_w;
                    level_h = next_h;
                }
            }
            encoded.reverse();
            mips = encoded;
        } else {
            mips.push(compress(codec, image.as_raw(), width, height));
        }

        Ok(Texture {
            width,
            height,
            format,
            has_mipmaps: mipmaps,
            unknown1: 1,
            unknown2: 0,
            mips,
        })
    }

    /// Encode an [`RgbaImage`] as a BC1 (DXT1) `.tex`, optionally with a generated mip chain.
    pub fn encode_bc1(image: &RgbaImage, mipmaps: bool) -> Result<Texture> {
        Self::encode(image, TexFormat::Bc1, mipmaps)
    }

    /// Encode an [`RgbaImage`] as a BC3 (DXT5) `.tex`, optionally with a generated mip chain.
    pub fn encode_bc3(image: &RgbaImage, mipmaps: bool) -> Result<Texture> {
        Self::encode(image, TexFormat::Bc3, mipmaps)
    }

    /// Build an uncompressed `Bgra8` `.tex` from an [`RgbaImage`] by reordering channels into the
    /// on-disk BGRA order. No mip chain is generated.
    pub fn from_rgba_bgra8(image: &RgbaImage) -> Texture {
        let mut data = Vec::with_capacity(image.as_raw().len());
        for px in image.as_raw().chunks_exact(4) {
            data.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
        Texture::new(image.width(), image.height(), TexFormat::Bgra8, data)
    }
}
