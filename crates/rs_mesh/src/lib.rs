#![forbid(unsafe_code)]
/*!
rs_mesh reads and writes the League of Legends mesh formats: the `.skn` skinned mesh and the
`.scb`/`.sco` static mesh. The skinned reader keeps the on-disk version, flags, vertex layout,
and bounds verbatim and stores the index and vertex buffers without dropping degenerate
triangles, so `from_reader` followed by `to_writer` reproduces the input bytes for the common
versions (1, 2, and 4). The static side parses the binary `r3d2Mesh` container and the text
`[ObjectBegin]` form into a shared position/face/material model.
*/

mod error;
mod read;
mod skinned;
mod static_mesh;
mod static_read;
mod write;

pub use error::{Error, Result};
pub use skinned::{SkinnedMesh, SkinnedMeshRange, SkinnedMeshVertex, SkinnedMeshVertexType};
pub use static_mesh::{StaticMesh, StaticMeshFace};
