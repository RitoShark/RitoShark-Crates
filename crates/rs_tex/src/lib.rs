#![forbid(unsafe_code)]
/*!
rs_tex reads and writes the League `.tex` extended texture format and reads/writes `.dds`
containers. The `.tex` reader follows the on-disk header exactly (magic, dimensions, format byte,
two unknown bytes, mip flag) and keeps the mip chain byte-for-byte so the writer reproduces it
losslessly. Decoding decompresses block-compressed payloads into a BGRA buffer and reorders it
into RGBA, while uncompressed BGRA8/RGBA16_SNORM are reordered directly, all yielding an
`RgbaImage`. The encoder block-compresses an `RgbaImage` into BC1/BC3 (BC5 too), generating a
Lanczos-3 mip chain on request, to produce a valid `.tex`. DDS parsing reads the header, resolves
the pixel format, and decodes every surface (including all six cubemap faces and array layers);
the DDS writer emits an uncompressed RGBA8 surface.
*/

mod dds;
mod decode;
mod encode;
mod error;
mod read;
mod texture;
mod write;

pub use dds::{
    dds_is_cubemap, read_dds, read_dds_bytes, read_dds_faces, read_dds_faces_bytes, save_dds,
    write_dds_bytes,
};
pub use error::{Error, Result};
pub use read::TEX_MAGIC;
pub use texture::{TexFormat, Texture};
