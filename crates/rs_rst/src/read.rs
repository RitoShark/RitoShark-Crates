use std::io::{Read, Seek};

use rs_io::{Parse, ReaderExt};

use crate::error::{Error, Result};
use crate::rst::{Rst, RstValue};

const MAGIC: [u8; 3] = *b"RST";

impl Parse for Rst {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let magic = reader.read_array::<3>()?;
        if magic != MAGIC {
            return Err(Error::InvalidMagic);
        }

        let version = reader.read_u8()?;
        let bits = Rst::check_version(version)?;
        let mask = (1u64 << bits) - 1;

        let font_config = if version == 2 && reader.read_bool()? {
            Some(reader.read_string_u32()?)
        } else {
            None
        };

        let count = reader.read_u32()? as usize;
        let mut raw = Vec::with_capacity(count);
        for _ in 0..count {
            let packed = reader.read_u64()?;
            let offset = (packed >> bits) as usize;
            let hash = packed & mask;
            raw.push((hash, offset));
        }

        let mode = if version < 5 { reader.read_u8()? } else { 0 };
        let has_trenc = version < 5 && mode != 0;

        let mut blob = Vec::new();
        reader.read_to_end(&mut blob)?;

        let mut entries = Vec::with_capacity(count);
        for &(hash, offset) in &raw {
            entries.push((hash, read_value_at(&blob, offset, has_trenc)?));
        }

        let mut distinct_offsets: Vec<usize> = raw.iter().map(|&(_, off)| off).collect();
        distinct_offsets.sort_unstable();
        distinct_offsets.dedup();
        let mut blob_order = Vec::with_capacity(distinct_offsets.len());
        for offset in distinct_offsets {
            blob_order.push(read_value_at(&blob, offset, has_trenc)?);
        }

        Ok(Rst {
            version,
            font_config,
            mode,
            entries,
            blob_order,
        })
    }
}

/** Decodes one entry at `offset`. With `has_trenc` set, a leading `0xFF` marks a legacy encrypted
payload laid out as `[0xFF][u16 length][length bytes]`; its raw bytes are kept verbatim. Otherwise
the value is a NUL-terminated UTF-8 string. */
fn read_value_at(blob: &[u8], offset: usize, has_trenc: bool) -> Result<RstValue> {
    let start = blob.get(offset..).ok_or(eof(offset, 1, blob.len()))?;

    if has_trenc && start.first() == Some(&0xFF) {
        let len_bytes = start.get(1..3).ok_or(eof(offset, 3, start.len()))?;
        let size = u16::from_le_bytes([len_bytes[0], len_bytes[1]]) as usize;
        let payload = start
            .get(3..3 + size)
            .ok_or(eof(offset + 3, size, start.len().saturating_sub(3)))?;
        return Ok(RstValue::Encrypted(payload.to_vec()));
    }

    let end = start.iter().position(|&b| b == 0).unwrap_or(start.len());
    Ok(RstValue::Text(
        String::from_utf8(start[..end].to_vec()).map_err(rs_io::Error::from)?,
    ))
}

fn eof(offset: usize, needed: usize, available: usize) -> Error {
    Error::Io(rs_io::Error::UnexpectedEof {
        offset,
        needed,
        available,
    })
}
