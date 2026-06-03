#![forbid(unsafe_code)]
/*!
RitoShark is a workspace of focused crates for reading and writing League of Legends file
formats. This umbrella re-exports each crate under a short module name so consumers can depend
on a single crate and reach everything through one path. The foundation modules (`io`, `hash`,
`math`, `file`) are always present; each format module is gated behind a feature of the same
name, all enabled by default.
*/

pub use rs_file as file;
pub use rs_hash as hash;
pub use rs_io as io;
pub use rs_math as math;

#[cfg(feature = "bin")]
pub use rs_bin as bin;
#[cfg(feature = "wad")]
pub use rs_wad as wad;
#[cfg(feature = "tex")]
pub use rs_tex as tex;
#[cfg(feature = "mesh")]
pub use rs_mesh as mesh;
#[cfg(feature = "anim")]
pub use rs_anim as anim;
#[cfg(feature = "mapgeo")]
pub use rs_mapgeo as mapgeo;
#[cfg(feature = "rst")]
pub use rs_rst as rst;
#[cfg(feature = "rman")]
pub use rs_rman as rman;
#[cfg(feature = "audio")]
pub use rs_audio as audio;

/// The shared parse/serialize traits, re-exported for ergonomic `use ritoshark::prelude::*;`.
pub mod prelude {
    pub use rs_io::{Parse, ReaderExt, Serialize, WriterExt};
}
