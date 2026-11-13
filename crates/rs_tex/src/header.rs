use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::path::Path;

use ddsfile::{D3DFormat, DxgiFormat, Header};
use num_traits::FromPrimitive;
use rs_io::{Parse, ReaderExt};

use crate::{Error, Result, TEX_MAGIC, TexFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureContainer {
    Tex,
    Dds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureHeader {
    pub container: TextureContainer,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub block_compressed: bool,
    pub unknown_format: bool,
    pub has_mipmaps: bool,
    pub mip_count: Option<u32>,
    pub dx10: bool,
    pub dxgi_format: Option<u32>,
    pub tex_format_code: Option<u8>,
    pub tex_depth: Option<u8>,
    pub tex_resource_type: Option<u8>,
    pub tex_flags: Option<u8>,
}

impl Parse for TextureHeader {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let magic = reader.read_u32()?;
        if magic == TEX_MAGIC {
            let mut bytes = [0; 8];
            reader.read_exact(&mut bytes)?;
            let format = TexFormat::from_u8(bytes[5]);
            let has_mipmaps = bytes[7] != 0;
            return Ok(Self {
                container: TextureContainer::Tex,
                width: u16::from_le_bytes([bytes[0], bytes[1]]).into(),
                height: u16::from_le_bytes([bytes[2], bytes[3]]).into(),
                format: format
                    .map_or_else(|| format!("format byte {}", bytes[5]), |f| format!("{f:?}")),
                block_compressed: format.is_some_and(|f| f.block_size() == 4),
                unknown_format: format.is_none(),
                has_mipmaps,
                mip_count: (!has_mipmaps).then_some(1),
                dx10: false,
                dxgi_format: None,
                tex_format_code: Some(bytes[5]),
                tex_depth: Some(bytes[4]),
                tex_resource_type: Some(bytes[6]),
                tex_flags: Some(bytes[7]),
            });
        }
        if magic != u32::from_le_bytes(*b"DDS ") {
            return Err(Error::UnsupportedFormat(
                "expected TEX or DDS header".into(),
            ));
        }
        let mut bytes = [0; 124];
        reader.read_exact(&mut bytes)?;
        let header = Header::read(Cursor::new(bytes))?;
        let fourcc = header.spf.fourcc.as_ref().map(|f| f.0.to_le_bytes());
        let dx10 = fourcc == Some(*b"DX10");
        let dxgi_format = if dx10 {
            let mut extension = [0; 20];
            reader.read_exact(&mut extension)?;
            Some(u32::from_le_bytes([
                extension[0],
                extension[1],
                extension[2],
                extension[3],
            ]))
        } else {
            None
        };
        let (format, block_compressed, unknown_format) = if let Some(code) = dxgi_format {
            let known = DxgiFormat::from_u32(code).filter(|_| code != 0 && code != u32::MAX);
            (
                known.map_or_else(|| format!("DXGI {code}"), |f| format!("{f:?}")),
                (70..=84).contains(&code) || (94..=99).contains(&code),
                known.is_none(),
            )
        } else if let Some(code) = fourcc {
            let blocks = matches!(
                &code,
                b"DXT1"
                    | b"DXT2"
                    | b"DXT3"
                    | b"DXT4"
                    | b"DXT5"
                    | b"ATI1"
                    | b"ATI2"
                    | b"BC4U"
                    | b"BC4S"
                    | b"BC5U"
                    | b"BC5S"
            );
            let known = blocks || D3DFormat::try_from_pixel_format(&header.spf).is_some();
            let name = if code.iter().all(u8::is_ascii_graphic) {
                String::from_utf8_lossy(&code).into_owned()
            } else {
                format!("FourCC {:#010x}", u32::from_le_bytes(code))
            };
            (name, blocks, !known)
        } else {
            let known = D3DFormat::try_from_pixel_format(&header.spf);
            (
                known.map_or_else(|| "uncompressed".into(), |f| format!("{f:?}")),
                false,
                known.is_none(),
            )
        };
        let mip_count = header.mip_map_count.unwrap_or(1).max(1);
        Ok(Self {
            container: TextureContainer::Dds,
            width: header.width,
            height: header.height,
            format,
            block_compressed,
            unknown_format,
            has_mipmaps: mip_count > 1,
            mip_count: Some(mip_count),
            dx10,
            dxgi_format,
            tex_format_code: None,
            tex_depth: None,
            tex_resource_type: None,
            tex_flags: None,
        })
    }

    fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_reader(&mut File::open(path)?)
    }
}
