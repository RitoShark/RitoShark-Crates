#![forbid(unsafe_code)]
/*!
rs_bin reads and writes the PROP/.bin format and its `#PROP_text` representation. The reader
parses the full binary layout into an owned `BinValue` tree, allocating only at leaves, and the
writer reproduces bytes exactly by backfilling the on-disk size fields, preserving magic and
version, linked-file order, entry and field order, the `LIST`/`LIST2` distinction, pointer versus
embed, option presence, the trailing `PTCH` patches section, and every raw hash, so binary
round-trips are lossless. The `text` module both prints and parses the editable ritobin text form,
so `bin → text → bin` reconstructs the original document exactly.
*/

mod bin;
mod error;
mod read;
mod write;

pub mod text;

pub use bin::{Bin, BinEntry, BinPatch, BinType, BinValue};
pub use error::{Error, Result};
pub use text::{from_text, to_text};
