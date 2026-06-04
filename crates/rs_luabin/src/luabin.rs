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
        self.set_string_bytes(value.as_bytes());
    }

    /// Replaces this constant with a string carrying the raw `value` bytes plus the trailing NUL,
    /// so non-UTF-8 strings (which [`as_string`](Self::as_string) can read) can be written back.
    pub fn set_string_bytes(&mut self, value: &[u8]) {
        let mut bytes = value.to_vec();
        bytes.push(0);
        *self = LuaConstant::Str(Some(bytes));
    }

    /// The boolean value of a `Bool` constant (`false` only for a stored `0`), or `None` for other
    /// constants. The raw byte is kept in the model so an unusual encoding still round-trips.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            LuaConstant::Bool(b) => Some(*b != 0),
            _ => None,
        }
    }

    /// Overwrites a `Bool` constant in place. Returns `false` if this is not a boolean.
    pub fn set_bool(&mut self, value: bool) -> bool {
        if let LuaConstant::Bool(b) = self {
            *b = value as u8;
            true
        } else {
            false
        }
    }
}

/// A stable address of a constant inside the prototype tree: the chain of child-prototype indices
/// from `main` (empty for `main` itself) followed by the constant's index in that prototype's pool.
/// Yielded by [`LuaBin::iter_constants`] and accepted by [`LuaBin::constant`] /
/// [`LuaBin::constant_mut`] / [`LuaBin::set_number`] so an edit lands on exactly one constant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstPath {
    pub proto: Vec<usize>,
    pub index: usize,
}

impl ConstPath {
    pub fn new(proto: Vec<usize>, index: usize) -> Self {
        ConstPath { proto, index }
    }
}

impl LuaBin {
    /// Walks the whole prototype tree depth-first and returns every constant paired with its
    /// [`ConstPath`], so an editor sees the constants nested in inner functions — where most
    /// moddable literals live — not just those in `main`.
    pub fn iter_constants(&self) -> impl Iterator<Item = (ConstPath, &LuaConstant)> {
        let mut out = Vec::new();
        collect_constants(&self.main, &mut Vec::new(), &mut out);
        out.into_iter()
    }

    /// Resolves the prototype named by a chain of child indices (empty selects `main`).
    pub fn proto_at(&self, proto: &[usize]) -> Option<&Proto> {
        let mut node = &self.main;
        for &i in proto {
            node = node.protos.get(i)?;
        }
        Some(node)
    }

    /// Mutable counterpart of [`proto_at`](Self::proto_at).
    pub fn proto_at_mut(&mut self, proto: &[usize]) -> Option<&mut Proto> {
        let mut node = &mut self.main;
        for &i in proto {
            node = node.protos.get_mut(i)?;
        }
        Some(node)
    }

    /// The constant at `path`, or `None` if the path does not address one.
    pub fn constant(&self, path: &ConstPath) -> Option<&LuaConstant> {
        self.proto_at(&path.proto)?.constants.get(path.index)
    }

    /// Mutable counterpart of [`constant`](Self::constant).
    pub fn constant_mut(&mut self, path: &ConstPath) -> Option<&mut LuaConstant> {
        self.proto_at_mut(&path.proto)?
            .constants
            .get_mut(path.index)
    }

    /// Reads the number constant at `path` as `f64`, honouring this chunk's `is_integral` flag and
    /// `number_size`. Returns `None` if the path does not address a number of a supported width.
    pub fn number(&self, path: &ConstPath) -> Option<f64> {
        let LuaConstant::Number(bytes) = self.constant(path)? else {
            return None;
        };
        decode_number(bytes, self.is_integral != 0)
    }

    /// Overwrites the number constant at `path`, re-encoding `value` in this chunk's on-disk number
    /// layout (integral vs. float, `number_size` wide) so the width — and therefore the file
    /// length — is preserved. Unlike [`LuaConstant::set_f64`] this never silently no-ops: it errors
    /// when the path is not a number or its width is unsupported.
    pub fn set_number(&mut self, path: &ConstPath, value: f64) -> crate::Result<()> {
        let integral = self.is_integral != 0;
        let Some(LuaConstant::Number(bytes)) = self.constant_mut(path) else {
            return Err(crate::Error::Malformed(
                "path does not address a number constant",
            ));
        };
        match encode_number(bytes.len(), value, integral) {
            Some(encoded) => {
                *bytes = encoded;
                Ok(())
            }
            None => Err(crate::Error::Unsupported("unsupported number width")),
        }
    }

    /// Reconstructs the `GlobalName = value` assignments by pairing each `SETGLOBAL` with the
    /// `LOADK` that fed its source register, across every prototype. Lets an editor present a named
    /// value (`SpellDamage = 50`) and edit it through the returned [`ConstPath`].
    pub fn global_assignments(&self) -> Vec<GlobalAssignment> {
        crate::globals::global_assignments(self)
    }
}

/// A reconstructed `GlobalName = value` statement: the global's name and the [`ConstPath`] of the
/// constant assigned to it, so an editor can show the pairing and edit the value in place.
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalAssignment {
    pub name: String,
    pub value: ConstPath,
}

fn collect_constants<'a>(
    proto: &'a Proto,
    stack: &mut Vec<usize>,
    out: &mut Vec<(ConstPath, &'a LuaConstant)>,
) {
    for (index, constant) in proto.constants.iter().enumerate() {
        out.push((
            ConstPath {
                proto: stack.clone(),
                index,
            },
            constant,
        ));
    }
    for (i, child) in proto.protos.iter().enumerate() {
        stack.push(i);
        collect_constants(child, stack, out);
        stack.pop();
    }
}

fn decode_number(bytes: &[u8], integral: bool) -> Option<f64> {
    match (integral, bytes.len()) {
        (false, 8) => Some(f64::from_le_bytes(bytes.try_into().ok()?)),
        (false, 4) => Some(f32::from_le_bytes(bytes.try_into().ok()?) as f64),
        (true, 8) => Some(i64::from_le_bytes(bytes.try_into().ok()?) as f64),
        (true, 4) => Some(i32::from_le_bytes(bytes.try_into().ok()?) as f64),
        _ => None,
    }
}

fn encode_number(width: usize, value: f64, integral: bool) -> Option<Vec<u8>> {
    match (integral, width) {
        (false, 8) => Some(value.to_le_bytes().to_vec()),
        (false, 4) => Some((value as f32).to_le_bytes().to_vec()),
        (true, 8) => Some((value as i64).to_le_bytes().to_vec()),
        (true, 4) => Some((value as i32).to_le_bytes().to_vec()),
        _ => None,
    }
}
