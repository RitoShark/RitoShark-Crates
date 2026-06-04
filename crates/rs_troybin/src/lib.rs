#![forbid(unsafe_code)]
/*!
rs_troybin reads and writes the legacy League `.troybin` particle/VFX parameter format. Values are
keyed by a 32-bit 65599 `ihash` of their `section/name` pair; the binary stores only hashes, so the
model keeps raw hashes as the source of truth and never applies the display multipliers the old
converters did. Version 2 lays values out in up to fourteen typed buckets selected by a flags word;
version 1 is a flat `(hash, offset)` table into a string blob. The reader preserves bucket order,
per-bucket hash order, raw typed values, and the version-1 blob verbatim, and the writer rebuilds
the file byte-for-byte. The format is the same one inibin v1/v2 uses.
*/

mod error;
mod read;
mod troybin;
mod write;

pub use error::{Error, Result};
pub use troybin::{Bucket, BucketValues, Troybin, TroybinBody, TroybinV1, TroybinV2, V1Entry};
