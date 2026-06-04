/*!
The `.troybin` data model. The format keys every value by a 32-bit `ihash` of its `section/name`
pair; the binary only stores hashes, so raw hashes are the source of truth and any human-readable
name is a display concern resolved separately. Version 2 groups values into up to fourteen typed
buckets selected by a flags word, each bucket holding a parallel `(hash, value)` column. Version 1
is a flat `(hash, offset)` table into a string blob. Both representations keep their on-disk order
and raw values so a rebuild is byte-exact.
*/

use crate::error::{Error, Result};

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

/// One decoded property value, flattened out of its typed bucket so a caller can read or edit a
/// single property without tracking which `flag_bit` bucket it lives in or how strings are blobbed.
/// String values carry their decoded bytes (no trailing NUL); the managed setters recompute the
/// blob and offsets on write.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    I32(i32),
    F32(f32),
    U8(u8),
    I16(i16),
    U16(u16),
    Bool(bool),
    U8x3([u8; 3]),
    F32x3([f32; 3]),
    U8x2([u8; 2]),
    F32x2([f32; 2]),
    U8x4([u8; 4]),
    F32x4([f32; 4]),
    String(Vec<u8>),
}

impl ScalarValue {
    /// The flag bit a freshly inserted value of this type is written under, choosing the canonical
    /// bit where two bits share a layout (bit 0 for `i32`, bit 2 for `u8`). `U16` has no on-disk
    /// bucket bit and therefore returns `None`.
    fn canonical_bit(&self) -> Option<u8> {
        Some(match self {
            ScalarValue::I32(_) => 0,
            ScalarValue::F32(_) => 1,
            ScalarValue::U8(_) => 2,
            ScalarValue::I16(_) => 3,
            ScalarValue::Bool(_) => 5,
            ScalarValue::U8x3(_) => 6,
            ScalarValue::F32x3(_) => 7,
            ScalarValue::U8x2(_) => 8,
            ScalarValue::F32x2(_) => 9,
            ScalarValue::U8x4(_) => 10,
            ScalarValue::F32x4(_) => 11,
            ScalarValue::String(_) => 12,
            ScalarValue::U16(_) => return None,
        })
    }
}

impl Bucket {
    /// One [`ScalarValue`] per hash, decoding packed booleans and blob-offset strings into standalone
    /// values. Length always equals `hashes.len()`, restoring the parallel-column invariant as a
    /// safe typed view.
    pub fn decoded(&self) -> Vec<ScalarValue> {
        let count = self.hashes.len();
        match &self.values {
            BucketValues::I32(v) => v.iter().map(|&x| ScalarValue::I32(x)).collect(),
            BucketValues::F32(v) => v.iter().map(|&x| ScalarValue::F32(x)).collect(),
            BucketValues::U8(v) => v.iter().map(|&x| ScalarValue::U8(x)).collect(),
            BucketValues::I16(v) => v.iter().map(|&x| ScalarValue::I16(x)).collect(),
            BucketValues::U16(v) => v.iter().map(|&x| ScalarValue::U16(x)).collect(),
            BucketValues::Bool(bytes) => (0..count)
                .map(|i| {
                    ScalarValue::Bool(bytes.get(i / 8).is_some_and(|b| b & (1 << (i % 8)) != 0))
                })
                .collect(),
            BucketValues::U8x3(v) => v.iter().map(|&a| ScalarValue::U8x3(a)).collect(),
            BucketValues::F32x3(v) => v.iter().map(|&a| ScalarValue::F32x3(a)).collect(),
            BucketValues::U8x2(v) => v.iter().map(|&a| ScalarValue::U8x2(a)).collect(),
            BucketValues::F32x2(v) => v.iter().map(|&a| ScalarValue::F32x2(a)).collect(),
            BucketValues::U8x4(v) => v.iter().map(|&a| ScalarValue::U8x4(a)).collect(),
            BucketValues::F32x4(v) => v.iter().map(|&a| ScalarValue::F32x4(a)).collect(),
            BucketValues::Strings { offsets, blob } => offsets
                .iter()
                .map(|&o| ScalarValue::String(read_blob_string(blob, o as usize)))
                .collect(),
        }
    }

    /// `(hash, value)` pairs for this bucket, the parallel columns zipped back together.
    pub fn entries(&self) -> Vec<(u32, ScalarValue)> {
        self.hashes.iter().copied().zip(self.decoded()).collect()
    }

    /// Rebuilds a bucket for `flag_bit` from `(hash, value)` pairs, re-packing booleans and rebuilding
    /// the strings blob/offsets. Every value must match the bit's layout, else
    /// [`Error::ValueTypeMismatch`](crate::Error::ValueTypeMismatch).
    fn rebuilt(flag_bit: u8, entries: Vec<(u32, ScalarValue)>) -> Result<Bucket> {
        let (hashes, values): (Vec<u32>, Vec<ScalarValue>) = entries.into_iter().unzip();
        let values = encode_bucket(flag_bit, &values)?;
        Ok(Bucket {
            flag_bit,
            hashes,
            values,
        })
    }
}

impl TroybinV2 {
    /// The decoded value keyed by `hash`, scanning the buckets in order. `None` if no bucket holds it.
    pub fn get(&self, hash: u32) -> Option<ScalarValue> {
        for bucket in &self.buckets {
            if let Some(i) = bucket.hashes.iter().position(|&h| h == hash) {
                return Some(bucket.decoded().swap_remove(i));
            }
        }
        None
    }

    /// Every `(hash, value)` pair across all buckets, in bucket-then-insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, ScalarValue)> + '_ {
        self.buckets.iter().flat_map(|b| b.entries())
    }

    /// Overwrites the value of an existing `hash` in place, keeping it in its current bucket and
    /// recomputing packed booleans / the strings blob as needed. Errors with
    /// [`Error::ValueTypeMismatch`](crate::Error::ValueTypeMismatch) if the new value's type does not
    /// match that bucket, and returns `Ok(false)` if the hash is not present.
    pub fn set(&mut self, hash: u32, value: ScalarValue) -> Result<bool> {
        for bucket in &mut self.buckets {
            if let Some(i) = bucket.hashes.iter().position(|&h| h == hash) {
                let mut entries = bucket.entries();
                entries[i].1 = value;
                *bucket = Bucket::rebuilt(bucket.flag_bit, entries)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Inserts a new property, appending it to the bucket for its type (creating that bucket, in
    /// ascending flag-bit order, if absent). If `hash` already exists its value is overwritten in its
    /// existing bucket instead. Returns the bit the value landed under.
    pub fn insert(&mut self, hash: u32, value: ScalarValue) -> Result<u8> {
        if self.set(hash, value.clone())? {
            return Ok(self
                .buckets
                .iter()
                .find(|b| b.hashes.contains(&hash))
                .map(|b| b.flag_bit)
                .unwrap_or_default());
        }
        let bit = value.canonical_bit().ok_or(Error::NoBucketForType)?;
        match self.buckets.iter_mut().find(|b| b.flag_bit == bit) {
            Some(bucket) => {
                let mut entries = bucket.entries();
                entries.push((hash, value));
                *bucket = Bucket::rebuilt(bit, entries)?;
            }
            None => {
                let bucket = Bucket::rebuilt(bit, vec![(hash, value)])?;
                let pos = self
                    .buckets
                    .iter()
                    .position(|b| b.flag_bit > bit)
                    .unwrap_or(self.buckets.len());
                self.buckets.insert(pos, bucket);
            }
        }
        Ok(bit)
    }

    /// Removes a property by `hash`, returning its decoded value, and drops the bucket if it empties.
    pub fn remove(&mut self, hash: u32) -> Result<Option<ScalarValue>> {
        for bi in 0..self.buckets.len() {
            let Some(i) = self.buckets[bi].hashes.iter().position(|&h| h == hash) else {
                continue;
            };
            let mut entries = self.buckets[bi].entries();
            let (_, removed) = entries.remove(i);
            if entries.is_empty() {
                self.buckets.remove(bi);
            } else {
                let bit = self.buckets[bi].flag_bit;
                self.buckets[bi] = Bucket::rebuilt(bit, entries)?;
            }
            return Ok(Some(removed));
        }
        Ok(None)
    }
}

impl Troybin {
    /// The 65599 `ihash` of `section/name` exactly as the binary keys it: `ihash("*", ihash(section))`
    /// seeds the property hash. Resolving a known name to its stored hash uses this.
    pub fn property_hash(section: &str, name: &str) -> u32 {
        let section_hash = rs_hash::ihash_seeded(rs_hash::ihash(section), "*");
        rs_hash::ihash_seeded(section_hash, name)
    }

    /// Reads a v2 property by its `section`/`name`, hashing the pair the way the binary keys it.
    /// Always `None` for a v1 body, whose flat `(hash, offset)` table carries no value typing.
    pub fn get(&self, section: &str, name: &str) -> Option<ScalarValue> {
        match &self.body {
            TroybinBody::V2(body) => body.get(Self::property_hash(section, name)),
            TroybinBody::V1(_) => None,
        }
    }

    /// Writes a v2 property by its `section`/`name` (overwriting if present, inserting otherwise).
    /// Errors on a v1 body, which this crate treats as read-only.
    pub fn set(&mut self, section: &str, name: &str, value: ScalarValue) -> Result<()> {
        match &mut self.body {
            TroybinBody::V2(body) => {
                body.insert(Self::property_hash(section, name), value)?;
                Ok(())
            }
            TroybinBody::V1(_) => Err(Error::UnsupportedVersion(1)),
        }
    }
}

fn read_blob_string(blob: &[u8], offset: usize) -> Vec<u8> {
    let start = offset.min(blob.len());
    let end = blob[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|p| start + p)
        .unwrap_or(blob.len());
    blob[start..end].to_vec()
}

fn encode_bucket(flag_bit: u8, values: &[ScalarValue]) -> Result<BucketValues> {
    let mismatch = || Error::ValueTypeMismatch(flag_bit);
    Ok(match flag_bit {
        0 | 13 => BucketValues::I32(map_values(flag_bit, values, |v| match v {
            ScalarValue::I32(x) => Some(*x),
            _ => None,
        })?),
        1 => BucketValues::F32(map_values(flag_bit, values, |v| match v {
            ScalarValue::F32(x) => Some(*x),
            _ => None,
        })?),
        2 | 4 => BucketValues::U8(map_values(flag_bit, values, |v| match v {
            ScalarValue::U8(x) => Some(*x),
            _ => None,
        })?),
        3 => BucketValues::I16(map_values(flag_bit, values, |v| match v {
            ScalarValue::I16(x) => Some(*x),
            _ => None,
        })?),
        5 => {
            let mut bytes = vec![0u8; values.len().div_ceil(8)];
            for (i, v) in values.iter().enumerate() {
                let ScalarValue::Bool(b) = v else {
                    return Err(mismatch());
                };
                if *b {
                    bytes[i / 8] |= 1 << (i % 8);
                }
            }
            BucketValues::Bool(bytes)
        }
        6 => BucketValues::U8x3(map_values(flag_bit, values, |v| match v {
            ScalarValue::U8x3(a) => Some(*a),
            _ => None,
        })?),
        7 => BucketValues::F32x3(map_values(flag_bit, values, |v| match v {
            ScalarValue::F32x3(a) => Some(*a),
            _ => None,
        })?),
        8 => BucketValues::U8x2(map_values(flag_bit, values, |v| match v {
            ScalarValue::U8x2(a) => Some(*a),
            _ => None,
        })?),
        9 => BucketValues::F32x2(map_values(flag_bit, values, |v| match v {
            ScalarValue::F32x2(a) => Some(*a),
            _ => None,
        })?),
        10 => BucketValues::U8x4(map_values(flag_bit, values, |v| match v {
            ScalarValue::U8x4(a) => Some(*a),
            _ => None,
        })?),
        11 => BucketValues::F32x4(map_values(flag_bit, values, |v| match v {
            ScalarValue::F32x4(a) => Some(*a),
            _ => None,
        })?),
        12 => {
            let mut offsets = Vec::with_capacity(values.len());
            let mut blob = Vec::new();
            for v in values {
                let ScalarValue::String(bytes) = v else {
                    return Err(mismatch());
                };
                if blob.len() > u16::MAX as usize {
                    return Err(Error::StringsTooLarge);
                }
                offsets.push(blob.len() as u16);
                blob.extend_from_slice(bytes);
                blob.push(0);
            }
            if blob.len() > u16::MAX as usize {
                return Err(Error::StringsTooLarge);
            }
            BucketValues::Strings { offsets, blob }
        }
        other => return Err(Error::UnsupportedBucket(other)),
    })
}

fn map_values<T>(
    flag_bit: u8,
    values: &[ScalarValue],
    pick: impl Fn(&ScalarValue) -> Option<T>,
) -> Result<Vec<T>> {
    values
        .iter()
        .map(|v| pick(v).ok_or(Error::ValueTypeMismatch(flag_bit)))
        .collect()
}
