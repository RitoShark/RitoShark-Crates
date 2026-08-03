use std::io::{Read, Seek, SeekFrom, Write};

use rs_io::{Parse, ReaderExt, Serialize, WriterExt};

use crate::error::{Error, Result};

const MAGIC: [u8; 4] = *b"r3d2";

/** Riot aligns three things to an eight-byte boundary: the end of the offset table, the end of
each entry record, and the end of each audio blob. Reproducing that single rule is what makes a
real package re-serialize byte for byte. */
const ALIGN: usize = 8;

fn align_up(value: usize) -> usize {
    value.next_multiple_of(ALIGN)
}

/** One embedded `.wem` within a WPK: its UTF-16-named entry plus the raw audio bytes.

`name` is the verbatim entry name. League stores it as `"<id>.wem"`, but any UTF-16 string is
kept as-is rather than reduced to a numeric id, so non-conforming names survive too. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WemEntry {
    pub name: String,
    pub data: Vec<u8>,
}

impl WemEntry {
    pub fn new(name: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            data,
        }
    }

    /** The numeric wem id parsed from a `"<id>.wem"` name, if the name follows that convention. */
    pub fn id(&self) -> Option<u32> {
        self.name.strip_suffix(".wem")?.parse().ok()
    }

    fn record_len(&self) -> usize {
        4 + 4 + 4 + self.name.encode_utf16().count() * 2
    }
}

/** A `r3d2` WPK container holding a flat list of named `.wem` blobs.

The on-disk layout is: `"r3d2"` magic, `u32` version, `u32` slot count, then one `u32`
entry-offset per slot. Each non-zero offset points at an entry record (`u32` data offset, `u32`
size, `u32` name length in UTF-16 units, then the UTF-16-LE name). The audio blobs follow, and
every one of those three regions is padded up to an eight-byte boundary.

Real packages can carry **dead slots** — offset-table entries whose value is `0`, pointing at
nothing. `dead_slots` records their positions (indices into the full offset table) so the table
is reproduced with identical length and zero placement. Live entries are kept in table order. */
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
    present; `None` for names not following that convention. */
    pub fn wems(&self) -> Vec<(Option<u32>, &str, &[u8])> {
        self.entries
            .iter()
            .map(|e| (e.id(), e.name.as_str(), e.data.as_slice()))
            .collect()
    }

    /** The bytes of the entry whose name parses to `id`, i.e. the payload League stores as
    `"<id>.wem"`. */
    pub fn wem(&self, id: u32) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|e| e.id() == Some(id))
            .map(|e| e.data.as_slice())
    }

    pub fn push(&mut self, entry: WemEntry) {
        self.entries.push(entry);
    }

    fn slot_count(&self) -> usize {
        self.entries.len() + self.dead_slots.len()
    }

    /** Canonical offsets for every live entry record and every audio blob, derived purely from
    the entry list and the eight-byte alignment rule. Both the reader's bounds checks and the
    writer's output come from this one function, so they cannot disagree. */
    fn layout(&self) -> (Vec<usize>, Vec<usize>) {
        let mut record_offsets = Vec::with_capacity(self.entries.len());
        let mut cursor = align_up(4 + 4 + 4 + self.slot_count() * 4);
        for entry in &self.entries {
            record_offsets.push(cursor);
            cursor = align_up(cursor + entry.record_len());
        }

        let mut data_offsets = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            data_offsets.push(cursor);
            cursor = align_up(cursor + entry.data.len());
        }

        (record_offsets, data_offsets)
    }
}

fn read_name_utf16(reader: &mut impl Read, units: usize) -> Result<String> {
    let mut buf = Vec::with_capacity(units.min(0x10000));
    for _ in 0..units {
        buf.push(reader.read_u16()?);
    }
    String::from_utf16(&buf).map_err(|_| Error::Unsupported("invalid utf-16 name"))
}

impl Parse for Wpk {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let magic = reader.read_byte_array::<4>()?;
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
        if (12 + total as u64 * 4) > end {
            return Err(Error::Truncated);
        }
        let mut table_offsets = Vec::with_capacity(total.min(0x10000));
        for _ in 0..total {
            table_offsets.push(reader.read_u32()? as u64);
        }

        let mut dead_slots = Vec::new();
        let mut located =
            Vec::with_capacity(total - table_offsets.iter().filter(|&&o| o == 0).count());

        for (slot, &table_offset) in table_offsets.iter().enumerate() {
            if table_offset == 0 {
                dead_slots.push(slot as u32);
                continue;
            }
            if table_offset.saturating_add(12) > end {
                return Err(Error::Truncated);
            }
            reader.seek(SeekFrom::Start(table_offset))?;
            let data_offset = reader.read_u32()? as u64;
            let size = reader.read_u32()? as u64;
            let unit_count = reader.read_u32()? as usize;
            if data_offset.checked_add(size).is_none_or(|e| e > end) {
                return Err(Error::Truncated);
            }
            if table_offset + 12 + (unit_count as u64) * 2 > end {
                return Err(Error::Truncated);
            }
            let name = read_name_utf16(reader, unit_count)?;
            located.push((name, data_offset, size as usize));
        }

        let mut entries = Vec::with_capacity(located.len());
        for (name, data_offset, size) in located {
            reader.seek(SeekFrom::Start(data_offset))?;
            let data = reader.read_bytes(size)?;
            entries.push(WemEntry { name, data });
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
        let (record_offsets, data_offsets) = self.layout();

        writer.write_bytes(&MAGIC)?;
        writer.write_u32(self.version)?;
        writer.write_u32(total as u32)?;

        let dead: std::collections::BTreeSet<u32> = self.dead_slots.iter().copied().collect();
        let mut live = record_offsets.iter();
        for slot in 0..total as u32 {
            if dead.contains(&slot) {
                writer.write_u32(0)?;
            } else {
                writer.write_u32(live.next().copied().unwrap_or(0) as u32)?;
            }
        }

        let mut written = 4 + 4 + 4 + total * 4;
        for (i, entry) in self.entries.iter().enumerate() {
            written += pad_to(writer, written, record_offsets[i])?;
            writer.write_u32(data_offsets[i] as u32)?;
            writer.write_u32(entry.data.len() as u32)?;
            let units: Vec<u16> = entry.name.encode_utf16().collect();
            writer.write_u32(units.len() as u32)?;
            for unit in &units {
                writer.write_u16(*unit)?;
            }
            written += entry.record_len();
        }

        for (i, entry) in self.entries.iter().enumerate() {
            written += pad_to(writer, written, data_offsets[i])?;
            writer.write_bytes(&entry.data)?;
            written += entry.data.len();
        }

        Ok(())
    }
}

fn pad_to<W: Write>(writer: &mut W, written: usize, target: usize) -> Result<usize> {
    let pad = target.saturating_sub(written);
    for _ in 0..pad {
        writer.write_u8(0)?;
    }
    Ok(pad)
}
