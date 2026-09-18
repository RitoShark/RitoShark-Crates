#![forbid(unsafe_code)]
/*!
rs_bin reads and writes the PROP/.bin format and its `#PROP_text` representation. The reader
parses the full binary layout into an owned `BinValue` tree, allocating only at leaves, and the
writer reproduces bytes exactly by backfilling the on-disk size fields, preserving magic and
version, linked-file order, entry and field order, the `LIST`/`LIST2` distinction, pointer versus
embed, option presence, the trailing `PTCH` patches section, and every raw hash, so binary
round-trips are lossless. The `text` module both prints and parses the editable ritobin text form,
so `bin -> text -> bin` reconstructs the original document exactly, and `value_to_text` /
`value_from_text` do the same for a SINGLE `BinValue` so an editor can show one node (one VFX
emitter, say) as editable text. `PathMap` is the `ritobinmap`
record: the names a mod invented, stored as an ordinary top-level entry so every parser carries
them, one `list[string]` per hash category. `Bin::trailing` carries whatever a tool appended after
the declared body, where the superseded `CELMAP` footer (`Trailer`) still reads for migration.
*/

mod bin;
mod blend;
mod error;
mod pathmap;
mod read;
mod trailer;
mod write;

pub mod text;

pub use bin::{Bin, BinEntry, BinPatch, BinType, BinValue};
pub use blend::{BLEND_DATA_TABLE, BLEND_KEY_FIELDS, BlendKey, is_blend_key_field};
pub use error::{Error, Result};
pub use pathmap::{
    PathMap, PathTables, RITOBINMAP_CLASS, RITOBINMAP_CLASS_NAME, RITOBINMAP_ENTRY,
    RITOBINMAP_ENTRY_NAME, capture, read_path_map, strip_path_map, write_path_map,
};
pub use trailer::{Trailer, read_trailer, strip_trailer};
// MERGE: union of both sides - see the note in `text/mod.rs`.
pub use text::{
    TextOptions, from_text, to_text, to_text_with, value_from_text, value_from_text_as,
    value_to_text,
};
