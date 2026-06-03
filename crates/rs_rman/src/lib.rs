#![forbid(unsafe_code)]
/*!
rs_rman reads the RMAN release-manifest format. The reader parses the fixed header, decompresses
the zstd body, and walks the body's FlatBuffer-style offset tables into owned bundles (with their
chunks), files (name, size, ordered chunk ids, directory, permissions and a locale/platform flag
mask), directories, and the flags table, then reconstructs full file paths by following each
directory's parent chain. Helpers resolve a file's flag tags, filter files by locale/platform, and
compute each file's ordered chunk byte-ranges within their bundles (the basis for extraction).
Every body read is bounds-checked so malformed input is an error rather than a panic. Writing is
not yet implemented and returns `Unsupported`.
*/

mod error;
mod read;
mod rman;
mod write;

pub use error::{Error, Result};
pub use rman::{Bundle, Chunk, ChunkRange, Directory, FileEntry, FileFlag, Rman};
