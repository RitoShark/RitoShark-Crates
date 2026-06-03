#![forbid(unsafe_code)]
/*!
rs_tex reads and writes the League `.tex` extended texture format and reads `.dds` containers.
The `.tex` reader follows the on-disk header exactly (magic, dimensions, format byte, two
unknown bytes, mip flag) and keeps the mip chain byte-for-byte so the writer reproduces it
losslessly. Decoding decompresses block-compressed payloads into a BGRA buffer and reorders it
into RGBA, while uncompressed BGRA8 is reordered directly, both yielding an `RgbaImage`. DDS
parsing reads the header, resolves the pixel format, and decodes the main image.
*/

mod decode;
mod dds;
mod error;
mod read;
mod texture;
mod write;

pub use dds::{read_dds, read_dds_bytes};
pub use error::{Error, Result};
pub use read::TEX_MAGIC;
pub use texture::{TexFormat, Texture};
