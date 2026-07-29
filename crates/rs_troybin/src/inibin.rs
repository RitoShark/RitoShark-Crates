/*!
The inibin view of the format. `.inibin`, `.cfgbin` and `.troybin` are one and the same binary
layout, so this module is a naming layer over the model in [`troybin`](crate::troybin) rather than a
second parser: [`Inibin`] is [`Troybin`] and reading/writing goes through the same byte-exact code
path. What it adds is the inibin vocabulary — [`InibinFlags`] names the fourteen v2 bucket bits (and
the version-1 body as [`InibinFlags::OldFormat`]) so a caller can enumerate buckets by type instead
of by raw bit.

Bits 2, 6, 8 and 10 hold fixed-point values: the bytes on disk are plain `u8`, and the conventional
display form is `raw * 0.1`. This crate keeps the raw byte as the stored value so files re-encode
byte-for-byte; [`InibinFlags::is_fixed_point`], [`fixed_point_to_f64`] and [`fixed_point_from_f64`]
are a derived view over it and never replace the stored form.
*/

use crate::error::{Error, Result};
use crate::troybin::{Bucket, ScalarValue, Troybin, TroybinBody, TroybinV2};

/// An inibin file. The inibin and troybin formats are identical, so this is [`Troybin`] under its
/// other name and carries the same [`Parse`](rs_io::Parse)/[`Serialize`](rs_io::Serialize) impls.
pub type Inibin = Troybin;

/// The scale a fixed-point bucket byte is displayed at: the stored `u8` reads as `raw * 0.1`.
pub const FIXED_POINT_SCALE: f64 = 0.1;

/// The value type of one bucket, named after the storage kind it holds. The discriminant is the v2
/// flag bit, except [`OldFormat`](InibinFlags::OldFormat), which is not a bit at all — it stands for
/// the version-1 body, whose flat `(hash, offset)` table carries no value typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum InibinFlags {
    Int32List = 0,
    Float32List = 1,
    FixedPointFloatList = 2,
    Int16List = 3,
    Int8List = 4,
    BitList = 5,
    FixedPointFloatListVec3 = 6,
    Float32ListVec3 = 7,
    FixedPointFloatListVec2 = 8,
    Float32ListVec2 = 9,
    FixedPointFloatListVec4 = 10,
    Float32ListVec4 = 11,
    StringList = 12,
    Int32LongList = 13,
    /// The version-1 body, which stores every value as a string in one blob.
    OldFormat = 255,
}

impl InibinFlags {
    /// Every variant, in ascending bit order with `OldFormat` last.
    pub const ALL: [InibinFlags; 15] = [
        InibinFlags::Int32List,
        InibinFlags::Float32List,
        InibinFlags::FixedPointFloatList,
        InibinFlags::Int16List,
        InibinFlags::Int8List,
        InibinFlags::BitList,
        InibinFlags::FixedPointFloatListVec3,
        InibinFlags::Float32ListVec3,
        InibinFlags::FixedPointFloatListVec2,
        InibinFlags::Float32ListVec2,
        InibinFlags::FixedPointFloatListVec4,
        InibinFlags::Float32ListVec4,
        InibinFlags::StringList,
        InibinFlags::Int32LongList,
        InibinFlags::OldFormat,
    ];

    /// The raw value backing this variant: the v2 flag bit, or `255` for `OldFormat`.
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// The variant for a raw flag bit (or `255`), or
    /// [`Error::UnsupportedBucket`](crate::Error::UnsupportedBucket) for any other value.
    pub fn from_u8(value: u8) -> Result<Self> {
        Ok(match value {
            0 => InibinFlags::Int32List,
            1 => InibinFlags::Float32List,
            2 => InibinFlags::FixedPointFloatList,
            3 => InibinFlags::Int16List,
            4 => InibinFlags::Int8List,
            5 => InibinFlags::BitList,
            6 => InibinFlags::FixedPointFloatListVec3,
            7 => InibinFlags::Float32ListVec3,
            8 => InibinFlags::FixedPointFloatListVec2,
            9 => InibinFlags::Float32ListVec2,
            10 => InibinFlags::FixedPointFloatListVec4,
            11 => InibinFlags::Float32ListVec4,
            12 => InibinFlags::StringList,
            13 => InibinFlags::Int32LongList,
            255 => InibinFlags::OldFormat,
            other => return Err(Error::UnsupportedBucket(other)),
        })
    }

    /// Whether this bucket's `u8` payload is conventionally displayed as `raw * 0.1`.
    pub fn is_fixed_point(self) -> bool {
        matches!(
            self,
            InibinFlags::FixedPointFloatList
                | InibinFlags::FixedPointFloatListVec3
                | InibinFlags::FixedPointFloatListVec2
                | InibinFlags::FixedPointFloatListVec4
        )
    }

    /// The variant name as written in text form.
    pub fn as_str(self) -> &'static str {
        match self {
            InibinFlags::Int32List => "Int32List",
            InibinFlags::Float32List => "Float32List",
            InibinFlags::FixedPointFloatList => "FixedPointFloatList",
            InibinFlags::Int16List => "Int16List",
            InibinFlags::Int8List => "Int8List",
            InibinFlags::BitList => "BitList",
            InibinFlags::FixedPointFloatListVec3 => "FixedPointFloatListVec3",
            InibinFlags::Float32ListVec3 => "Float32ListVec3",
            InibinFlags::FixedPointFloatListVec2 => "FixedPointFloatListVec2",
            InibinFlags::Float32ListVec2 => "Float32ListVec2",
            InibinFlags::FixedPointFloatListVec4 => "FixedPointFloatListVec4",
            InibinFlags::Float32ListVec4 => "Float32ListVec4",
            InibinFlags::StringList => "StringList",
            InibinFlags::Int32LongList => "Int32LongList",
            InibinFlags::OldFormat => "OldFormat",
        }
    }

    /// The variant for a text-form name, or `None` if it names no bucket.
    pub fn from_str_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == name)
    }
}

impl core::fmt::Display for InibinFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<u8> for InibinFlags {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        Self::from_u8(value)
    }
}

impl From<InibinFlags> for u8 {
    fn from(flags: InibinFlags) -> u8 {
        flags.to_u8()
    }
}

/// The display value of a fixed-point byte: `raw * 0.1`.
pub fn fixed_point_to_f64(raw: u8) -> f64 {
    raw as f64 * FIXED_POINT_SCALE
}

/// The byte a fixed-point display value stores as, rounded to the nearest tenth and clamped to the
/// `0..=255` a byte can hold. The conversion is lossy in both directions — the stored byte, not the
/// float, remains the source of truth.
pub fn fixed_point_from_f64(value: f64) -> u8 {
    (value / FIXED_POINT_SCALE).round().clamp(0.0, 255.0) as u8
}

impl Bucket {
    /// This bucket's value type under its inibin name. Errors only if the file carries a flag bit
    /// the format does not define.
    pub fn flags(&self) -> Result<InibinFlags> {
        InibinFlags::from_u8(self.flag_bit)
    }
}

impl TroybinV2 {
    /// The bucket holding values of `flags`, or `None` if the file has none.
    /// [`OldFormat`](InibinFlags::OldFormat) never matches a v2 bucket.
    pub fn bucket(&self, flags: InibinFlags) -> Option<&Bucket> {
        self.buckets.iter().find(|b| b.flag_bit == flags.to_u8())
    }

    /// The mutable bucket holding values of `flags`, or `None` if the file has none.
    pub fn bucket_mut(&mut self, flags: InibinFlags) -> Option<&mut Bucket> {
        self.buckets
            .iter_mut()
            .find(|b| b.flag_bit == flags.to_u8())
    }

    /// The value type of every bucket present, in on-disk order.
    pub fn bucket_flags(&self) -> Result<Vec<InibinFlags>> {
        self.buckets.iter().map(|b| b.flags()).collect()
    }

    /// The value keyed by `hash`, but only if it lives in the `flags` bucket.
    pub fn get_from(&self, flags: InibinFlags, hash: u32) -> Option<ScalarValue> {
        let bucket = self.bucket(flags)?;
        let i = bucket.hashes.iter().position(|&h| h == hash)?;
        Some(bucket.decoded().swap_remove(i))
    }

    /// Whether any bucket carries `hash`.
    pub fn contains(&self, hash: u32) -> bool {
        self.buckets.iter().any(|b| b.hashes.contains(&hash))
    }

    /// Total number of properties across every bucket.
    pub fn len(&self) -> usize {
        self.buckets.iter().map(|b| b.hashes.len()).sum()
    }

    /// Whether the file carries no properties at all.
    pub fn is_empty(&self) -> bool {
        self.buckets.iter().all(|b| b.hashes.is_empty())
    }

    /// Inserts a property into the `flags` bucket specifically, creating that bucket in ascending
    /// bit order if absent, rather than letting the value's type pick the bucket. Use this to place
    /// a value under a bit that shares its layout with another (a `u8` under
    /// [`Int8List`](InibinFlags::Int8List) instead of
    /// [`FixedPointFloatList`](InibinFlags::FixedPointFloatList), an `i32` under
    /// [`Int32LongList`](InibinFlags::Int32LongList) instead of
    /// [`Int32List`](InibinFlags::Int32List)). Any existing copy of `hash` in another bucket is
    /// removed first, so a property never lands in two buckets. Errors with
    /// [`Error::ValueTypeMismatch`](crate::Error::ValueTypeMismatch) if the value does not match the
    /// bucket's layout, and with [`Error::UnsupportedBucket`](crate::Error::UnsupportedBucket) for
    /// [`OldFormat`](InibinFlags::OldFormat), which is not a v2 bucket.
    pub fn insert_into(&mut self, flags: InibinFlags, hash: u32, value: ScalarValue) -> Result<()> {
        if flags == InibinFlags::OldFormat {
            return Err(Error::UnsupportedBucket(flags.to_u8()));
        }
        let bit = flags.to_u8();
        if let Some(existing) = self.buckets.iter().find(|b| b.hashes.contains(&hash))
            && existing.flag_bit != bit
        {
            self.remove(hash)?;
        }
        match self.buckets.iter().position(|b| b.flag_bit == bit) {
            Some(bi) => {
                let mut entries = self.buckets[bi].entries();
                match entries.iter().position(|(h, _)| *h == hash) {
                    Some(i) => entries[i].1 = value,
                    None => entries.push((hash, value)),
                }
                self.buckets[bi] = Bucket::rebuilt(bit, entries)?;
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
        Ok(())
    }
}

impl Inibin {
    /// Reads an inibin from a byte slice. Same bytes, same parser, and the same byte-exact
    /// round-trip as [`from_bytes`](rs_io::Parse::from_bytes), which this forwards to.
    pub fn from_slice(data: &[u8]) -> Result<Self> {
        <Self as rs_io::Parse>::from_bytes(data)
    }

    /// The v2 body, or `None` for a version-1 file.
    pub fn v2(&self) -> Option<&TroybinV2> {
        match &self.body {
            TroybinBody::V2(body) => Some(body),
            TroybinBody::V1(_) => None,
        }
    }

    /// The mutable v2 body, or `None` for a version-1 file, which this crate treats as read-only.
    pub fn v2_mut(&mut self) -> Option<&mut TroybinV2> {
        match &mut self.body {
            TroybinBody::V2(body) => Some(body),
            TroybinBody::V1(_) => None,
        }
    }

    /// The value types present in the file: the v2 bucket types in on-disk order, or the single
    /// [`OldFormat`](InibinFlags::OldFormat) for a version-1 body.
    pub fn flags(&self) -> Result<Vec<InibinFlags>> {
        match &self.body {
            TroybinBody::V2(body) => body.bucket_flags(),
            TroybinBody::V1(_) => Ok(vec![InibinFlags::OldFormat]),
        }
    }

    /// The v2 property keyed by `hash`. Always `None` for a version-1 body, whose flat table carries
    /// no value typing.
    pub fn get_hash(&self, hash: u32) -> Option<ScalarValue> {
        self.v2()?.get(hash)
    }

    /// Every `(hash, value)` pair in the file, in bucket-then-insertion order. Empty for a
    /// version-1 body.
    pub fn entries(&self) -> Vec<(u32, ScalarValue)> {
        self.v2().map(|b| b.iter().collect()).unwrap_or_default()
    }

    /// Overwrites or inserts a v2 property by `hash`, letting the value's type pick its bucket.
    /// Errors on a version-1 body.
    pub fn set_hash(&mut self, hash: u32, value: ScalarValue) -> Result<()> {
        self.v2_mut()
            .ok_or(Error::UnsupportedVersion(1))?
            .insert(hash, value)?;
        Ok(())
    }

    /// Removes a v2 property by `hash`, returning its value. `Ok(None)` for a version-1 body.
    pub fn remove_hash(&mut self, hash: u32) -> Result<Option<ScalarValue>> {
        match self.v2_mut() {
            Some(body) => body.remove(hash),
            None => Ok(None),
        }
    }
}
