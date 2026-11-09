#![forbid(unsafe_code)]
/*!
rs_anim reads and writes League skeleton (`.skl`) and animation (`.anm`) files. The skeleton side
handles the modern `0x22FD4FC3` format: a flat joint list with local and inverse-bind transforms,
the skin-influence list, and skeleton/asset names, round-tripping byte-for-byte. The animation side
reads the uncompressed `r3d2anmd` container in versions 3, 4, and 5, expanding the shared
vector/quaternion palettes into explicit per-joint keyframes, and decodes the compressed `r3d2canm`
container (versions 1-3) by dequantizing and resampling its sparse, per-component keyframe stream
into the same explicit keyframes. v3 joint hashes use the shared lowercased ELF hash
(`rs_hash::elf_lower`). Every accepted container additionally retains its complete source bytes, so
an unedited `read -> write` reproduces the original file byte-for-byte (uncompressed v3/v4/v5 and
compressed `r3d2canm` alike). Editing a parsed animation (after `make_editable`) or writing an
in-memory animation emits uncompressed version 4, where full quaternions survive a round-trip
without quantization loss. Legacy skeleton versions 1 and 2 are also decoded; unchanged skeletons
retain their original bytes, while edited skeletons are emitted as modern version 0.

For explicit selection with automatic fallback, use [`Animation::export_bytes`] or
[`Animation::export_to_path`] with [`AnimationExportOptions`]. The selected format starts
CANM -> v5 -> v4 (or a suffix of that chain); [`AnimationExport`] reports actual format,
fallback reasons and measured errors. The adaptive frame-input fitter streams validation
and preserves the caller's quality targets. Existing strict writers remain available.

Explicit dense v5 writing uses [`Animation::to_v5_bytes`], [`Animation::write_v5`] and
[`Animation::to_v5_path`], retaining every sample and float32 vectors with 48-bit rotations.

Explicit compressed animation writing is available through [`Animation::to_compressed_bytes`],
[`Animation::write_compressed`] and [`Animation::to_compressed_path`]. [`CompressionOptions`]
controls rotation (radians), translation and scale errors on a subframe validation grid,
including quantization. Supplying a skeleton also checks model-space joint probes. The encoder reduces keys using the playback cubic, emits seek tables with 16- or
32-bit indices as required, and validates the finished stream by decoding it. Unattainable
precision is an error rather than silently relaxing a caller's tolerances.

On top of the decoded keyframes the crate poses a rig: [`AnimTrack::sample`] interpolates a track at
an arbitrary time, and [`Pose`] binds an animation to a [`Skeleton`], composing the joint hierarchy
into model-space and skinning matrices. Joints an animation does not drive keep their bind pose, so
a clip that animates part of a rig poses the whole of it correctly.
*/

mod animation;
mod animation_export;
mod animation_read;
mod animation_v5_write;
mod animation_write;
mod compressed;
mod compressed_quality;
mod compressed_write;
mod error;
mod pose;
pub mod quantized;
mod raw;
mod skeleton;
mod skeleton_read;
mod skeleton_write;

pub use animation::{AnimFrame, AnimTrack, Animation};
pub use animation_export::{
    AnimationExport, AnimationExportMetrics, AnimationExportOptions, AnimationFallback,
    AnimationFormat,
};
pub use compressed_write::CompressionOptions;
pub use error::{Error, Result};
pub use pose::{Pose, Transform};
pub use skeleton::{Joint, Skeleton};
