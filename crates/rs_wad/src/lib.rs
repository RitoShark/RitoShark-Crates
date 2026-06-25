/*!
rs_wad reads and writes League `.wad`/`.wad.client` archives. The reader parses the `RW` header and
the chunk table of contents and captures the ECDSA-signature/checksum header span and the packed
data section verbatim, so writing reproduces the archive byte-for-byte. Chunk payloads are read by
absolute offset from that captured data section and decompressed on demand: stored, gzip
(flate2), zstd, and the zstd-multi split-frame encoding are supported; the satellite encoding is
not. Versions 2 and 3 parse. The 32-byte table entry has two layouts that differ only in their
3-byte subchunk region: v3.0–3.3 store an `is_duplicated` flag (u8) plus a 16-bit first-subchunk
index, while v3.4+ drops the flag (always false) and widens the index to 24 bits (packed hi/lo/mi).

It also *builds* archives: [`WadBuilder`] streams a v3.4 WAD from loose files, zstd-compressing and
deduplicating chunks and laying out a sorted table of contents. [`compress`] is the symmetric
counterpart to [`decompress`]. Built archives are valid and round-trip exactly, but are not
byte-identical to other tools because zstd encoder choices differ.
*/

#![forbid(unsafe_code)]

mod builder;
mod chunk;
mod decoder;
mod encoder;
mod error;
mod wad;

pub use builder::WadBuilder;
pub use chunk::{WadChunk, WadCompression, WadSubchunk};
pub use decoder::{decompress, decompress_zstd_multi_with_toc};
pub use encoder::{DEFAULT_ZSTD_LEVEL, compress};
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
