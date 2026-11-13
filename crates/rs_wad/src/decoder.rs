use std::io::{Cursor, Read};

use crate::chunk::{WadCompression, WadSubchunk};
use crate::error::{Error, Result};

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/** Decompresses one chunk's raw bytes according to its [`WadCompression`]. `None` copies the
input; `Gzip` inflates with flate2; `Zstd` decodes zstd frames. `ZstdMulti` holds one zstd
frame per subchunk concatenated end to end (optionally after a stored prefix); the decoder copies
any bytes before the first frame verbatim and then streams the concatenated frames in order, which
the zstd reader decodes one after another until the input is exhausted. The decoded length must
equal `uncompressed_size`; decoding stops after at most one excess byte and does not preallocate
the declared size. `Satellite` is unsupported and returns
[`Error::UnsupportedCompression`]. */
pub fn decompress(
    raw: &[u8],
    compression: WadCompression,
    uncompressed_size: usize,
) -> Result<Vec<u8>> {
    match compression {
        WadCompression::None => {
            check_size(raw.len(), uncompressed_size)?;
            Ok(raw.to_vec())
        }
        WadCompression::Gzip => decompress_gzip(raw, uncompressed_size),
        WadCompression::Satellite => Err(Error::UnsupportedCompression(
            WadCompression::Satellite as u8,
        )),
        WadCompression::Zstd => decompress_zstd(raw, uncompressed_size),
        WadCompression::ZstdMulti => decompress_zstd_multi(raw, uncompressed_size),
    }
}

fn decompress_gzip(raw: &[u8], uncompressed_size: usize) -> Result<Vec<u8>> {
    read_decoded(flate2::read::MultiGzDecoder::new(raw), uncompressed_size)
}

fn decompress_zstd(raw: &[u8], uncompressed_size: usize) -> Result<Vec<u8>> {
    let decoder =
        zstd::Decoder::new(Cursor::new(raw)).map_err(|e| Error::Decompress(e.to_string()))?;
    read_decoded(decoder, uncompressed_size)
}

fn check_size(actual: usize, expected: usize) -> Result<()> {
    if actual != expected {
        return Err(Error::Decompress(format!(
            "decoded size mismatch: expected {expected} bytes, got {actual}"
        )));
    }
    Ok(())
}

fn read_decoded(reader: impl Read, expected: usize) -> Result<Vec<u8>> {
    let limit = u64::try_from(expected)
        .ok()
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| Error::Decompress(String::from("decoded size exceeds supported limit")))?;
    let mut out = Vec::new();
    reader
        .take(limit)
        .read_to_end(&mut out)
        .map_err(|e| Error::Decompress(e.to_string()))?;
    check_size(out.len(), expected)?;
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
    let mut out = Vec::new();
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
            .filter(|&end| end <= uncompressed_size)
            .ok_or_else(|| {
                Error::Decompress(String::from("subchunk uncompressed range exceeds chunk"))
            })?;

        let src = &raw[raw_offset..raw_end];
        if csize == usize_ {
            out.extend_from_slice(src);
        } else {
            out.extend_from_slice(&decompress_zstd(src, usize_)?);
        }

        raw_offset = raw_end;
        out_offset = out_end;
    }

    if out_offset != uncompressed_size || raw_offset != raw.len() {
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

    if magic_offset > uncompressed_size {
        return Err(Error::Decompress(String::from(
            "zstd frame offset out of range",
        )));
    }
    let mut out = raw[..magic_offset].to_vec();
    out.extend_from_slice(&decompress_zstd(
        &raw[magic_offset..],
        uncompressed_size - magic_offset,
    )?);
    Ok(out)
}
