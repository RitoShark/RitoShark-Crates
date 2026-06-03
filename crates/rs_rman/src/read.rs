use std::io::{Read, Seek};

use rs_io::{Parse, ReaderExt};

use crate::error::{Error, Result};
use crate::rman::{Bundle, Chunk, Directory, FileEntry, Rman};

const MAGIC: [u8; 4] = *b"RMAN";
const HEADER_LEN: u32 = 4 + 1 + 1 + 2 + 4 + 4 + 8 + 4;

impl Parse for Rman {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let magic = reader.read_array::<4>()?;
        if magic != MAGIC {
            return Err(Error::InvalidMagic(magic));
        }
        let major = reader.read_u8()?;
        let minor = reader.read_u8()?;
        if (major, minor) != (2, 0) {
            return Err(Error::UnsupportedVersion(major, minor));
        }

        let flags = reader.read_u16()?;
        let offset = reader.read_u32()?;
        let compressed_size = reader.read_u32()?;
        let manifest_id = reader.read_u64()?;
        let _uncompressed_size = reader.read_u32()?;

        if offset < HEADER_LEN {
            return Err(Error::Malformed("body offset overlaps header"));
        }
        if offset > HEADER_LEN {
            let mut gap = vec![0u8; (offset - HEADER_LEN) as usize];
            reader.read_exact(&mut gap)?;
        }

        let compressed = reader.read_bytes(compressed_size as usize)?;
        let body = zstd::stream::decode_all(compressed.as_slice())
            .map_err(|e| Error::Decompress(e.to_string()))?;

        let (bundles, files, directories) = parse_body(&body)?;

        Ok(Rman {
            version: (major, minor),
            flags,
            manifest_id,
            bundles,
            files,
            directories,
        })
    }
}

fn parse_body(body: &[u8]) -> Result<(Vec<Bundle>, Vec<FileEntry>, Vec<Directory>)> {
    let mut root = Cursor::new(body, 0);
    let header_len = root.read_i32()?;
    root.skip(header_len)?;

    let bundles_off = root.read_offset()?;
    let _flags_off = root.read_offset()?;
    let files_off = root.read_offset()?;
    let dirs_off = root.read_offset()?;

    let bundles = parse_table(body, bundles_off, parse_bundle)?;
    let files = parse_table(body, files_off, parse_file)?;
    let directories = parse_table(body, dirs_off, parse_directory)?;

    Ok((bundles, files, directories))
}

fn parse_table<T>(
    body: &[u8],
    offset: i32,
    parse: fn(Cursor<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    let mut cursor = Cursor::new(body, offset);
    let count = cursor.read_u32()? as usize;
    let mut out = Vec::with_capacity(count.min(1 << 20));
    for _ in 0..count {
        let entry = cursor.subcursor()?;
        out.push(parse(entry)?);
    }
    Ok(out)
}

fn parse_bundle(cursor: Cursor<'_>) -> Result<Bundle> {
    let fields = cursor.fields()?;
    let id = fields.get_u64(0)?.ok_or(Error::Malformed("bundle id"))?;
    let chunks = match fields.offset_cursor(1)? {
        Some(c) => parse_chunks(c)?,
        None => Vec::new(),
    };
    Ok(Bundle { id, chunks })
}

fn parse_chunks(mut cursor: Cursor<'_>) -> Result<Vec<Chunk>> {
    let count = cursor.read_u32()? as usize;
    let mut out = Vec::with_capacity(count.min(1 << 24));
    for _ in 0..count {
        let entry = cursor.subcursor()?;
        let fields = entry.fields()?;
        let id = fields.get_u64(0)?.ok_or(Error::Malformed("chunk id"))?;
        let compressed_size = fields
            .get_u32(1)?
            .ok_or(Error::Malformed("chunk compressed size"))?;
        let uncompressed_size = fields
            .get_u32(2)?
            .ok_or(Error::Malformed("chunk uncompressed size"))?;
        out.push(Chunk {
            id,
            compressed_size,
            uncompressed_size,
        });
    }
    Ok(out)
}

fn parse_file(cursor: Cursor<'_>) -> Result<FileEntry> {
    let fields = cursor.fields()?;
    let id = fields.get_u64(0)?.ok_or(Error::Malformed("file id"))?;
    let directory_id = fields.get_u64(1)?;
    let size = fields.get_u32(2)?.ok_or(Error::Malformed("file size"))?;
    let name = fields.get_str(3)?.ok_or(Error::Malformed("file name"))?;
    let chunk_ids = match fields.offset_cursor(7)? {
        Some(c) => parse_chunk_ids(c)?,
        None => Vec::new(),
    };
    let link = fields.get_str(9)?.filter(|s| !s.is_empty());
    let permissions = fields.get_u8(12)?.unwrap_or(0);

    Ok(FileEntry {
        id,
        name,
        size,
        directory_id,
        chunk_ids,
        link,
        permissions,
    })
}

fn parse_chunk_ids(mut cursor: Cursor<'_>) -> Result<Vec<u64>> {
    let count = cursor.read_u32()? as usize;
    let mut out = Vec::with_capacity(count.min(1 << 24));
    for _ in 0..count {
        out.push(cursor.read_u64()?);
    }
    Ok(out)
}

fn parse_directory(cursor: Cursor<'_>) -> Result<Directory> {
    let fields = cursor.fields()?;
    let id = fields.get_u64(0)?.unwrap_or(0);
    let parent_id = fields.get_u64(1)?;
    let name = fields.get_str(2)?.ok_or(Error::Malformed("directory name"))?;
    Ok(Directory {
        id,
        parent_id,
        name,
    })
}

/** Bounds-checked walker over the decompressed FlatBuffer body. RMAN uses signed self-relative
offsets and a vtable per table, so the cursor tracks an `i32` position into the body and every
read validates the range before slicing, turning malformed input into an error instead of a
panic. */
#[derive(Clone, Copy)]
struct Cursor<'a> {
    body: &'a [u8],
    offset: i32,
}

impl<'a> Cursor<'a> {
    fn new(body: &'a [u8], offset: i32) -> Self {
        Self { body, offset }
    }

    fn slice(&self, at: i32, n: i32) -> Result<&'a [u8]> {
        if at < 0 || n < 0 {
            return Err(Error::Malformed("negative offset"));
        }
        let start = at as usize;
        let end = start
            .checked_add(n as usize)
            .ok_or(Error::Malformed("offset overflow"))?;
        self.body
            .get(start..end)
            .ok_or(Error::Malformed("read past end of body"))
    }

    fn read_slice(&mut self, n: i32) -> Result<&'a [u8]> {
        let s = self.slice(self.offset, n)?;
        self.offset += n;
        Ok(s)
    }

    fn skip(&mut self, n: i32) -> Result<()> {
        let next = self
            .offset
            .checked_add(n)
            .ok_or(Error::Malformed("offset overflow"))?;
        if next < 0 {
            return Err(Error::Malformed("negative offset"));
        }
        self.offset = next;
        Ok(())
    }

    fn read_i32(&mut self) -> Result<i32> {
        let b = self.read_slice(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_u32(&mut self) -> Result<u32> {
        let b = self.read_slice(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_u64(&mut self) -> Result<u64> {
        let b = self.read_slice(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// Read a self-relative i32 offset and return the absolute body offset it points at.
    fn read_offset(&mut self) -> Result<i32> {
        let base = self.offset;
        let rel = self.read_i32()?;
        base.checked_add(rel).ok_or(Error::Malformed("offset overflow"))
    }

    /// Follow a self-relative offset into a fresh cursor.
    fn subcursor(&mut self) -> Result<Cursor<'a>> {
        Ok(Cursor::new(self.body, self.read_offset()?))
    }

    /// Resolve this entry's vtable and expose its indexed fields.
    fn fields(mut self) -> Result<Fields<'a>> {
        let entry = self.offset;
        let vtable_rel = self.read_i32()?;
        let vtable = entry
            .checked_sub(vtable_rel)
            .ok_or(Error::Malformed("vtable offset overflow"))?
            .checked_add(4)
            .ok_or(Error::Malformed("vtable offset overflow"))?;
        Ok(Fields {
            body: self.body,
            vtable,
            entry,
        })
    }
}

/** Indexed-field view of one FlatBuffer table. `vtable` points past the two `u16` header
entries to the field-offset array; each field's `u16` slot is its byte offset from `entry`,
where `0` means the field is absent. */
struct Fields<'a> {
    body: &'a [u8],
    vtable: i32,
    entry: i32,
}

impl<'a> Fields<'a> {
    fn cursor(&self, offset: i32) -> Cursor<'a> {
        Cursor::new(self.body, offset)
    }

    fn field_offset(&self, field: u8) -> Result<i32> {
        let at = self
            .vtable
            .checked_add(2 * field as i32)
            .ok_or(Error::Malformed("field offset overflow"))?;
        let s = self.cursor(0).slice(at, 2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]) as i32)
    }

    fn field_at(&self, field: u8) -> Result<Option<i32>> {
        match self.field_offset(field)? {
            0 => Ok(None),
            o => Ok(Some(
                self.entry
                    .checked_add(o)
                    .ok_or(Error::Malformed("field position overflow"))?,
            )),
        }
    }

    fn get_u8(&self, field: u8) -> Result<Option<u8>> {
        match self.field_at(field)? {
            Some(at) => Ok(Some(self.cursor(0).slice(at, 1)?[0])),
            None => Ok(None),
        }
    }

    fn get_u32(&self, field: u8) -> Result<Option<u32>> {
        match self.field_at(field)? {
            Some(at) => {
                let s = self.cursor(0).slice(at, 4)?;
                Ok(Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]])))
            }
            None => Ok(None),
        }
    }

    fn get_u64(&self, field: u8) -> Result<Option<u64>> {
        match self.field_at(field)? {
            Some(at) => {
                let s = self.cursor(0).slice(at, 8)?;
                Ok(Some(u64::from_le_bytes([
                    s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
                ])))
            }
            None => Ok(None),
        }
    }

    /// Read the cursor a field's self-relative offset points to.
    fn offset_cursor(&self, field: u8) -> Result<Option<Cursor<'a>>> {
        match self.field_at(field)? {
            Some(at) => {
                let mut c = self.cursor(at);
                Ok(Some(Cursor::new(self.body, c.read_offset()?)))
            }
            None => Ok(None),
        }
    }

    /// Follow a field's offset to a length-prefixed UTF-8 string.
    fn get_str(&self, field: u8) -> Result<Option<String>> {
        match self.offset_cursor(field)? {
            Some(mut c) => {
                let len = c.read_i32()?;
                let bytes = c.read_slice(len)?;
                let s = std::str::from_utf8(bytes)
                    .map_err(|_| Error::Malformed("invalid utf-8 string"))?;
                Ok(Some(s.to_owned()))
            }
            None => Ok(None),
        }
    }
}
