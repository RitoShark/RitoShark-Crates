/*!
The `#PROP_text` representation. `to_text` renders a [`Bin`] in the human-editable ritobin text
form, resolving hashes to names through an optional [`HashMapper`] and falling back to hex when a
name is unknown. `from_text` is a best-effort stub: the headline contract for this crate is the
byte-exact binary round-trip, and the text parser is not yet implemented.
*/

mod print;

pub use print::to_text;

use rs_hash::HashMapper;

use crate::bin::Bin;
use crate::error::{Error, Result};

/// Parses `#PROP_text` back into a [`Bin`]. Not yet implemented; returns [`Error::Unsupported`].
pub fn from_text(_text: &str, _mapper: Option<&HashMapper>) -> Result<Bin> {
    Err(Error::Unsupported("text parsing is not implemented"))
}
