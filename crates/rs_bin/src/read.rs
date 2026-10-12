use std::io::{Cursor, Read, Seek};

use indexmap::IndexMap;
use rs_io::{Parse, ReaderExt};

use crate::bin::{Bin, BinEntry, BinPatch, BinType, BinValue};
use crate::error::{Error, Result};

const PROP: [u8; 4] = *b"PROP";
const PTCH: [u8; 4] = *b"PTCH";

impl Parse for Bin {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut magic = reader.read_byte_array::<4>()?;
        let mut is_patch = false;
        let mut patch_header = [0u8; 8];
        if magic == PTCH {
            is_patch = true;
            patch_header = reader.read_byte_array::<8>()?;
            magic = reader.read_byte_array::<4>()?;
        }
        if magic != PROP {
            return Err(Error::InvalidMagic(magic));
        }

        let version = reader.read_u32()?;

        let mut linked = Vec::new();
        if version >= 2 {
            let count = reader.read_u32()? as usize;
            linked.reserve(count.min(1 << 16));
            for _ in 0..count {
                linked.push(reader.read_string_u16()?);
            }
        }

        let entry_count = reader.read_u32()? as usize;
        let mut class_hashes = Vec::with_capacity(entry_count.min(1 << 20));
        for _ in 0..entry_count {
            class_hashes.push(reader.read_u32()?);
        }

        let mut entries = Vec::with_capacity(entry_count.min(1 << 20));
        for class_hash in class_hashes {
            let length = reader.read_u32()? as u64;
            let mut budget = Budget::new(length);
            let (path_hash, fields) = region(reader, length, &mut budget, |r, choices, budget| {
                let path_hash = r.read_u32()?;
                Ok((path_hash, read_fields(r, choices, budget)?))
            })?;
            entries.push(BinEntry {
                path_hash,
                class_hash,
                fields,
            });
        }

        let mut patches = Vec::new();
        if is_patch {
            let patch_count = reader.read_u32()? as usize;
            patches.reserve(patch_count.min(1 << 20));
            for _ in 0..patch_count {
                patches.push(read_patch(reader)?);
            }
        }

        // Capture any bytes after the declared body (entries + patches). The bin
        // format has no whole-file length, so tools may append arbitrary trailing
        // data (e.g. a hash->path side table) that the game and this parser ignore.
        // Preserving it here means a read -> write round-trip never drops it.
        let mut trailing = Vec::new();
        reader.read_to_end(&mut trailing)?;

        Ok(Bin {
            is_patch,
            patch_header,
            version,
            linked,
            entries,
            patches,
            trailing,
        })
    }
}

fn read_patch<R: Read + Seek>(reader: &mut R) -> Result<BinPatch> {
    let key_hash = reader.read_u32()?;
    let length = reader.read_u32()? as u64;
    let mut budget = Budget::new(length);
    let (path, value) = region(reader, length, &mut budget, |r, choices, budget| {
        let ty = BinType::from_u8(r.read_u8()?)?;
        let path = r.read_string_u16()?;
        Ok((path, read_value(r, ty, choices, budget)?))
    })?;
    Ok(BinPatch {
        key_hash,
        path,
        value,
    })
}

fn read_value<R: Read + Seek>(
    reader: &mut R,
    ty: BinType,
    choices: &mut Choices,
    budget: &mut Budget,
) -> Result<BinValue> {
    budget.step()?;
    Ok(match ty {
        BinType::None => BinValue::None,
        BinType::Bool => BinValue::Bool(reader.read_bool()?),
        BinType::I8 => BinValue::I8(reader.read_i8()?),
        BinType::U8 => BinValue::U8(reader.read_u8()?),
        BinType::I16 => BinValue::I16(reader.read_i16()?),
        BinType::U16 => BinValue::U16(reader.read_u16()?),
        BinType::I32 => BinValue::I32(reader.read_i32()?),
        BinType::U32 => BinValue::U32(reader.read_u32()?),
        BinType::I64 => BinValue::I64(reader.read_i64()?),
        BinType::U64 => BinValue::U64(reader.read_u64()?),
        BinType::F32 => BinValue::F32(reader.read_f32()?),
        BinType::Vec2 => BinValue::Vec2([reader.read_f32()?, reader.read_f32()?]),
        BinType::Vec3 => {
            BinValue::Vec3([reader.read_f32()?, reader.read_f32()?, reader.read_f32()?])
        }
        BinType::Vec4 => BinValue::Vec4([
            reader.read_f32()?,
            reader.read_f32()?,
            reader.read_f32()?,
            reader.read_f32()?,
        ]),
        BinType::Mtx44 => BinValue::Mtx44(reader.read_mtx44()?),
        BinType::Rgba => BinValue::Rgba(reader.read_byte_array::<4>()?),
        BinType::String => BinValue::String(reader.read_string_u16()?),
        BinType::Hash => read_hash(reader, choices.next())?,
        BinType::File => BinValue::File(reader.read_u64()?),
        BinType::Link => BinValue::Link(reader.read_u32()?),
        BinType::Flag => BinValue::Flag(reader.read_bool()?),
        BinType::List | BinType::List2 => read_list(reader, ty == BinType::List2, budget)?,
        BinType::Map => read_map(reader, budget)?,
        BinType::Option => read_option(reader, choices, budget)?,
        BinType::Pointer => read_struct(reader, false, budget)?,
        BinType::Embed => read_struct(reader, true, budget)?,
    })
}

// Width is not encoded in the Hash tag. Resolve each sized region independently,
// trying the legacy four-byte layout first. A shared budget bounds all retries,
// including nested regions. Field names, class names and object links remain u32.
#[derive(Default)]
struct Choices {
    wide: Vec<bool>,
    index: usize,
}
impl Choices {
    fn next(&mut self) -> bool {
        if self.index == self.wide.len() {
            self.wide.push(false);
        }
        let wide = self.wide[self.index];
        self.index += 1;
        wide
    }
    fn advance(&mut self) -> bool {
        let Some(i) = self.wide.iter().rposition(|wide| !wide) else {
            return false;
        };
        self.wide[i] = true;
        self.wide.truncate(i + 1);
        true
    }
}
struct Budget {
    remaining: u64,
    depth: usize,
}
impl Budget {
    fn new(size: u64) -> Self {
        Self {
            remaining: size.saturating_mul(64).saturating_add(1024).min(16_000_000),
            depth: 0,
        }
    }
    fn step(&mut self) -> Result<()> {
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or(Error::Unsupported("hash-width search limit"))?;
        Ok(())
    }
}
fn region<R: Read, T>(
    reader: &mut R,
    size: u64,
    budget: &mut Budget,
    mut parse: impl FnMut(&mut Cursor<&[u8]>, &mut Choices, &mut Budget) -> Result<T>,
) -> Result<T> {
    budget.step()?;
    if size > 128 * 1024 * 1024 || budget.depth >= 128 {
        return Err(Error::Unsupported("BIN region size or nesting limit"));
    }
    let mut bytes = Vec::new();
    reader.take(size).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size {
        return Err(Error::SizeMismatch {
            declared: size as usize,
            actual: bytes.len(),
        });
    }
    budget.depth += 1;
    let mut choices = Choices::default();
    let mut first_error = None;
    let result = loop {
        let mut r = Cursor::new(bytes.as_slice());
        choices.index = 0;
        let parsed = parse(&mut r, &mut choices, budget);
        let error = match parsed {
            Ok(value) if r.position() == size => break Ok(value),
            Ok(_) => Error::SizeMismatch {
                declared: size as usize,
                actual: r.position() as usize,
            },
            Err(error) => error,
        };
        first_error.get_or_insert(error);
        if budget.remaining == 0 || !choices.advance() {
            break Err(first_error.take().unwrap());
        }
        budget.remaining -= 1;
    };
    budget.depth -= 1;
    result
}
fn read_hash<R: Read>(reader: &mut R, wide: bool) -> Result<BinValue> {
    Ok(if wide {
        BinValue::Hash64(reader.read_u64()?)
    } else {
        BinValue::Hash(reader.read_u32()?)
    })
}
fn read_fields<R: Read + Seek>(
    reader: &mut R,
    choices: &mut Choices,
    budget: &mut Budget,
) -> Result<IndexMap<u32, BinValue>> {
    let count = reader.read_u16()? as usize;
    let mut fields = IndexMap::with_capacity(count.min(1024));
    for _ in 0..count {
        let name = reader.read_u32()?;
        let ty = BinType::from_u8(reader.read_u8()?)?;
        fields.insert(name, read_value(reader, ty, choices, budget)?);
    }
    Ok(fields)
}
fn read_list<R: Read + Seek>(
    reader: &mut R,
    is_list2: bool,
    budget: &mut Budget,
) -> Result<BinValue> {
    let item = BinType::from_u8(reader.read_u8()?)?;
    if item.is_container() {
        return Err(Error::NestedContainer(item.to_u8()));
    }
    let size = reader.read_u32()? as u64;
    let items = region(reader, size, budget, |r, choices, budget| {
        let count = r.read_u32()? as usize;
        let mut items = Vec::with_capacity(count.min(1024));
        let width = if item == BinType::Hash && count > 0 {
            let payload = size
                .checked_sub(4)
                .ok_or(Error::Unsupported("short hash list"))?;
            if payload % count as u64 != 0 || !matches!(payload / count as u64, 4 | 8) {
                return Err(Error::Unsupported("invalid hash list width"));
            }
            payload / count as u64
        } else {
            4
        };
        for _ in 0..count {
            items.push(if item == BinType::Hash {
                budget.step()?;
                read_hash(r, width == 8)?
            } else {
                read_value(r, item, choices, budget)?
            });
        }
        Ok(items)
    })?;
    Ok(BinValue::List {
        is_list2,
        item,
        items,
    })
}
fn read_map<R: Read + Seek>(reader: &mut R, budget: &mut Budget) -> Result<BinValue> {
    let key = BinType::from_u8(reader.read_u8()?)?;
    let value = BinType::from_u8(reader.read_u8()?)?;
    if !key.is_primitive() {
        return Err(Error::NestedContainer(key.to_u8()));
    }
    if value.is_container() {
        return Err(Error::NestedContainer(value.to_u8()));
    }
    let size = reader.read_u32()? as u64;
    let entries = region(reader, size, budget, |r, choices, budget| {
        let count = r.read_u32()? as usize;
        if key == BinType::Hash
            && value == BinType::Hash
            && count > 0
            && size == 4 + 12 * count as u64
        {
            return Err(Error::Unsupported("ambiguous hash map widths (4+8 or 8+4)"));
        }
        // Each map column has one width, shared by every entry.
        let key_wide = key == BinType::Hash && choices.next();
        let value_wide = value == BinType::Hash && choices.next();
        let mut entries = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            budget.step()?;
            let k = if key == BinType::Hash {
                read_hash(r, key_wide)?
            } else {
                read_value(r, key, choices, budget)?
            };
            let v = if value == BinType::Hash {
                read_hash(r, value_wide)?
            } else {
                read_value(r, value, choices, budget)?
            };
            entries.push((k, v));
        }
        Ok(entries)
    })?;
    Ok(BinValue::Map {
        key,
        value,
        entries,
    })
}
fn read_option<R: Read + Seek>(
    reader: &mut R,
    choices: &mut Choices,
    budget: &mut Budget,
) -> Result<BinValue> {
    let item = BinType::from_u8(reader.read_u8()?)?;
    if item.is_container() {
        return Err(Error::NestedContainer(item.to_u8()));
    }
    let value = match reader.read_u8()? {
        0 => None,
        1 => Some(Box::new(read_value(reader, item, choices, budget)?)),
        n => return Err(Error::InvalidOptionCount(n)),
    };
    Ok(BinValue::Option { item, value })
}
fn read_struct<R: Read + Seek>(
    reader: &mut R,
    is_embed: bool,
    budget: &mut Budget,
) -> Result<BinValue> {
    let class = reader.read_u32()?;
    if !is_embed && class == 0 {
        return Ok(BinValue::Pointer {
            class,
            fields: IndexMap::new(),
        });
    }
    let size = reader.read_u32()? as u64;
    let fields = region(reader, size, budget, |r, c, b| read_fields(r, c, b))?;
    Ok(if is_embed {
        BinValue::Embed { class, fields }
    } else {
        BinValue::Pointer { class, fields }
    })
}
