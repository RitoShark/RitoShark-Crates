use std::io::{Read, Seek, SeekFrom, Write};

use rs_io::{Parse, ReaderExt, Serialize, WriterExt};

use crate::error::{Error, Result};

const MAGIC: [u8; 4] = *b"r3d2";

/** One embedded `.wem` within a WPK: its UTF-16-named entry plus the raw audio bytes. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WemEntry {
    pub name: String,
    pub data: Vec<u8>,
}

/** A `r3d2` WPK container holding a flat list of named `.wem` blobs. Reading follows the per-file
offset table to each entry (data offset, size, UTF-16-LE name) and pulls the referenced bytes;
writing rebuilds a canonical layout: header, the entry-offset array, packed entries, then the
audio blobs in order. */
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wpk {
    pub version: u32,
    pub entries: Vec<WemEntry>,
}

impl Wpk {
    pub fn new(version: u32) -> Self {
        Self {
            version,
            entries: Vec::new(),
        }
    }
}

fn read_name_utf16(reader: &mut impl Read) -> Result<String> {
    let units = reader.read_u32()? as usize;
    let mut buf = Vec::with_capacity(units);
    for _ in 0..units {
        buf.push(reader.read_u16()?);
    }
    String::from_utf16(&buf).map_err(|_| Error::Unsupported("invalid utf-16 name"))
}

fn name_units(name: &str) -> Vec<u16> {
    name.encode_utf16().collect()
}

impl Parse for Wpk {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let magic = reader.read_array::<4>()?;
        if magic != MAGIC {
            return Err(Error::InvalidMagic);
        }
        let version = reader.read_u32()?;
        if version != 1 {
            return Err(Error::Unsupported("wpk version"));
        }

        let count = reader.read_u32()? as usize;
        let mut table_offsets = Vec::with_capacity(count);
        for _ in 0..count {
            table_offsets.push(reader.read_u32()? as u64);
        }

        let mut entries = Vec::with_capacity(count);
        for table_offset in table_offsets {
            reader.seek(SeekFrom::Start(table_offset))?;
            let data_offset = reader.read_u32()? as u64;
            let size = reader.read_u32()? as usize;
            let name = read_name_utf16(reader)?;

            reader.seek(SeekFrom::Start(data_offset))?;
            let data = reader.read_bytes(size)?;
            entries.push(WemEntry { name, data });
        }

        Ok(Self { version, entries })
    }
}

impl Serialize for Wpk {
    type Error = Error;

    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        let count = self.entries.len();

        let names: Vec<Vec<u16>> = self.entries.iter().map(|e| name_units(&e.name)).collect();

        let header_len = 4 + 4 + 4 + count * 4;
        let entry_sizes: Vec<usize> = names.iter().map(|n| 4 + 4 + 4 + n.len() * 2).collect();

        let mut table_offsets = Vec::with_capacity(count);
        let mut cursor = header_len;
        for size in &entry_sizes {
            table_offsets.push(cursor as u32);
            cursor += size;
        }

        let mut data_offsets = Vec::with_capacity(count);
        for entry in &self.entries {
            data_offsets.push(cursor as u32);
            cursor += entry.data.len();
        }

        writer.write_bytes(&MAGIC)?;
        writer.write_u32(self.version)?;
        writer.write_u32(count as u32)?;
        for &offset in &table_offsets {
            writer.write_u32(offset)?;
        }

        for (i, name) in names.iter().enumerate() {
            writer.write_u32(data_offsets[i])?;
            writer.write_u32(self.entries[i].data.len() as u32)?;
            writer.write_u32(name.len() as u32)?;
            for &unit in name {
                writer.write_u16(unit)?;
            }
        }

        for entry in &self.entries {
            writer.write_bytes(&entry.data)?;
        }

        Ok(())
    }
}
