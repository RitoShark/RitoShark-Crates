#![forbid(unsafe_code)]
/*!
rs_audio reads and writes the Wwise containers League ships audio in — `.bnk` soundbanks and
`.wpk` audio packages — and decodes the `.wem` files inside them.

The containers are layout-preserving: the BNK reader keeps every chunked section verbatim so
unknown sections survive a byte-exact round-trip, and the WPK writer reproduces Riot's eight-byte
alignment so a real package re-serializes byte for byte. Both expose the embedded `.wem` blobs.

`Wem` decodes those blobs. League is entirely Wwise Vorbis, which stores its audio with the
Vorbis headers stripped and its codebooks replaced by indices into an external library; decoding
rebuilds the headers, expands the referenced codebooks, and repages the audio packets without
re-encoding, so the result is bit-exact rather than a transcode.
*/

mod bnk;
mod edit;
mod error;
mod hirc;
mod wem;
mod wpk;

pub use bnk::{Bnk, BnkSection};
pub use error::{Error, Result};
pub use hirc::{
    Action, Container, Event, HircBody, HircKind, HircObject, HircSection, MusicSwitch, MusicTrack,
    Sound, SwitchContainer,
};
pub use wem::{AudioFormat, DecodedAudio, PcmAudio, Wem, WemCodec, WemFormat, encode_pcm, silence};
pub use wpk::{WemEntry, Wpk};
