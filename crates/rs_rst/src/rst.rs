use rs_hash::xxh3_64;

use crate::error::Result;

/// Default RST version produced by [`Rst::new`]: the current 38-bit layout with no font config
/// and no legacy mode byte.
pub const DEFAULT_VERSION: u8 = 5;

/// A single entry value: either a normal localized string or a legacy pre-v5 "translation
/// encrypted" payload whose raw bytes are not valid UTF-8.
///
/// Pre-v5 files whose `mode` byte is non-zero may store some entries as an encrypted blob
/// (`0xFF`, a `u16` length, then that many raw bytes) instead of a NUL-terminated string. Those
/// bytes are kept verbatim in [`RstValue::Encrypted`] so the file round-trips byte-for-byte even
/// though their plaintext cannot be recovered without the per-file key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RstValue {
    Text(String),
    Encrypted(Vec<u8>),
}

impl RstValue {
    /// The decoded string, or `None` for an encrypted payload.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            RstValue::Text(s) => Some(s.as_str()),
            RstValue::Encrypted(_) => None,
        }
    }

    /// The raw on-blob bytes: UTF-8 for text, the ciphertext for encrypted entries.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            RstValue::Text(s) => s.as_bytes(),
            RstValue::Encrypted(b) => b.as_slice(),
        }
    }
}

impl From<String> for RstValue {
    fn from(s: String) -> Self {
        RstValue::Text(s)
    }
}

impl From<&str> for RstValue {
    fn from(s: &str) -> Self {
        RstValue::Text(s.to_owned())
    }
}

/// A parsed RST string table.
///
/// `entries` keeps every `(hash, value)` pair in file order; the hash is the xxh3-64 of the key
/// truncated to [`hash_bits`](Rst::hash_bits). `font_config` holds the optional v2 configuration
/// string and `mode` the single byte present before v5 (non-zero means some entries may be
/// encrypted), both retained so unsupported-feature files still round-trip byte-for-byte.
///
/// The string blob in a real file lays its distinct values out in an order that is independent of
/// the entry table, so reproducing it byte-for-byte requires remembering that layout. `blob_order`
/// captures the exact sequence of distinct values as they appear in the source blob; the writer
/// emits those first, then appends any value not present in it. Tables built in memory leave it
/// empty, in which case the writer falls back to first-seen entry order.
#[derive(Debug, Clone)]
pub struct Rst {
    pub version: u8,
    pub font_config: Option<String>,
    pub mode: u8,
    pub entries: Vec<(u64, RstValue)>,
    pub(crate) blob_order: Vec<RstValue>,
}

/// Two tables are equal when their logical content matches. `blob_order` is an internal hint that
/// only reproduces the source blob layout byte-for-byte and never changes which value a key
/// resolves to, so it is deliberately excluded from equality.
impl PartialEq for Rst {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
            && self.font_config == other.font_config
            && self.mode == other.mode
            && self.entries == other.entries
    }
}

impl Eq for Rst {}

impl Rst {
    pub fn new() -> Self {
        Self {
            version: DEFAULT_VERSION,
            font_config: None,
            mode: 0,
            entries: Vec::new(),
            blob_order: Vec::new(),
        }
    }

    /// Creates an empty table for an explicit `version`.
    pub fn with_version(version: u8) -> Self {
        Self {
            version,
            ..Self::new()
        }
    }

    /// Number of low bits the key hash is truncated to for `version`, or `None` if the version is
    /// not supported. v2/v3 use 40 bits, v4/v5 use 38.
    pub fn hash_bits_for(version: u8) -> Option<u32> {
        match version {
            2 | 3 => Some(40),
            4 | 5 => Some(38),
            _ => None,
        }
    }

    /// Bit width this table's hashes are truncated to.
    pub fn hash_bits(&self) -> Option<u32> {
        Self::hash_bits_for(self.version)
    }

    /// Mask applied to a key hash for `version`, or `None` if the version is unsupported.
    pub fn hash_mask_for(version: u8) -> Option<u64> {
        Self::hash_bits_for(version).map(|bits| (1u64 << bits) - 1)
    }

    /// xxh3-64 of `key`, ASCII-lowercased and truncated to the mask for `version`.
    pub fn hash_key(version: u8, key: &str) -> Option<u64> {
        Self::hash_mask_for(version).map(|mask| xxh3_64(key) & mask)
    }

    /// Hashes `key` for this table's version and appends the `(hash, value)` entry, preserving
    /// insertion order. Returns the entry's hash, or `None` if the version is unsupported.
    pub fn add(&mut self, key: &str, value: impl Into<RstValue>) -> Option<u64> {
        let hash = Self::hash_key(self.version, key)?;
        self.entries.push((hash, value.into()));
        Some(hash)
    }

    /// Looks up the first value whose stored hash matches `key` under this table's version,
    /// returning its decoded string. Encrypted entries resolve to `None`; use
    /// [`value_by_hash`](Rst::value_by_hash) to reach their raw bytes.
    pub fn get(&self, key: &str) -> Option<&str> {
        let hash = Self::hash_key(self.version, key)?;
        self.get_by_hash(hash)
    }

    /// Looks up the first value stored under the raw (already masked) `hash`, returning its
    /// decoded string. Encrypted entries resolve to `None`.
    pub fn get_by_hash(&self, hash: u64) -> Option<&str> {
        self.value_by_hash(hash).and_then(RstValue::as_str)
    }

    /// Looks up the first [`RstValue`] stored under the raw (already masked) `hash`, exposing
    /// encrypted payloads as well as plain strings.
    pub fn value_by_hash(&self, hash: u64) -> Option<&RstValue> {
        self.entries
            .iter()
            .find(|(h, _)| *h == hash)
            .map(|(_, v)| v)
    }

    pub(crate) fn check_version(version: u8) -> Result<u32> {
        Self::hash_bits_for(version).ok_or(crate::Error::UnsupportedVersion(version))
    }
}

impl Default for Rst {
    fn default() -> Self {
        Self::new()
    }
}
