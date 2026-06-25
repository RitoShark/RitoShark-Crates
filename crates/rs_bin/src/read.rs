use std::io::{Read, Seek};

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
            let start = reader.stream_position()?;
            let path_hash = reader.read_u32()?;
            let field_count = reader.read_u16()? as usize;
            let mut fields = IndexMap::with_capacity(field_count.min(1 << 16));
            for _ in 0..field_count {
                let name = reader.read_u32()?;
                let ty = BinType::from_u8(reader.read_u8()?)?;
                fields.insert(name, read_value(reader, ty)?);
            }
            let consumed = reader.stream_position()? - start;
            if consumed != length {
                return Err(Error::SizeMismatch {
                    declared: length as usize,
                    actual: consumed as usize,
                });
            }
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

        Ok(Bin {
            is_patch,
            patch_header,
            version,
            linked,
            entries,
            patches,
        })
    }
}

fn read_patch<R: Read + Seek>(reader: &mut R) -> Result<BinPatch> {
    let key_hash = reader.read_u32()?;
    let length = reader.read_u32()? as u64;
    let start = reader.stream_position()?;
    let ty = BinType::from_u8(reader.read_u8()?)?;
    let path = reader.read_string_u16()?;
    let value = read_value(reader, ty)?;
    check_size(reader, start, length)?;
    Ok(BinPatch {
        key_hash,
        path,
        value,
    })
}

fn read_value<R: Read + Seek>(reader: &mut R, ty: BinType) -> Result<BinValue> {
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
        BinType::Hash => BinValue::Hash(reader.read_u32()?),
        BinType::File => BinValue::File(reader.read_u64()?),
        BinType::Link => BinValue::Link(reader.read_u32()?),
        BinType::Flag => BinValue::Flag(reader.read_bool()?),
        BinType::List | BinType::List2 => read_list(reader, ty == BinType::List2)?,
        BinType::Map => read_map(reader)?,
        BinType::Option => read_option(reader)?,
        BinType::Pointer => read_struct(reader, false)?,
        BinType::Embed => read_struct(reader, true)?,
    })
}

fn read_list<R: Read + Seek>(reader: &mut R, is_list2: bool) -> Result<BinValue> {
    let item = BinType::from_u8(reader.read_u8()?)?;
    if item.is_container() {
        return Err(Error::NestedContainer(item.to_u8()));
    }
    let size = reader.read_u32()? as u64;
    let start = reader.stream_position()?;
    let count = reader.read_u32()? as usize;
    let mut items = Vec::with_capacity(count.min(1 << 24));
    for _ in 0..count {
        items.push(read_value(reader, item)?);
    }
    check_size(reader, start, size)?;
    Ok(BinValue::List {
        is_list2,
        item,
        items,
    })
}

fn read_map<R: Read + Seek>(reader: &mut R) -> Result<BinValue> {
    let key = BinType::from_u8(reader.read_u8()?)?;
    let value = BinType::from_u8(reader.read_u8()?)?;
    if !key.is_primitive() {
        return Err(Error::NestedContainer(key.to_u8()));
    }
    if value.is_container() {
        return Err(Error::NestedContainer(value.to_u8()));
    }
    let size = reader.read_u32()? as u64;
    let start = reader.stream_position()?;
    let count = reader.read_u32()? as usize;
    let mut entries = Vec::with_capacity(count.min(1 << 24));
    for _ in 0..count {
        let k = read_value(reader, key)?;
        let v = read_value(reader, value)?;
        entries.push((k, v));
    }
    check_size(reader, start, size)?;
    Ok(BinValue::Map {
        key,
        value,
        entries,
    })
}

fn read_option<R: Read + Seek>(reader: &mut R) -> Result<BinValue> {
    let item = BinType::from_u8(reader.read_u8()?)?;
    if item.is_container() {
        return Err(Error::NestedContainer(item.to_u8()));
    }
    let count = reader.read_u8()?;
    let value = match count {
        0 => None,
        1 => Some(Box::new(read_value(reader, item)?)),
        other => return Err(Error::InvalidOptionCount(other)),
    };
    Ok(BinValue::Option { item, value })
}

fn read_struct<R: Read + Seek>(reader: &mut R, is_embed: bool) -> Result<BinValue> {
    let class = reader.read_u32()?;
    if !is_embed && class == 0 {
        return Ok(BinValue::Pointer {
            class: 0,
            fields: IndexMap::new(),
        });
    }
    let size = reader.read_u32()? as u64;
    let start = reader.stream_position()?;
    let field_count = reader.read_u16()? as usize;
    let mut fields = IndexMap::with_capacity(field_count.min(1 << 16));
    for _ in 0..field_count {
        let name = reader.read_u32()?;
        let ty = BinType::from_u8(reader.read_u8()?)?;
        fields.insert(name, read_value(reader, ty)?);
    }
    check_size(reader, start, size)?;
    if is_embed {
        Ok(BinValue::Embed { class, fields })
    } else {
        Ok(BinValue::Pointer { class, fields })
    }
}

fn check_size<R: Seek>(reader: &mut R, start: u64, declared: u64) -> Result<()> {
    let consumed = reader.stream_position()? - start;
    if consumed != declared {
        return Err(Error::SizeMismatch {
            declared: declared as usize,
            actual: consumed as usize,
        });
    }
    Ok(())
}
