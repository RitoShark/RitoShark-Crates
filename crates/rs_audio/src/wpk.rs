use std::io::{Read, Seek, SeekFrom, Write};

use rs_io::{Parse, ReaderExt, Serialize, WriterExt};

use crate::error::{Error, Result};

const MAGIC: [u8; 4] = *b"r3d2";

/** One embedded `.wem` within a WPK: its UTF-16-named entry plus the raw audio bytes.

`align` records how many padding bytes precede this entry's data blob, measured against the
position our canonical packing would otherwise place it at. It is `0` for a freshly constructed
entry and is captured on read so a real package's blob alignment round-trips exactly. `name` is
the verbatim entry name; League stores it as `"<id>.wem"` but any UTF-16 string is kept as-is
rather than reduced to a numeric id, so non-conforming names survive too. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WemEntry {
    pub name: String,
    pub data: Vec<u8>,
    pub align: u32,
}

impl WemEntry {
    pub fn new(name: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            data,
            align: 0,
        }
    }

    /** The numeric wem id parsed from a `"<id>.wem"` name, if the name follows that convention. */
    pub fn id(&self) -> Option<u32> {
        self.name.strip_suffix(".wem")?.parse().ok()
    }
}

/** A `r3d2` WPK container holding a flat list of named `.wem` blobs.

The on-disk layout is: `"r3d2"` magic, `u32` version, `u32` slot count, then one `u32`
entry-offset per slot. Each non-zero offset points at an entry record (`u32` data offset, `u32`
size, `u32` name length in UTF-16 units, then the UTF-16-LE name). The audio blobs follow.

Real packages can carry **dead slots** — offset-table entries whose value is `0`, pointing at
nothing. `dead_slots` records their positions (indices into the full offset table) so the table is
reproduced with identical length and zero placement. Live entries are kept in **table order**, and
each [`WemEntry::align`] preserves padding before its blob, so a real package round-trips byte for
byte even where our canonical packing would otherwise differ. */
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wpk {
    pub version: u32,
    pub entries: Vec<WemEntry>,
    pub dead_slots: Vec<u32>,
}

impl Wpk {
    pub fn new(version: u32) -> Self {
        Self {
            version,
            entries: Vec::new(),
            dead_slots: Vec::new(),
        }
    }

    /** The embedded wems as `(id, name, bytes)`. The id is parsed from a `"<id>.wem"` name when
    present, mirroring the reference reader; `None` for names not following that convention. */
    pub fn wems(&self) -> Vec<(Option<u32>, &str, &[u8])> {
        self.entries
            .iter()
            .map(|e| (e.id(), e.name.as_str(), e.data.as_slice()))
            .collect()
    }

    pub fn push(&mut self, entry: WemEntry) {
        self.entries.push(entry);
    }

    fn slot_count(&self) -> usize {
        self.entries.len() + self.dead_slots.len()
    }
}

fn read_name_utf16(reader: &mut impl Read, units: usize) -> Result<String> {
    let mut buf = Vec::with_capacity(units.min(0x10000));
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

        let end = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(8))?;

        let total = reader.read_u32()? as usize;
        let mut table_offsets = Vec::with_capacity(total.min(0x10000));
        for _ in 0..total {
            table_offsets.push(reader.read_u32()? as u64);
        }

        let names_count = total - table_offsets.iter().filter(|&&o| o == 0).count();
        let mut names: Vec<Vec<u16>> = Vec::with_capacity(names_count.min(0x10000));
        let mut dead_slots = Vec::new();
        let mut raw = Vec::with_capacity(names_count.min(0x10000));

        for (slot, &table_offset) in table_offsets.iter().enumerate() {
            if table_offset == 0 {
                dead_slots.push(slot as u32);
                continue;
            }
            if table_offset > end {
                return Err(Error::Truncated);
            }
            reader.seek(SeekFrom::Start(table_offset))?;
            let data_offset = reader.read_u32()? as u64;
            let size = reader.read_u32()? as u64;
            let unit_count = reader.read_u32()? as usize;
            if data_offset.checked_add(size).is_none_or(|e| e > end) {
                return Err(Error::Truncated);
            }
            let name = read_name_utf16(reader, unit_count)?;
            names.push(name_units(&name));
            raw.push((name, data_offset, size as usize));
        }

        let header_len = 4 + 4 + 4 + total * 4;
        let mut cursor = header_len as u64;
        for units in &names {
            cursor += (4 + 4 + 4 + units.len() * 2) as u64;
        }

        let mut entries = Vec::with_capacity(raw.len());
        for (name, data_offset, size) in raw {
            let align = data_offset.saturating_sub(cursor) as u32;
            reader.seek(SeekFrom::Start(data_offset))?;
            let data = reader.read_bytes(size)?;
            cursor = data_offset + size as u64;
            entries.push(WemEntry { name, data, align });
        }

        Ok(Self {
            version,
            entries,
            dead_slots,
        })
    }
}

impl Serialize for Wpk {
    type Error = Error;

    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        let total = self.slot_count();
        let live = self.entries.len();

        let names: Vec<Vec<u16>> = self.entries.iter().map(|e| name_units(&e.name)).collect();

        let header_len = 4 + 4 + 4 + total * 4;
        let entry_sizes: Vec<usize> = names.iter().map(|n| 4 + 4 + 4 + n.len() * 2).collect();

        let mut table_offsets = Vec::with_capacity(live);
        let mut cursor = header_len;
        for size in &entry_sizes {
            table_offsets.push(cursor as u32);
            cursor += size;
        }

        let mut data_offsets = Vec::with_capacity(live);
        for entry in &self.entries {
            cursor += entry.align as usize;
            data_offsets.push(cursor as u32);
            cursor += entry.data.len();
        }

        writer.write_bytes(&MAGIC)?;
        writer.write_u32(self.version)?;
        writer.write_u32(total as u32)?;

        let mut live_iter = table_offsets.iter();
        let dead: std::collections::BTreeSet<u32> = self.dead_slots.iter().copied().collect();
        for slot in 0..total as u32 {
            if dead.contains(&slot) {
                writer.write_u32(0)?;
            } else if let Some(&offset) = live_iter.next() {
                writer.write_u32(offset)?;
            } else {
                writer.write_u32(0)?;
            }
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
            for _ in 0..entry.align {
                writer.write_u8(0)?;
            }
            writer.write_bytes(&entry.data)?;
        }

        Ok(())
    }
}
