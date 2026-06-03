use std::io::Write;

use rs_io::{Serialize, WriterExt};

use crate::error::{Error, Result};
use crate::read::TEX_MAGIC;
use crate::texture::Texture;

impl Serialize for Texture {
    type Error = Error;

    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u32(TEX_MAGIC)?;
        writer.write_u16(self.width as u16)?;
        writer.write_u16(self.height as u16)?;
        writer.write_u8(self.unknown1)?;
        writer.write_u8(self.format.to_u8())?;
        writer.write_u8(self.unknown2)?;
        writer.write_bool(self.has_mipmaps)?;
        for mip in &self.mips {
            writer.write_bytes(mip)?;
        }
        Ok(())
    }
}
