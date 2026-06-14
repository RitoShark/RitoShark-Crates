//! Chunk hash validation. A chunk's manifest `id` is the truncated hash of its
//! compressed bytes; these functions recompute it for comparison.

use crate::error::{Error, Result};
use crate::ChunkHashType;

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

/// Recompute `compressed`'s chunk hash under `hash_type` and compare to `expected_id`
/// (the chunk's manifest id). `Err` only for unsupported algorithms (e.g. SHA512).
pub fn validate_chunk(compressed: &[u8], expected_id: u64, hash_type: ChunkHashType) -> Result<bool> {
    let got = match hash_type {
        ChunkHashType::Sha256 => hash_sha256(compressed),
        ChunkHashType::Blake3 => hash_blake3(compressed),
        ChunkHashType::Hkdf => return Err(Error::Unsupported("HKDF (pending)")),
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
