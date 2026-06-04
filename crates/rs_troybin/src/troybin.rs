/*!
The `.troybin` data model. The format keys every value by a 32-bit `ihash` of its `section/name`
pair; the binary only stores hashes, so raw hashes are the source of truth and any human-readable
name is a display concern resolved separately. Version 2 groups values into up to fourteen typed
buckets selected by a flags word, each bucket holding a parallel `(hash, value)` column. Version 1
is a flat `(hash, offset)` table into a string blob. Both representations keep their on-disk order
and raw values so a rebuild is byte-exact.
*/

/// A parsed troybin file: the version byte plus the version-specific body.
#[derive(Debug, Clone, PartialEq)]
pub struct Troybin {
    pub version: u8,
    pub body: TroybinBody,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TroybinBody {
    V1(TroybinV1),
    V2(TroybinV2),
}

/// Legacy version-1 body: three header bytes, a `(hash, offset)` table, and the string blob the
/// offsets point into. Kept verbatim so the file round-trips byte-for-byte.
#[derive(Debug, Clone, PartialEq)]
pub struct TroybinV1 {
    pub header: [u8; 3],
    pub entries: Vec<V1Entry>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V1Entry {
    pub hash: u32,
    pub offset: u32,
}

/// Version-2 body: the string blob length, the flags word (optionally preceded by a zero `u16` in
/// some files), and the typed buckets in ascending flag-bit order.
#[derive(Debug, Clone, PartialEq)]
pub struct TroybinV2 {
    pub strings_length: u16,
    pub flags_zero_prefix: bool,
    pub buckets: Vec<Bucket>,
}

/// One typed value bucket: its flag bit, the parallel hash column, and the typed values. The flag
/// bit is retained because distinct bits can share a byte layout (e.g. bits 0 and 13 are both
/// `i32`), and the write must re-emit each under its original bit.
#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    pub flag_bit: u8,
    pub hashes: Vec<u32>,
    pub values: BucketValues,
}

/// The typed value column of a bucket, mirroring the on-disk layout for each flag bit. Values are
/// raw — no display multiplier is applied — so they re-encode byte-for-byte.
#[derive(Debug, Clone, PartialEq)]
pub enum BucketValues {
    I32(Vec<i32>),
    F32(Vec<f32>),
    U8(Vec<u8>),
    I16(Vec<i16>),
    U16(Vec<u16>),
    /// Packed bit flags, one bit per hash, stored as the raw `ceil(count / 8)` bytes.
    Bool(Vec<u8>),
    U8x3(Vec<[u8; 3]>),
    F32x3(Vec<[f32; 3]>),
    U8x2(Vec<[u8; 2]>),
    F32x2(Vec<[f32; 2]>),
    U8x4(Vec<[u8; 4]>),
    F32x4(Vec<[f32; 4]>),
    /// One `u16` blob offset per hash, plus the shared blob the offsets index into.
    Strings {
        offsets: Vec<u16>,
        blob: Vec<u8>,
    },
}

impl Troybin {
    /// The 65599 `ihash` of `section/name` exactly as the binary keys it: `ihash("*", ihash(section))`
    /// seeds the property hash. Resolving a known name to its stored hash uses this.
    pub fn property_hash(section: &str, name: &str) -> u32 {
        let section_hash = rs_hash::ihash_seeded(rs_hash::ihash(section), "*");
        rs_hash::ihash_seeded(section_hash, name)
    }
}
