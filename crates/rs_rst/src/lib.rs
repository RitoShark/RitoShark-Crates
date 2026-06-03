/*!
rs_rst reads and writes the RST string table: a list of xxh3-64 key hashes paired with
localized strings. Each hash is truncated to a version-dependent width (40 bits for v2/v3,
38 bits for v4/v5) and packed with its blob offset into one little-endian `u64`. The reader
resolves every entry against the trailing null-terminated string blob — decoding the legacy
`0xFF`-framed encrypted payloads that pre-v5 files gate behind a non-zero mode byte — and the
writer rebuilds the table and blob byte-for-byte, preserving entry order, the optional v2 font
config, the mode byte, and any encrypted payloads.
*/

#![forbid(unsafe_code)]

mod error;
mod read;
mod rst;
mod write;

pub use error::{Error, Result};
pub use rst::{Rst, RstValue};
