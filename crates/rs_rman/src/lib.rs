#![forbid(unsafe_code)]
/*!
rs_rman reads the RMAN release-manifest format. The reader parses the fixed header, decompresses
the zstd body, and walks the body's FlatBuffer-style offset tables into owned bundles (with their
chunks), files (name, size, ordered chunk ids, directory and permissions), and directories, then
reconstructs full file paths by following each directory's parent chain. Every body read is
bounds-checked so malformed input is an error rather than a panic. Writing is not yet implemented
and returns `Unsupported`.
*/

mod error;
mod read;
mod rman;
mod write;

pub use error::{Error, Result};
pub use rman::{Bundle, Chunk, Directory, FileEntry, Rman};
