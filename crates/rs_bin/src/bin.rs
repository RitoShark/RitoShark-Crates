use indexmap::IndexMap;

use crate::error::{Error, Result};

/// The on-disk value type tag. Primitive tags are `0..=18`; container tags have the high bit set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BinType {
    None = 0,
    Bool = 1,
    I8 = 2,
    U8 = 3,
    I16 = 4,
    U16 = 5,
    I32 = 6,
    U32 = 7,
    I64 = 8,
    U64 = 9,
    F32 = 10,
    Vec2 = 11,
    Vec3 = 12,
    Vec4 = 13,
    Mtx44 = 14,
    Rgba = 15,
    String = 16,
    Hash = 17,
    File = 18,
    List = 0x80,
    List2 = 0x81,
    Pointer = 0x82,
    Embed = 0x83,
    Link = 0x84,
    Option = 0x85,
    Map = 0x86,
    Flag = 0x87,
}

impl BinType {
    pub fn from_u8(v: u8) -> Result<Self> {
        Ok(match v {
            0 => Self::None,
            1 => Self::Bool,
            2 => Self::I8,
            3 => Self::U8,
            4 => Self::I16,
            5 => Self::U16,
            6 => Self::I32,
            7 => Self::U32,
            8 => Self::I64,
            9 => Self::U64,
            10 => Self::F32,
            11 => Self::Vec2,
            12 => Self::Vec3,
            13 => Self::Vec4,
            14 => Self::Mtx44,
            15 => Self::Rgba,
            16 => Self::String,
            17 => Self::Hash,
            18 => Self::File,
            0x80 => Self::List,
            0x81 => Self::List2,
            0x82 => Self::Pointer,
            0x83 => Self::Embed,
            0x84 => Self::Link,
            0x85 => Self::Option,
            0x86 => Self::Map,
            0x87 => Self::Flag,
            other => return Err(Error::InvalidType(other)),
        })
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// True only for the nesting container tags that may not appear as a container element type:
    /// list, list2, map, and option. Pointer, embed, link, and flag are complex but legal elements.
    pub fn is_container(self) -> bool {
        matches!(self, Self::List | Self::List2 | Self::Map | Self::Option)
    }

    /// True for the primitive tags (high bit clear, `0..=18`); these are the only legal map keys.
    pub fn is_primitive(self) -> bool {
        self.to_u8() & 0x80 == 0
    }
}

/// An owned `.bin` value. The tree is fully owned so it can be edited and cloned freely; the
/// integer hash fields are the source of truth for writing, and names are resolved only at print
/// time. `List`/`List2` share a layout and are distinguished by `is_list2`; `Pointer`/`Embed`
/// share a struct body and are kept as separate variants.
#[derive(Debug, Clone, PartialEq)]
pub enum BinValue {
    None,
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Mtx44([f32; 16]),
    Rgba([u8; 4]),
    String(String),
    Hash(u32),
    File(u64),
    Link(u32),
    List {
        is_list2: bool,
        item: BinType,
        items: Vec<BinValue>,
    },
    Map {
        key: BinType,
        value: BinType,
        entries: Vec<(BinValue, BinValue)>,
    },
    Pointer {
        class: u32,
        fields: IndexMap<u32, BinValue>,
    },
    Embed {
        class: u32,
        fields: IndexMap<u32, BinValue>,
    },
    Option {
        item: BinType,
        value: Option<Box<BinValue>>,
    },
    Flag(bool),
}

impl BinValue {
    /// The on-disk type tag this value serializes as.
    pub fn ty(&self) -> BinType {
        match self {
            BinValue::None => BinType::None,
            BinValue::Bool(_) => BinType::Bool,
            BinValue::I8(_) => BinType::I8,
            BinValue::U8(_) => BinType::U8,
            BinValue::I16(_) => BinType::I16,
            BinValue::U16(_) => BinType::U16,
            BinValue::I32(_) => BinType::I32,
            BinValue::U32(_) => BinType::U32,
            BinValue::I64(_) => BinType::I64,
            BinValue::U64(_) => BinType::U64,
            BinValue::F32(_) => BinType::F32,
            BinValue::Vec2(_) => BinType::Vec2,
            BinValue::Vec3(_) => BinType::Vec3,
            BinValue::Vec4(_) => BinType::Vec4,
            BinValue::Mtx44(_) => BinType::Mtx44,
            BinValue::Rgba(_) => BinType::Rgba,
            BinValue::String(_) => BinType::String,
            BinValue::Hash(_) => BinType::Hash,
            BinValue::File(_) => BinType::File,
            BinValue::Link(_) => BinType::Link,
            BinValue::List { is_list2, .. } => {
                if *is_list2 {
                    BinType::List2
                } else {
                    BinType::List
                }
            }
            BinValue::Map { .. } => BinType::Map,
            BinValue::Pointer { .. } => BinType::Pointer,
            BinValue::Embed { .. } => BinType::Embed,
            BinValue::Option { .. } => BinType::Option,
            BinValue::Flag(_) => BinType::Flag,
        }
    }
}

/// One top-level entry: a struct identified by its path hash and class hash.
#[derive(Debug, Clone, PartialEq)]
pub struct BinEntry {
    pub path_hash: u32,
    pub class_hash: u32,
    pub fields: IndexMap<u32, BinValue>,
}

/// A parsed `.bin`/PROP document.
///
/// `is_patch` selects the `PTCH` magic and its extra header; `patch_header` holds the 8 raw header
/// bytes that follow a `PTCH` magic so they round-trip verbatim. `version` is the format version;
/// `linked` is the ordered list of linked-file paths (version >= 2); `entries` preserves entry
/// order, each carrying its class hash and ordered fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Bin {
    pub is_patch: bool,
    pub patch_header: [u8; 8],
    pub version: u32,
    pub linked: Vec<String>,
    pub entries: Vec<BinEntry>,
}

impl Bin {
    pub fn new() -> Self {
        Self {
            is_patch: false,
            patch_header: [0u8; 8],
            version: 3,
            linked: Vec::new(),
            entries: Vec::new(),
        }
    }
}

impl Default for Bin {
    fn default() -> Self {
        Self::new()
    }
}
