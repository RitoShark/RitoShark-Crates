use rs_hash::xxh3_64;

use crate::error::Result;

/// Default RST version produced by [`Rst::new`]: the current 39-bit layout with no font config
/// and no legacy mode byte.
pub const DEFAULT_VERSION: u8 = 5;

/// A parsed RST string table.
///
/// `entries` keeps every `(hash, string)` pair in file order; the hash is the xxh3-64 of the key
/// truncated to [`hash_bits`](Rst::hash_bits). `font_config` holds the optional v2 configuration
/// string and `mode` the single byte present before v5, both retained so unsupported-feature files
/// still round-trip byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rst {
    pub version: u8,
    pub font_config: Option<String>,
    pub mode: u8,
    pub entries: Vec<(u64, String)>,
}

impl Rst {
    pub fn new() -> Self {
        Self {
            version: DEFAULT_VERSION,
            font_config: None,
            mode: 0,
            entries: Vec::new(),
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
    /// not supported. v2/v3 use 40 bits, v4/v5 use 39.
    pub fn hash_bits_for(version: u8) -> Option<u32> {
        match version {
            2 | 3 => Some(40),
            4 | 5 => Some(39),
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

    /// xxh3-64 of `key`, lowercased and truncated to the mask for `version`.
    pub fn hash_key(version: u8, key: &str) -> Option<u64> {
        Self::hash_mask_for(version).map(|mask| xxh3_64(key) & mask)
    }

    /// Hashes `key` for this table's version and appends the `(hash, value)` entry, preserving
    /// insertion order. Returns the entry's hash, or `None` if the version is unsupported.
    pub fn add(&mut self, key: &str, value: impl Into<String>) -> Option<u64> {
        let hash = Self::hash_key(self.version, key)?;
        self.entries.push((hash, value.into()));
        Some(hash)
    }

    /// Looks up the first string whose stored hash matches `key` under this table's version.
    pub fn get(&self, key: &str) -> Option<&str> {
        let hash = Self::hash_key(self.version, key)?;
        self.get_by_hash(hash)
    }

    /// Looks up the first string stored under the raw (already masked) `hash`.
    pub fn get_by_hash(&self, hash: u64) -> Option<&str> {
        self.entries
            .iter()
            .find(|(h, _)| *h == hash)
            .map(|(_, s)| s.as_str())
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
