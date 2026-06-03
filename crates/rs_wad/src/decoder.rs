use std::io::{Cursor, Read};

use crate::chunk::WadCompression;
use crate::error::{Error, Result};

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/** Decompresses one chunk's raw bytes according to its [`WadCompression`]. `None` copies the
input; `Gzip` inflates with flate2; `Zstd` decodes a single frame. `ZstdMulti` holds one zstd
frame per subchunk concatenated end to end (optionally after a stored prefix); the decoder copies
any bytes before the first frame verbatim and then streams the concatenated frames in order, which
the zstd reader decodes one after another until the input is exhausted. `uncompressed_size` sizes
the output buffer up front. `Satellite` is unsupported and returns
[`Error::UnsupportedCompression`]. */
pub fn decompress(
    raw: &[u8],
    compression: WadCompression,
    uncompressed_size: usize,
) -> Result<Vec<u8>> {
    match compression {
        WadCompression::None => Ok(raw.to_vec()),
        WadCompression::Gzip => decompress_gzip(raw, uncompressed_size),
        WadCompression::Satellite => Err(Error::UnsupportedCompression(WadCompression::Satellite as u8)),
        WadCompression::Zstd => decompress_zstd(raw, uncompressed_size),
        WadCompression::ZstdMulti => decompress_zstd_multi(raw, uncompressed_size),
    }
}

fn decompress_gzip(raw: &[u8], uncompressed_size: usize) -> Result<Vec<u8>> {
    let mut out = vec![0u8; uncompressed_size];
    flate2::read::GzDecoder::new(Cursor::new(raw))
        .read_exact(&mut out)
        .map_err(|e| Error::Decompress(e.to_string()))?;
    Ok(out)
}

fn decompress_zstd(raw: &[u8], uncompressed_size: usize) -> Result<Vec<u8>> {
    let mut out = vec![0u8; uncompressed_size];
    zstd::Decoder::new(Cursor::new(raw))
        .and_then(|mut d| d.read_exact(&mut out))
        .map_err(|e| Error::Decompress(e.to_string()))?;
    Ok(out)
}

fn decompress_zstd_multi(raw: &[u8], uncompressed_size: usize) -> Result<Vec<u8>> {
    let magic_offset = raw
        .windows(ZSTD_MAGIC.len())
        .position(|w| w == ZSTD_MAGIC)
        .ok_or_else(|| Error::Decompress(String::from("no zstd frame in multi chunk")))?;

    let mut out = vec![0u8; uncompressed_size];
    if magic_offset > out.len() || magic_offset > raw.len() {
        return Err(Error::Decompress(String::from("zstd frame offset out of range")));
    }
    out[..magic_offset].copy_from_slice(&raw[..magic_offset]);
    zstd::Decoder::new(Cursor::new(&raw[magic_offset..]))
        .and_then(|mut d| d.read_exact(&mut out[magic_offset..]))
        .map_err(|e| Error::Decompress(e.to_string()))?;
    Ok(out)
}
