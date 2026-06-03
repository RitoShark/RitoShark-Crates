#![forbid(unsafe_code)]
/*!
rs_anim reads and writes League skeleton (`.skl`) and animation (`.anm`) files. The skeleton side
handles the modern `0x22FD4FC3` format: a flat joint list with local and inverse-bind transforms,
the skin-influence list, and skeleton/asset names, round-tripping byte-for-byte. The animation side
reads the uncompressed `r3d2anmd` container in versions 3, 4, and 5, expanding the shared
vector/quaternion palettes into explicit per-joint keyframes. Version 5 additionally retains its raw
sections so `read -> write` reproduces the original bytes exactly; editing a v5 animation (after
`make_editable`) or writing a v3/v4/in-memory animation emits version 4, where full quaternions
survive a round-trip without quantization loss. It also decodes the compressed `r3d2canm`
container (versions 1-3): the sparse, per-component quantized keyframe stream is dequantized and
resampled into the same explicit per-joint keyframes. Legacy skeletons are reported as errors.
*/

mod animation;
mod animation_read;
mod animation_write;
mod error;
pub mod quantized;
mod raw;
mod skeleton;
mod skeleton_read;
mod skeleton_write;

pub use animation::{AnimFrame, AnimTrack, Animation};
pub use error::{Error, Result};
pub use skeleton::{Joint, Skeleton};
