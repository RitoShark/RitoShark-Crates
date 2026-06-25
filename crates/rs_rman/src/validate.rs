//! Chunk hash validation. A chunk's manifest `id` is the truncated hash of its
//! decompressed bytes. `validate_chunk` takes already-decompressed bytes and recomputes
//! the hash for comparison — callers are expected to decompress before calling.

use crate::ChunkHashType;
use crate::error::{Error, Result};

/// SHA-256 of `data`, first 8 bytes as a little-endian `u64`.
pub(crate) fn hash_sha256(data: &[u8]) -> u64 {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(data);
    u64::from_le_bytes(digest[..8].try_into().expect("sha256 is 32 bytes"))
}

/// BLAKE3 of `data`, first 8 bytes as a little-endian `u64`.
pub(crate) fn hash_blake3(data: &[u8]) -> u64 {
    let hash = blake3::hash(data);
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("blake3 is 32 bytes"))
}

/*
 * Riot's custom HKDF chunk hash (see Morilli/ManifestDownloader rman.c::hash_hkdf):
 * PRK = SHA256(data), zero-padded to a 64-byte HMAC block key. With ipad/opad derived
 * from that key, expand over 32 blocks using a big-endian block counter, XOR-folding the
 * low 8 bytes of each block's HMAC output into an 8-byte accumulator. The accumulator,
 * read little-endian, is the chunk id.
 */
fn hash_hkdf(data: &[u8]) -> u64 {
    use sha2::{Digest, Sha256};

    let prk = Sha256::digest(data);
    let mut key = [0u8; 64];
    key[..32].copy_from_slice(&prk);

    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= key[i];
        opad[i] ^= key[i];
    }

    let mut acc = [0u8; 8];
    for counter in 1u32..=32 {
        let inner = {
            let mut h = Sha256::new();
            h.update(ipad);
            h.update(counter.to_be_bytes());
            h.finalize()
        };
        let block = {
            let mut h = Sha256::new();
            h.update(opad);
            h.update(inner);
            h.finalize()
        };
        for i in 0..8 {
            acc[i] ^= block[i];
        }
    }
    u64::from_le_bytes(acc)
}

/*
 * Chunk ids are hashes of decompressed chunk bytes (Morilli/ManifestDownloader rman.c).
 * Callers decompress each chunk for writing to disk anyway; passing decompressed bytes
 * here avoids a second zstd pass per chunk.
 */
/// Recompute the chunk hash of `decompressed` (a chunk's already-inflated bytes) under
/// `hash_type` and compare to `expected_id` (the chunk's manifest id). Chunk ids are
/// hashes of the DECOMPRESSED chunk bytes. `Err` only for unsupported algorithms (SHA512).
pub fn validate_chunk(
    decompressed: &[u8],
    expected_id: u64,
    hash_type: ChunkHashType,
) -> Result<bool> {
    let got = match hash_type {
        ChunkHashType::Sha256 => hash_sha256(decompressed),
        ChunkHashType::Blake3 => hash_blake3(decompressed),
        ChunkHashType::Hkdf => hash_hkdf(decompressed),
        ChunkHashType::Sha512 => return Err(Error::Unsupported("SHA512 chunk hash")),
    };
    Ok(got == expected_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChunkHashType;

    // Known vector: sha256("") first 8 bytes (LE) of e3b0c442... = 0x42c4b0e3.
    // The full first 8 bytes are e3 b0 c4 42 98 fc 1c 14 -> LE u64.
    #[test]
    fn sha256_empty_input_matches_known_u64() {
        let expected = u64::from_le_bytes([0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14]);
        assert_eq!(hash_sha256(b""), expected);
        assert!(validate_chunk(b"", expected, ChunkHashType::Sha256).unwrap());
        assert!(!validate_chunk(b"", expected ^ 1, ChunkHashType::Sha256).unwrap());
    }

    #[test]
    fn blake3_empty_input_matches_known_u64() {
        // blake3("") = af1349b9... first 8 bytes af 13 49 b9 f5 f9 a1 a6 -> LE u64.
        let expected = u64::from_le_bytes([0xaf, 0x13, 0x49, 0xb9, 0xf5, 0xf9, 0xa1, 0xa6]);
        assert_eq!(hash_blake3(b""), expected);
        assert!(validate_chunk(b"", expected, ChunkHashType::Blake3).unwrap());
    }

    #[test]
    fn sha512_is_unsupported_error() {
        assert!(validate_chunk(b"", 0, ChunkHashType::Sha512).is_err());
    }
}
