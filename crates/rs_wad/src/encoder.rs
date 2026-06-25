use crate::chunk::WadCompression;
use crate::error::{Error, Result};

/// Default zstd level used by [`crate::WadBuilder`]; matches what League's own tooling ships with.
pub const DEFAULT_ZSTD_LEVEL: i32 = 3;

/** Compresses `data` for storage in a WAD chunk's data section, the inverse of
[`crate::decompress`]. `None` stores the bytes verbatim; `Gzip` deflates with flate2; `Zstd`
encodes a single frame at `level`. `Satellite` and `ZstdMulti` cannot be produced here (the
satellite codec is proprietary and multi-frame chunks need an explicit sub-chunk table) and return
[`Error::UnsupportedCompression`]. The returned bytes are what get written to disk and what the
chunk's xxh3-64 checksum is taken over. */
pub fn compress(data: &[u8], compression: WadCompression, level: i32) -> Result<Vec<u8>> {
    match compression {
        WadCompression::None => Ok(data.to_vec()),
        WadCompression::Gzip => compress_gzip(data),
        WadCompression::Zstd => compress_zstd(data, level),
        WadCompression::Satellite => Err(Error::UnsupportedCompression(
            WadCompression::Satellite as u8,
        )),
        WadCompression::ZstdMulti => Err(Error::UnsupportedCompression(
            WadCompression::ZstdMulti as u8,
        )),
    }
}

fn compress_gzip(data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(data)
        .and_then(|_| encoder.finish())
        .map_err(|e| Error::Decompress(e.to_string()))
}

fn compress_zstd(data: &[u8], level: i32) -> Result<Vec<u8>> {
    zstd::encode_all(data, level).map_err(|e| Error::Decompress(e.to_string()))
}
