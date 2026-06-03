use std::io::{Read, Seek};

use rs_io::{Parse, ReaderExt};

use crate::error::{Error, Result};
use crate::texture::{TexFormat, Texture};

pub const TEX_MAGIC: u32 = 0x0058_4554;

fn mips_use_block_layout(format: TexFormat) -> bool {
    matches!(
        format,
        TexFormat::Bc1 | TexFormat::Bc1Alt | TexFormat::Bc3 | TexFormat::Bgra8
    )
}

impl Parse for Texture {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let magic = reader.read_u32()?;
        if magic != TEX_MAGIC {
            return Err(Error::InvalidMagic {
                expected: TEX_MAGIC,
                got: magic,
            });
        }

        let width = reader.read_u16()? as u32;
        let height = reader.read_u16()? as u32;
        let unknown1 = reader.read_u8()?;
        let format_byte = reader.read_u8()?;
        let unknown2 = reader.read_u8()?;
        let has_mipmaps = reader.read_bool()?;

        let format = TexFormat::from_u8(format_byte)
            .ok_or_else(|| Error::UnsupportedFormat(format!("tex format byte {format_byte}")))?;

        let mips = if has_mipmaps && mips_use_block_layout(format) {
            let count = derive_mip_count(width, height);
            let mut mips = Vec::with_capacity(count as usize);
            for level in (0..count).rev() {
                let w = (width >> level).max(1);
                let h = (height >> level).max(1);
                mips.push(reader.read_bytes(format.mip_size(w, h))?);
            }
            mips
        } else {
            let mut data = Vec::new();
            reader.read_to_end(&mut data)?;
            vec![data]
        };

        Ok(Self {
            width,
            height,
            format,
            has_mipmaps,
            unknown1,
            unknown2,
            mips,
        })
    }
}

fn derive_mip_count(width: u32, height: u32) -> u32 {
    let largest = width.max(height).max(1);
    32 - largest.leading_zeros()
}
