#![forbid(unsafe_code)]
/*!
rs_anim reads and writes League skeleton (`.skl`) and animation (`.anm`) files. The skeleton side
handles the modern `0x22FD4FC3` format: a flat joint list with local and inverse-bind transforms,
the skin-influence list, and skeleton/asset names, round-tripping byte-for-byte. The animation side
reads the uncompressed `r3d2anmd` container in versions 3, 4, and 5, expanding the shared
vector/quaternion palettes into explicit per-joint keyframes, and writes version 4 so frame values
survive a round-trip without quantization loss. Legacy skeletons and the compressed `r3d2canm`
animation container are reported as errors rather than parsed.
*/

mod animation;
mod animation_read;
mod animation_write;
mod error;
pub mod quantized;
mod skeleton;
mod skeleton_read;
mod skeleton_write;

pub use animation::{AnimFrame, AnimTrack, Animation};
pub use error::{Error, Result};
pub use skeleton::{Joint, Skeleton};
