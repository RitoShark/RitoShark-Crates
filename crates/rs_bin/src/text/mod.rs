/*!
The `#PROP_text` representation. `to_text` renders a [`Bin`] in the human-editable ritobin text
form, resolving hashes to names through an optional [`HashMapper`] and falling back to hex when a
name is unknown. `from_text` is the matching recursive-descent parser: it reads the header, the
`name: type = value` sections, and every value type recursively, accepting hashes as `0xHEX` or as
barewords/strings it hashes itself, so `to_text` followed by `from_text` reconstructs the original
[`Bin`] exactly.
*/

mod parse;
mod print;

pub use parse::from_text;
pub use print::to_text;
