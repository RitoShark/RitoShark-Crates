use std::io::Write;

use indexmap::IndexMap;
use rs_io::{Serialize, WriterExt};

use crate::bin::{Bin, BinValue};
use crate::error::{Error, Result};

impl Serialize for Bin {
    type Error = Error;

    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        if self.is_patch {
            writer.write_bytes(b"PTCH")?;
            writer.write_bytes(&self.patch_header)?;
        }
        writer.write_bytes(b"PROP")?;
        writer.write_u32(self.version)?;

        if self.version >= 2 {
            writer.write_u32(len_u32(self.linked.len(), "linked-files count")?)?;
            for path in &self.linked {
                if path.len() > u16::MAX as usize {
                    return Err(Error::TooLarge("linked-file path"));
                }
                writer.write_string_u16(path)?;
            }
        }

        writer.write_u32(len_u32(self.entries.len(), "entry count")?)?;
        for entry in &self.entries {
            writer.write_u32(entry.class_hash)?;
        }

        for entry in &self.entries {
            let mut body = Vec::new();
            body.write_u32(entry.path_hash)?;
            write_fields(&mut body, &entry.fields)?;
            writer.write_u32(len_u32(body.len(), "entry length")?)?;
            writer.write_bytes(&body)?;
        }

        if self.is_patch {
            writer.write_u32(len_u32(self.patches.len(), "patch count")?)?;
            for patch in &self.patches {
                writer.write_u32(patch.key_hash)?;
                let mut body = Vec::new();
                body.write_u8(patch.value.ty().to_u8())?;
                if patch.path.len() > u16::MAX as usize {
                    return Err(Error::TooLarge("patch path"));
                }
                body.write_string_u16(&patch.path)?;
                write_value(&mut body, &patch.value)?;
                writer.write_u32(len_u32(body.len(), "patch length")?)?;
                writer.write_bytes(&body)?;
            }
        }

        Ok(())
    }
}

fn write_fields<W: Write>(writer: &mut W, fields: &IndexMap<u32, BinValue>) -> Result<()> {
    writer.write_u16(
        u16::try_from(fields.len()).map_err(|_| Error::TooLarge("struct field count"))?,
    )?;
    for (name, value) in fields {
        writer.write_u32(*name)?;
        writer.write_u8(value.ty().to_u8())?;
        write_value(writer, value)?;
    }
    Ok(())
}

fn write_value<W: Write>(writer: &mut W, value: &BinValue) -> Result<()> {
    match value {
        BinValue::None => {}
        BinValue::Bool(v) => writer.write_bool(*v)?,
        BinValue::I8(v) => writer.write_i8(*v)?,
        BinValue::U8(v) => writer.write_u8(*v)?,
        BinValue::I16(v) => writer.write_i16(*v)?,
        BinValue::U16(v) => writer.write_u16(*v)?,
        BinValue::I32(v) => writer.write_i32(*v)?,
        BinValue::U32(v) => writer.write_u32(*v)?,
        BinValue::I64(v) => writer.write_i64(*v)?,
        BinValue::U64(v) => writer.write_u64(*v)?,
        BinValue::F32(v) => writer.write_f32(*v)?,
        BinValue::Vec2(a) => {
            for &f in a {
                writer.write_f32(f)?;
            }
        }
        BinValue::Vec3(a) => {
            for &f in a {
                writer.write_f32(f)?;
            }
        }
        BinValue::Vec4(a) => {
            for &f in a {
                writer.write_f32(f)?;
            }
        }
        BinValue::Mtx44(a) => writer.write_mtx44(a)?,
        BinValue::Rgba(a) => writer.write_bytes(a)?,
        BinValue::String(s) => {
            if s.len() > u16::MAX as usize {
                return Err(Error::TooLarge("string"));
            }
            writer.write_string_u16(s)?;
        }
        BinValue::Hash(v) => writer.write_u32(*v)?,
        BinValue::File(v) => writer.write_u64(*v)?,
        BinValue::Link(v) => writer.write_u32(*v)?,
        BinValue::Flag(v) => writer.write_bool(*v)?,
        BinValue::List { item, items, .. } => {
            writer.write_u8(item.to_u8())?;
            let mut body = Vec::new();
            body.write_u32(len_u32(items.len(), "list count")?)?;
            for v in items {
                write_value(&mut body, v)?;
            }
            writer.write_u32(len_u32(body.len(), "list size")?)?;
            writer.write_bytes(&body)?;
        }
        BinValue::Map {
            key,
            value,
            entries,
        } => {
            writer.write_u8(key.to_u8())?;
            writer.write_u8(value.to_u8())?;
            let mut body = Vec::new();
            body.write_u32(len_u32(entries.len(), "map count")?)?;
            for (k, v) in entries {
                write_value(&mut body, k)?;
                write_value(&mut body, v)?;
            }
            writer.write_u32(len_u32(body.len(), "map size")?)?;
            writer.write_bytes(&body)?;
        }
        BinValue::Option { item, value } => {
            writer.write_u8(item.to_u8())?;
            match value {
                None => writer.write_u8(0)?,
                Some(v) => {
                    writer.write_u8(1)?;
                    write_value(writer, v)?;
                }
            }
        }
        BinValue::Pointer { class, fields } => {
            writer.write_u32(*class)?;
            if *class == 0 {
                return Ok(());
            }
            let mut body = Vec::new();
            write_fields(&mut body, fields)?;
            writer.write_u32(len_u32(body.len(), "pointer size")?)?;
            writer.write_bytes(&body)?;
        }
        BinValue::Embed { class, fields } => {
            writer.write_u32(*class)?;
            let mut body = Vec::new();
            write_fields(&mut body, fields)?;
            writer.write_u32(len_u32(body.len(), "embed size")?)?;
            writer.write_bytes(&body)?;
        }
    }
    Ok(())
}

fn len_u32(n: usize, what: &'static str) -> Result<u32> {
    u32::try_from(n).map_err(|_| Error::TooLarge(what))
}
