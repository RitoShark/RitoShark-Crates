use std::io::{Read, Seek, SeekFrom};

use rs_io::{Parse, ReaderExt};

use crate::error::{Error, Result};
use crate::texture::{TexFormat, Texture};

pub const TEX_MAGIC: u32 = 0x0058_4554;

fn mips_use_block_layout(format: TexFormat) -> bool {
    matches!(
        format,
        TexFormat::Bc1
            | TexFormat::Bc1Alt
            | TexFormat::Bc3
            | TexFormat::Bc7
            | TexFormat::Bc5
            | TexFormat::Bgra8
            | TexFormat::Rgba16Snorm
    )
}

impl Parse for Texture {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut texture = read_header(reader)?;
        texture.mips = if texture.has_mipmaps && mips_use_block_layout(texture.format) {
            let count = derive_mip_count(texture.width, texture.height);
            let sizes: Vec<u64> = (0..count).map(|level| mip_size(&texture, level)).collect();
            let expected = sizes.iter().sum();
            let (start, end) = payload_bounds(reader)?;
            let actual = end - start;
            if actual != expected {
                return Err(Error::InvalidMipPayload {
                    expected,
                    actual,
                    levels: count,
                });
            }
            let mut mips = Vec::with_capacity(count as usize);
            for size in sizes.into_iter().rev() {
                mips.push(reader.read_bytes(addressable_size(size)?)?);
            }
            mips
        } else {
            let mut data = Vec::new();
            reader.read_to_end(&mut data)?;
            vec![data]
        };

        Ok(texture)
    }
}

impl Texture {
    pub fn from_reader_largest_mip<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut texture = read_header(reader)?;
        if texture.unknown2 != 0 || texture.unknown1 > 1 {
            return Err(Error::UnsupportedFormat(
                "largest-mip extraction requires a 2D TEX resource".into(),
            ));
        }
        if !mips_use_block_layout(texture.format) {
            return Err(Error::UnsupportedFormat(
                "largest-mip extraction requires a known BC or uncompressed layout".into(),
            ));
        }
        if texture.width == 0 || texture.height == 0 {
            return Err(Error::Decode("TEX dimensions must be nonzero".into()));
        }
        let size = mip_size(&texture, 0);
        let (start, end) = payload_bounds(reader)?;
        if end - start < size {
            return Err(Error::InvalidMipPayload {
                expected: size,
                actual: end - start,
                levels: 1,
            });
        }
        reader.seek(SeekFrom::Start(end - size))?;
        texture
            .mips
            .push(reader.read_bytes(addressable_size(size)?)?);
        texture.has_mipmaps = false;
        Ok(texture)
    }
}

fn read_header<R: Read>(reader: &mut R) -> Result<Texture> {
    let magic = reader.read_u32()?;
    if magic != TEX_MAGIC {
        return Err(Error::InvalidMagic {
            expected: TEX_MAGIC,
            got: magic,
        });
    }
    let width = reader.read_u16()?.into();
    let height = reader.read_u16()?.into();
    let unknown1 = reader.read_u8()?;
    let format_byte = reader.read_u8()?;
    let unknown2 = reader.read_u8()?;
    let has_mipmaps = reader.read_bool()?;
    let format = TexFormat::from_u8(format_byte)
        .ok_or_else(|| Error::UnsupportedFormat(format!("tex format byte {format_byte}")))?;
    Ok(Texture {
        width,
        height,
        format,
        has_mipmaps,
        unknown1,
        unknown2,
        mips: Vec::new(),
    })
}

fn payload_bounds<R: Seek>(reader: &mut R) -> Result<(u64, u64)> {
    let start = reader.stream_position()?;
    let end = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(start))?;
    if end < start {
        return Err(Error::Decode("TEX payload ends before its header".into()));
    }
    Ok((start, end))
}

fn mip_size(texture: &Texture, level: u32) -> u64 {
    let block = texture.format.block_size() as u64;
    let width = u64::from((texture.width >> level).max(1));
    let height = u64::from((texture.height >> level).max(1));
    width.div_ceil(block) * height.div_ceil(block) * texture.format.bytes_per_block() as u64
}

fn addressable_size(size: u64) -> Result<usize> {
    usize::try_from(size).map_err(|_| Error::Decode("TEX mip exceeds the addressable size".into()))
}

fn derive_mip_count(width: u32, height: u32) -> u32 {
    let largest = width.max(height).max(1);
    32 - largest.leading_zeros()
}
