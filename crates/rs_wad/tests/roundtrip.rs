use std::io::{Cursor, Write};

use rs_io::{Parse, Serialize, WriterExt};
use rs_wad::{decompress, decompress_zstd_multi_with_toc, Wad, WadChunk, WadCompression, WadSubchunk};

const V3_TRAILER_LEN: usize = 256 + 8;

/// Builds a minimal v3.3 WAD: header + one uncompressed chunk holding `payload`.
fn build_v3_wad(path_hash: u64, payload: &[u8]) -> Vec<u8> {
    let data_start = 4 + V3_TRAILER_LEN + 4 + 32;

    let mut buf = Cursor::new(Vec::new());
    buf.write_u16(0x5752).unwrap();
    buf.write_u8(3).unwrap();
    buf.write_u8(3).unwrap();

    let mut trailer = vec![0u8; V3_TRAILER_LEN];
    for (i, b) in trailer.iter_mut().enumerate() {
        *b = (i % 7) as u8;
    }
    buf.write_all(&trailer).unwrap();

    buf.write_u32(1).unwrap();

    buf.write_u64(path_hash).unwrap();
    buf.write_u32(data_start as u32).unwrap();
    buf.write_u32(payload.len() as u32).unwrap();
    buf.write_u32(payload.len() as u32).unwrap();
    buf.write_u8(WadCompression::None as u8).unwrap();
    buf.write_u8(0).unwrap();
    buf.write_u16(0).unwrap();
    buf.write_u64(0xDEAD_BEEF_CAFE_F00D).unwrap();

    buf.write_all(payload).unwrap();

    buf.into_inner()
}

#[test]
fn parses_header_and_toc() {
    let bytes = build_v3_wad(0x0123_4567_89AB_CDEF, b"hello");
    let wad = Wad::from_bytes(&bytes).unwrap();

    assert_eq!(wad.version, (3, 3));
    assert_eq!(wad.chunks.len(), 1);

    let chunk = wad.chunks[0];
    assert_eq!(chunk.path_hash, 0x0123_4567_89AB_CDEF);
    assert_eq!(chunk.compression, WadCompression::None);
    assert_eq!(chunk.compressed_size, 5);
    assert_eq!(chunk.uncompressed_size, 5);
    assert_eq!(chunk.checksum, 0xDEAD_BEEF_CAFE_F00D);
    assert!(!chunk.is_duplicated);
}

#[test]
fn round_trip_is_byte_exact() {
    let bytes = build_v3_wad(0x0123_4567_89AB_CDEF, b"hello world");
    let wad = Wad::from_bytes(&bytes).unwrap();
    let out = wad.to_bytes().unwrap();
    assert_eq!(out, bytes);
}

#[test]
fn extracts_uncompressed_chunk() {
    let bytes = build_v3_wad(0x11, b"payload-bytes");
    let wad = Wad::from_bytes(&bytes).unwrap();
    let data = wad.chunk_data(&wad.chunks[0]).unwrap();
    assert_eq!(data, b"payload-bytes");
}

#[test]
fn rejects_bad_magic() {
    let bytes = vec![0x00u8; 64];
    assert!(Wad::from_bytes(&bytes).is_err());
}

#[test]
fn rejects_unsupported_version() {
    let mut bytes = vec![0u8; 64];
    bytes[0] = b'R';
    bytes[1] = b'W';
    bytes[2] = 9;
    bytes[3] = 0;
    assert!(Wad::from_bytes(&bytes).is_err());
}

#[test]
fn truncated_toc_errors_not_panics() {
    let mut bytes = build_v3_wad(0x1, b"x");
    bytes.truncate(4 + V3_TRAILER_LEN + 4 + 8);
    assert!(Wad::from_bytes(&bytes).is_err());
}

#[test]
fn decompress_none_copies() {
    let raw = b"verbatim";
    let out = decompress(raw, WadCompression::None, raw.len()).unwrap();
    assert_eq!(out, raw);
}

#[test]
fn decompress_gzip_roundtrip() {
    let original = b"the quick brown fox jumps over the lazy dog";
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(original).unwrap();
    let compressed = encoder.finish().unwrap();

    let out = decompress(&compressed, WadCompression::Gzip, original.len()).unwrap();
    assert_eq!(out, original);
}

#[test]
fn decompress_zstd_roundtrip() {
    let original = b"the quick brown fox jumps over the lazy dog";
    let compressed = zstd::encode_all(&original[..], 3).unwrap();

    let out = decompress(&compressed, WadCompression::Zstd, original.len()).unwrap();
    assert_eq!(out, original);
}

#[test]
fn decompress_zstd_multi_with_raw_prefix() {
    let prefix = b"RAWHEAD!";
    let tail = b"compressed-tail-section-data";
    let mut compressed = prefix.to_vec();
    compressed.extend_from_slice(&zstd::encode_all(&tail[..], 3).unwrap());

    let total = prefix.len() + tail.len();
    let out = decompress(&compressed, WadCompression::ZstdMulti, total).unwrap();

    let mut expected = prefix.to_vec();
    expected.extend_from_slice(tail);
    assert_eq!(out, expected);
}

#[test]
fn subchunk_toc_decode_mixed_stored_and_zstd() {
    let a = b"first-zstd-subchunk-payload".to_vec();
    let b = b"STORED-MIDDLE".to_vec();
    let c = b"third-zstd-subchunk-payload".to_vec();

    let a_z = zstd::encode_all(&a[..], 3).unwrap();
    let c_z = zstd::encode_all(&c[..], 3).unwrap();

    let mut raw = Vec::new();
    raw.extend_from_slice(&a_z);
    raw.extend_from_slice(&b);
    raw.extend_from_slice(&c_z);

    let toc = [
        WadSubchunk { compressed_size: a_z.len() as u32, uncompressed_size: a.len() as u32, checksum: 0 },
        WadSubchunk { compressed_size: b.len() as u32, uncompressed_size: b.len() as u32, checksum: 0 },
        WadSubchunk { compressed_size: c_z.len() as u32, uncompressed_size: c.len() as u32, checksum: 0 },
    ];

    let total = a.len() + b.len() + c.len();
    let out = decompress_zstd_multi_with_toc(&raw, total, &toc).unwrap();

    let mut expected = a.clone();
    expected.extend_from_slice(&b);
    expected.extend_from_slice(&c);
    assert_eq!(out, expected);
}

#[test]
fn satellite_is_unsupported() {
    assert!(decompress(b"\x00\x00", WadCompression::Satellite, 2).is_err());
}

#[test]
fn round_trip_v3_4_layout() {
    let data_start = 4 + V3_TRAILER_LEN + 4 + 32;
    let payload = b"v34";

    let mut buf = Cursor::new(Vec::new());
    buf.write_u16(0x5752).unwrap();
    buf.write_u8(3).unwrap();
    buf.write_u8(4).unwrap();
    buf.write_all(&vec![0u8; V3_TRAILER_LEN]).unwrap();
    buf.write_u32(1).unwrap();
    buf.write_u64(0x42).unwrap();
    buf.write_u32(data_start as u32).unwrap();
    buf.write_u32(payload.len() as u32).unwrap();
    buf.write_u32(payload.len() as u32).unwrap();
    buf.write_u8(WadCompression::None as u8).unwrap();
    /* 24-bit subchunk_start = 0 */
    buf.write_u8(0).unwrap();
    buf.write_u8(0).unwrap();
    buf.write_u8(0).unwrap();
    buf.write_u64(0).unwrap();
    buf.write_all(payload).unwrap();
    let bytes = buf.into_inner();

    let wad = Wad::from_bytes(&bytes).unwrap();
    assert_eq!(wad.version, (3, 4));
    assert!(!wad.chunks[0].is_duplicated);
    let out = wad.to_bytes().unwrap();
    assert_eq!(out, bytes);

    let _ = WadChunk::clone(&wad.chunks[0]);
}
