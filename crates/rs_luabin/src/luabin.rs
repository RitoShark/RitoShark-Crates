/*!
The luabin64 data model: a faithful, fully-addressable view of a Lua 5.1 bytecode chunk. Every
field of the global header and of the recursive function-prototype tree (instructions, constants,
nested prototypes, and the debug tables) is retained verbatim so the chunk re-emits byte-for-byte.
Instruction words and number constants are kept as raw bytes — the editing surface lives on the
constant pool, where [`LuaConstant`] exposes typed get/set for the numbers and strings that hold
the data a modder actually changes; because constants are referenced by index and strings are
inline length-prefixed, editing one constant needs no external fix-ups beyond re-serialization.
*/

/// A parsed Lua 5.1 bytecode chunk: the global header plus the top-level function prototype.
#[derive(Debug, Clone, PartialEq)]
pub struct LuaBin {
    pub version: u8,
    pub format: u8,
    pub endian: u8,
    pub int_size: u8,
    pub size_t_size: u8,
    pub instruction_size: u8,
    pub number_size: u8,
    pub is_integral: u8,
    pub main: Proto,
}

/// A Lua function prototype. `code` holds raw instruction words; the debug tables (`line_info`,
/// `locals`, `upvalue_names`) are retained even though they are not needed to execute, so the chunk
/// round-trips exactly. A string is `None` when stored with length zero and otherwise carries its
/// raw bytes including Lua's trailing NUL.
#[derive(Debug, Clone, PartialEq)]
pub struct Proto {
    pub source: Option<Vec<u8>>,
    pub line_defined: i64,
    pub last_line_defined: i64,
    pub num_upvalues: u8,
    pub num_params: u8,
    pub is_vararg: u8,
    pub max_stack: u8,
    pub code: Vec<u32>,
    pub constants: Vec<LuaConstant>,
    pub protos: Vec<Proto>,
    pub line_info: Vec<i64>,
    pub locals: Vec<LocalVar>,
    pub upvalue_names: Vec<Option<Vec<u8>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalVar {
    pub name: Option<Vec<u8>>,
    pub start_pc: i64,
    pub end_pc: i64,
}

/// A constant-pool entry. Numbers and strings keep their exact on-disk bytes; [`as_f64`](Self::as_f64)
/// /[`set_f64`](Self::set_f64) and [`as_string`](Self::as_string)/[`set_string`](Self::set_string)
/// provide the typed editing surface.
#[derive(Debug, Clone, PartialEq)]
pub enum LuaConstant {
    Nil,
    Bool(u8),
    /// Raw number bytes, `number_size` wide (8 for the standard f64 layout).
    Number(Vec<u8>),
    /// `None` for a length-zero string, else the raw bytes including the trailing NUL.
    Str(Option<Vec<u8>>),
}

impl LuaConstant {
    /// Decodes a number constant as `f64` (handling the 4- and 8-byte float layouts). Returns
    /// `None` for non-number constants or unexpected widths.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            LuaConstant::Number(bytes) => match bytes.len() {
                8 => Some(f64::from_le_bytes(bytes[..8].try_into().ok()?)),
                4 => Some(f32::from_le_bytes(bytes[..4].try_into().ok()?) as f64),
                _ => None,
            },
            _ => None,
        }
    }

    /// Overwrites a number constant in place, preserving its on-disk width. Returns `false` if this
    /// is not a number or its width is unsupported.
    pub fn set_f64(&mut self, value: f64) -> bool {
        if let LuaConstant::Number(bytes) = self {
            match bytes.len() {
                8 => {
                    *bytes = value.to_le_bytes().to_vec();
                    true
                }
                4 => {
                    *bytes = (value as f32).to_le_bytes().to_vec();
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }

    /// The string constant's bytes without Lua's trailing NUL, or `None` for non-strings / the
    /// null string.
    pub fn as_string(&self) -> Option<&[u8]> {
        match self {
            LuaConstant::Str(Some(bytes)) => Some(bytes.strip_suffix(&[0]).unwrap_or(bytes)),
            _ => None,
        }
    }

    /// Replaces this constant with a string carrying `value` plus the trailing NUL Lua expects.
    pub fn set_string(&mut self, value: &str) {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        *self = LuaConstant::Str(Some(bytes));
    }
}
