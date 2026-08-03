/*!
The `#PROP_text` representation. `to_text` renders a [`Bin`] in the human-editable ritobin text
form, resolving hashes to names through an optional [`HashMapper`] and falling back to hex when a
name is unknown. `from_text` is the matching recursive-descent parser: it reads the header, the
`name: type = value` sections, and every value type recursively, accepting hashes as `0xHEX` or as
barewords/strings it hashes itself, so `to_text` followed by `from_text` reconstructs the original
[`Bin`] exactly.

`value_to_text` / `value_from_text` are the same pair scoped to ONE [`crate::BinValue`], for editors
that show a single node (one VFX emitter, say) as editable ritobin text rather than widget rows. They
share the whole-file printer and grammar, so a node printed standalone is formatted exactly as it
would be inside a document and parses back through the same readers. A struct root prints its class
header (`VfxEmitterDefinitionData { ... }`) and carries no type tag of its own, so
`value_from_text_as` exists for the caller that already knows the replaced node's type and must not
let a pointer/embed pair, which print identically, swap places.
*/

mod parse;
mod print;

// MERGE: both sides added API and neither supersedes the other - the single-NODE print/parse
// (value_to_text / value_from_text / value_from_text_as, for editing one subtree as text) and the
// whole-file rendering OPTIONS (TextOptions / to_text_with). Re-export both.
pub use parse::{from_text, value_from_text, value_from_text_as};
pub use print::{TextOptions, to_text, to_text_with, value_to_text};
