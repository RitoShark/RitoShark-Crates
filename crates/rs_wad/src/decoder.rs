use std::io::{Cursor, Read};

use crate::chunk::{WadCompression, WadSubchunk};
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
        WadCompression::Satellite => Err(Error::UnsupportedCompression(
            WadCompression::Satellite as u8,
        )),
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

/** Decodes a [`WadCompression::ZstdMulti`] chunk using an explicit sub-chunk table, matching the
C# oracle exactly. The `subchunks` slice must be the parent chunk's run of the `.subchunktoc`
(`subchunk_start .. subchunk_start + subchunk_count`); each entry's `compressed_size` carves the
next slice out of `raw` and each `uncompressed_size` advances the output. A sub-chunk whose two
sizes are equal is copied verbatim; otherwise it is decoded as one independent zstd frame. This
removes the streaming heuristic's assumption that the data section is exactly concatenated frames,
so a stored sub-chunk sitting between two compressed ones is handled correctly. */
pub fn decompress_zstd_multi_with_toc(
    raw: &[u8],
    uncompressed_size: usize,
    subchunks: &[WadSubchunk],
) -> Result<Vec<u8>> {
    let mut out = vec![0u8; uncompressed_size];
    let mut raw_offset = 0usize;
    let mut out_offset = 0usize;

    for sub in subchunks {
        let csize = sub.compressed_size as usize;
        let usize_ = sub.uncompressed_size as usize;

        let raw_end = raw_offset
            .checked_add(csize)
            .filter(|&end| end <= raw.len())
            .ok_or_else(|| {
                Error::Decompress(String::from("subchunk compressed range exceeds chunk"))
            })?;
        let out_end = out_offset
            .checked_add(usize_)
            .filter(|&end| end <= out.len())
            .ok_or_else(|| {
                Error::Decompress(String::from("subchunk uncompressed range exceeds chunk"))
            })?;

        let src = &raw[raw_offset..raw_end];
        if csize == usize_ {
            out[out_offset..out_end].copy_from_slice(src);
        } else {
            zstd::Decoder::new(Cursor::new(src))
                .and_then(|mut d| d.read_exact(&mut out[out_offset..out_end]))
                .map_err(|e| Error::Decompress(e.to_string()))?;
        }

        raw_offset = raw_end;
        out_offset = out_end;
    }

    if out_offset != out.len() {
        return Err(Error::Decompress(String::from(
            "subchunk table did not cover the whole chunk",
        )));
    }
    Ok(out)
}

fn decompress_zstd_multi(raw: &[u8], uncompressed_size: usize) -> Result<Vec<u8>> {
    let magic_offset = raw
        .windows(ZSTD_MAGIC.len())
        .position(|w| w == ZSTD_MAGIC)
        .ok_or_else(|| Error::Decompress(String::from("no zstd frame in multi chunk")))?;

    let mut out = vec![0u8; uncompressed_size];
    if magic_offset > out.len() || magic_offset > raw.len() {
        return Err(Error::Decompress(String::from(
            "zstd frame offset out of range",
        )));
    }
    out[..magic_offset].copy_from_slice(&raw[..magic_offset]);
    zstd::Decoder::new(Cursor::new(&raw[magic_offset..]))
        .and_then(|mut d| d.read_exact(&mut out[magic_offset..]))
        .map_err(|e| Error::Decompress(e.to_string()))?;
    Ok(out)
}
