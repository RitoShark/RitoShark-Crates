/*!
rs_hash provides the hashing primitives and hash-dictionary lookup shared by every format crate.
FNV1a-32 (a `const fn`) hashes bin field, class, and entry names; XXH64 and xxh3-64 hash file
paths and string-table keys. All hashers lowercase ASCII input first, matching the on-disk
convention. `HashMapper` loads CDTB-style `<hex> <name>` dictionaries so raw integer hashes can be
resolved back to their original names for display.
*/

#![forbid(unsafe_code)]

mod error;
mod fnv;
mod mapper;
mod xx;

pub use error::{Error, Result};
pub use fnv::fnv1a;
pub use mapper::HashMapper;
pub use xx::{xxh3_64, xxh64};
