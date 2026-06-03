use std::io::Write;

use indexmap::IndexMap;
use rs_io::{Serialize, WriterExt};

use crate::error::{Error, Result};
use crate::rst::{Rst, RstValue};

impl Serialize for Rst {
    type Error = Error;

    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        let bits = Rst::check_version(self.version)?;

        let mut blob: Vec<u8> = Vec::new();
        let mut offsets: IndexMap<&RstValue, u64> = IndexMap::with_capacity(self.entries.len());

        for value in &self.blob_order {
            offsets.entry(value).or_insert_with(|| append(&mut blob, value));
        }

        let mut table = Vec::with_capacity(self.entries.len());
        for (hash, value) in &self.entries {
            let offset = *offsets.entry(value).or_insert_with(|| append(&mut blob, value));
            table.push((offset << bits) | (hash & ((1u64 << bits) - 1)));
        }

        writer.write_bytes(b"RST")?;
        writer.write_u8(self.version)?;

        if self.version == 2 {
            match &self.font_config {
                Some(config) => {
                    writer.write_bool(true)?;
                    writer.write_string_u32(config)?;
                }
                None => writer.write_bool(false)?,
            }
        }

        writer.write_u32(self.entries.len() as u32)?;
        for packed in table {
            writer.write_u64(packed)?;
        }

        if self.version < 5 {
            writer.write_u8(self.mode)?;
        }

        writer.write_bytes(&blob)?;
        Ok(())
    }
}

/** Appends `value` to the blob and returns its starting offset. A text value is written as its
UTF-8 bytes plus a NUL terminator; an encrypted value is written as `[0xFF][u16 length][bytes]`,
the exact framing the reader recognizes. */
fn append(blob: &mut Vec<u8>, value: &RstValue) -> u64 {
    let off = blob.len() as u64;
    match value {
        RstValue::Text(text) => {
            blob.extend_from_slice(text.as_bytes());
            blob.push(0);
        }
        RstValue::Encrypted(bytes) => {
            blob.push(0xFF);
            blob.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            blob.extend_from_slice(bytes);
        }
    }
    off
}
