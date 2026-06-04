use std::io::Write;

use rs_io::{Serialize, WriterExt};

use crate::error::{Error, Result};
use crate::luabin::{LuaBin, LuaConstant, Proto};

const SIGNATURE: [u8; 4] = *b"\x1bLua";

struct Layout {
    int_size: u8,
    size_t_size: u8,
}

impl Serialize for LuaBin {
    type Error = Error;

    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_bytes(&SIGNATURE)?;
        writer.write_u8(self.version)?;
        writer.write_u8(self.format)?;
        writer.write_u8(self.endian)?;
        writer.write_u8(self.int_size)?;
        writer.write_u8(self.size_t_size)?;
        writer.write_u8(self.instruction_size)?;
        writer.write_u8(self.number_size)?;
        writer.write_u8(self.is_integral)?;

        let layout = Layout {
            int_size: self.int_size,
            size_t_size: self.size_t_size,
        };
        write_proto(writer, &layout, &self.main)
    }
}

fn write_proto<W: Write>(w: &mut W, layout: &Layout, proto: &Proto) -> Result<()> {
    write_string(w, layout.size_t_size, proto.source.as_deref())?;
    write_int(w, layout.int_size, proto.line_defined)?;
    write_int(w, layout.int_size, proto.last_line_defined)?;
    w.write_u8(proto.num_upvalues)?;
    w.write_u8(proto.num_params)?;
    w.write_u8(proto.is_vararg)?;
    w.write_u8(proto.max_stack)?;

    write_int(w, layout.int_size, proto.code.len() as i64)?;
    for &word in &proto.code {
        w.write_u32(word)?;
    }

    write_int(w, layout.int_size, proto.constants.len() as i64)?;
    for constant in &proto.constants {
        write_constant(w, layout, constant)?;
    }

    write_int(w, layout.int_size, proto.protos.len() as i64)?;
    for child in &proto.protos {
        write_proto(w, layout, child)?;
    }

    write_int(w, layout.int_size, proto.line_info.len() as i64)?;
    for &line in &proto.line_info {
        write_int(w, layout.int_size, line)?;
    }

    write_int(w, layout.int_size, proto.locals.len() as i64)?;
    for local in &proto.locals {
        write_string(w, layout.size_t_size, local.name.as_deref())?;
        write_int(w, layout.int_size, local.start_pc)?;
        write_int(w, layout.int_size, local.end_pc)?;
    }

    write_int(w, layout.int_size, proto.upvalue_names.len() as i64)?;
    for name in &proto.upvalue_names {
        write_string(w, layout.size_t_size, name.as_deref())?;
    }

    Ok(())
}

fn write_constant<W: Write>(w: &mut W, layout: &Layout, constant: &LuaConstant) -> Result<()> {
    match constant {
        LuaConstant::Nil => w.write_u8(0)?,
        LuaConstant::Bool(b) => {
            w.write_u8(1)?;
            w.write_u8(*b)?;
        }
        LuaConstant::Number(bytes) => {
            w.write_u8(3)?;
            w.write_bytes(bytes)?;
        }
        LuaConstant::Str(value) => {
            w.write_u8(4)?;
            write_string(w, layout.size_t_size, value.as_deref())?;
        }
    }
    Ok(())
}

fn write_string<W: Write>(w: &mut W, size_t_size: u8, value: Option<&[u8]>) -> Result<()> {
    match value {
        None => write_size_t(w, size_t_size, 0),
        Some(bytes) => {
            write_size_t(w, size_t_size, bytes.len() as u64)?;
            w.write_bytes(bytes)?;
            Ok(())
        }
    }
}

fn write_int<W: Write>(w: &mut W, size: u8, value: i64) -> Result<()> {
    match size {
        4 => w.write_i32(value as i32)?,
        8 => w.write_i64(value)?,
        _ => return Err(Error::Unsupported("int size not 4 or 8")),
    }
    Ok(())
}

fn write_size_t<W: Write>(w: &mut W, size: u8, value: u64) -> Result<()> {
    match size {
        4 => w.write_u32(value as u32)?,
        8 => w.write_u64(value)?,
        _ => return Err(Error::Unsupported("size_t size not 4 or 8")),
    }
    Ok(())
}
