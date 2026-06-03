/*!
rs_wad reads and writes League `.wad`/`.wad.client` archives. The reader parses the `RW` header and
the chunk table of contents and captures the ECDSA-signature/checksum header span and the packed
data section verbatim, so writing reproduces the archive byte-for-byte. Chunk payloads are read by
absolute offset from that captured data section and decompressed on demand: stored, gzip
(flate2), zstd, and the zstd-multi split-frame encoding are supported; the satellite encoding is
not. Versions 2 and 3 parse; every v3 minor shares one 32-byte table-entry layout (the
duplicate flag and 16-bit first-subchunk index), so v3.4 reads with the same code path as v3.0.
*/

#![forbid(unsafe_code)]

mod chunk;
mod decoder;
mod error;
mod wad;

pub use chunk::{WadChunk, WadCompression};
pub use decoder::decompress;
pub use error::{Error, Result};
pub use wad::Wad;

impl rs_io::Parse for Wad {
    type Error = Error;

    fn from_reader<R: std::io::Read + std::io::Seek>(reader: &mut R) -> Result<Self> {
        Wad::from_reader(reader)
    }
}

impl rs_io::Serialize for Wad {
    type Error = Error;

    fn to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<()> {
        Wad::to_writer(self, writer)
    }
}
