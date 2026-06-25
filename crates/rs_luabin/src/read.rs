use std::io::{Read, Seek};

use rs_io::{Parse, ReaderExt};

use crate::error::{Error, Result};
use crate::luabin::{LocalVar, LuaBin, LuaConstant, Proto};

const SIGNATURE: [u8; 4] = *b"\x1bLua";
const LUA_51: u8 = 0x51;
const CAP_LIMIT: usize = 1 << 16;

struct Layout {
    int_size: u8,
    size_t_size: u8,
    number_size: u8,
}

impl Parse for LuaBin {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        if reader.read_byte_array::<4>()? != SIGNATURE {
            return Err(Error::InvalidSignature);
        }
        let version = reader.read_u8()?;
        if version != LUA_51 {
            return Err(Error::UnsupportedVersion(version));
        }
        let format = reader.read_u8()?;
        let endian = reader.read_u8()?;
        if endian != 1 {
            return Err(Error::Unsupported("big-endian bytecode"));
        }
        let int_size = reader.read_u8()?;
        let size_t_size = reader.read_u8()?;
        let instruction_size = reader.read_u8()?;
        let number_size = reader.read_u8()?;
        let is_integral = reader.read_u8()?;

        if instruction_size != 4 {
            return Err(Error::Unsupported("instruction size != 4"));
        }
        if !matches!(int_size, 4 | 8) {
            return Err(Error::Unsupported("int size not 4 or 8"));
        }
        if !matches!(size_t_size, 4 | 8) {
            return Err(Error::Unsupported("size_t size not 4 or 8"));
        }

        let layout = Layout {
            int_size,
            size_t_size,
            number_size,
        };
        let main = read_proto(reader, &layout)?;

        let mut rest = Vec::new();
        reader.read_to_end(&mut rest)?;
        if !rest.is_empty() {
            return Err(Error::TrailingBytes(rest.len()));
        }

        Ok(LuaBin {
            version,
            format,
            endian,
            int_size,
            size_t_size,
            instruction_size,
            number_size,
            is_integral,
            main,
        })
    }
}

fn read_proto<R: Read>(r: &mut R, layout: &Layout) -> Result<Proto> {
    let source = read_string(r, layout.size_t_size)?;
    let line_defined = read_int(r, layout.int_size)?;
    let last_line_defined = read_int(r, layout.int_size)?;
    let num_upvalues = r.read_u8()?;
    let num_params = r.read_u8()?;
    let is_vararg = r.read_u8()?;
    let max_stack = r.read_u8()?;

    let code_count = read_count(r, layout.int_size)?;
    let mut code = Vec::with_capacity(code_count.min(CAP_LIMIT));
    for _ in 0..code_count {
        code.push(r.read_u32()?);
    }

    let const_count = read_count(r, layout.int_size)?;
    let mut constants = Vec::with_capacity(const_count.min(CAP_LIMIT));
    for _ in 0..const_count {
        constants.push(read_constant(r, layout)?);
    }

    let proto_count = read_count(r, layout.int_size)?;
    let mut protos = Vec::with_capacity(proto_count.min(CAP_LIMIT));
    for _ in 0..proto_count {
        protos.push(read_proto(r, layout)?);
    }

    let line_count = read_count(r, layout.int_size)?;
    let mut line_info = Vec::with_capacity(line_count.min(CAP_LIMIT));
    for _ in 0..line_count {
        line_info.push(read_int(r, layout.int_size)?);
    }

    let local_count = read_count(r, layout.int_size)?;
    let mut locals = Vec::with_capacity(local_count.min(CAP_LIMIT));
    for _ in 0..local_count {
        let name = read_string(r, layout.size_t_size)?;
        let start_pc = read_int(r, layout.int_size)?;
        let end_pc = read_int(r, layout.int_size)?;
        locals.push(LocalVar {
            name,
            start_pc,
            end_pc,
        });
    }

    let upvalue_count = read_count(r, layout.int_size)?;
    let mut upvalue_names = Vec::with_capacity(upvalue_count.min(CAP_LIMIT));
    for _ in 0..upvalue_count {
        upvalue_names.push(read_string(r, layout.size_t_size)?);
    }

    Ok(Proto {
        source,
        line_defined,
        last_line_defined,
        num_upvalues,
        num_params,
        is_vararg,
        max_stack,
        code,
        constants,
        protos,
        line_info,
        locals,
        upvalue_names,
    })
}

fn read_constant<R: Read>(r: &mut R, layout: &Layout) -> Result<LuaConstant> {
    match r.read_u8()? {
        0 => Ok(LuaConstant::Nil),
        1 => Ok(LuaConstant::Bool(r.read_u8()?)),
        3 => Ok(LuaConstant::Number(
            r.read_bytes(layout.number_size as usize)?,
        )),
        4 => Ok(LuaConstant::Str(read_string(r, layout.size_t_size)?)),
        other => Err(Error::UnknownConstant(other)),
    }
}

fn read_string<R: Read>(r: &mut R, size_t_size: u8) -> Result<Option<Vec<u8>>> {
    let len = read_size_t(r, size_t_size)? as usize;
    if len == 0 {
        Ok(None)
    } else {
        Ok(Some(r.read_bytes(len)?))
    }
}

fn read_int<R: Read>(r: &mut R, size: u8) -> Result<i64> {
    match size {
        4 => Ok(r.read_i32()? as i64),
        8 => Ok(r.read_i64()?),
        _ => Err(Error::Unsupported("int size not 4 or 8")),
    }
}

fn read_size_t<R: Read>(r: &mut R, size: u8) -> Result<u64> {
    match size {
        4 => Ok(r.read_u32()? as u64),
        8 => Ok(r.read_u64()?),
        _ => Err(Error::Unsupported("size_t size not 4 or 8")),
    }
}

fn read_count<R: Read>(r: &mut R, int_size: u8) -> Result<usize> {
    usize::try_from(read_int(r, int_size)?).map_err(|_| Error::Malformed("negative count"))
}
