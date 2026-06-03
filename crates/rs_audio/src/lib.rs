#![forbid(unsafe_code)]
/*!
rs_audio reads and writes the Wwise `.wpk` and `.bnk` containers League ships audio in, at the
container level only: it extracts and repacks the embedded `.wem` blobs without interpreting the
Wwise event graph or object hierarchy. The WPK reader walks the per-file offset table to each
named entry and its bytes and rebuilds a canonical layout on write; the BNK reader keeps every
chunked section verbatim so unknown sections survive a byte-exact round-trip, and exposes the
DIDX/DATA pair as embedded `.wem` audio.
*/

mod bnk;
mod error;
mod wpk;

pub use bnk::{Bnk, BnkSection};
pub use error::{Error, Result};
pub use wpk::{WemEntry, Wpk};
