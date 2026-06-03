/*!
The League `.tex` extended texture model: a header carrying dimensions, the extended format
byte, two unknown bytes, and a mipmap flag, followed by the mip chain stored smallest-first.
Block-compressed payloads (ETC/BC) decode to a BGRA value per pixel, reordered into RGBA.
*/

/// The League `.tex` extended format byte. Values match the on-disk encoding: ETC variants
/// `1..=3`, the DXT/BC block formats `10..=12`, and uncompressed BGRA8 at `20`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TexFormat {
    Etc1 = 1,
    Etc2 = 2,
    Etc2Eac = 3,
    Bc1 = 10,
    Bc1Alt = 11,
    Bc3 = 12,
    Bgra8 = 20,
}

impl TexFormat {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            1 => TexFormat::Etc1,
            2 => TexFormat::Etc2,
            3 => TexFormat::Etc2Eac,
            10 => TexFormat::Bc1,
            11 => TexFormat::Bc1Alt,
            12 => TexFormat::Bc3,
            20 => TexFormat::Bgra8,
            _ => return None,
        })
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Side length in pixels of one encoded block; `1` for uncompressed formats.
    pub fn block_size(self) -> usize {
        match self {
            TexFormat::Bgra8 => 1,
            _ => 4,
        }
    }

    /// Number of bytes one encoded block occupies on disk.
    pub fn bytes_per_block(self) -> usize {
        match self {
            TexFormat::Etc1 | TexFormat::Bc1 | TexFormat::Bc1Alt => 8,
            TexFormat::Etc2 | TexFormat::Etc2Eac | TexFormat::Bc3 => 16,
            TexFormat::Bgra8 => 4,
        }
    }

    /// Byte length of a single mip of the given dimensions in this format.
    pub fn mip_size(self, width: u32, height: u32) -> usize {
        let bs = self.block_size();
        let blocks_w = (width as usize).max(1).div_ceil(bs);
        let blocks_h = (height as usize).max(1).div_ceil(bs);
        blocks_w * blocks_h * self.bytes_per_block()
    }
}

/// A decoded-or-raw League texture: its dimensions, extended format, mip flag, and the mip
/// chain stored exactly as on disk (smallest mip first, full-resolution mip last).
#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub format: TexFormat,
    pub has_mipmaps: bool,
    pub unknown1: u8,
    pub unknown2: u8,
    pub mips: Vec<Vec<u8>>,
}

impl Texture {
    /// Construct a single-mip texture from one raw encoded payload.
    pub fn new(width: u32, height: u32, format: TexFormat, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            format,
            has_mipmaps: false,
            unknown1: 1,
            unknown2: 0,
            mips: vec![data],
        }
    }

    /// Number of mip levels derived from the largest dimension, or `1` without mipmaps.
    pub fn mip_count(&self) -> u32 {
        if self.has_mipmaps {
            let largest = self.width.max(self.height).max(1);
            32 - (largest.leading_zeros())
        } else {
            1
        }
    }

    /// The full-resolution mip, which League stores as the final block in the chain.
    pub fn largest_mip(&self) -> Option<&[u8]> {
        self.mips.last().map(Vec::as_slice)
    }
}
