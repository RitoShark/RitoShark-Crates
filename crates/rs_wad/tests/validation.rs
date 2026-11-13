use std::io::Cursor;

use rs_io::{Parse, Serialize};
use rs_wad::{Wad, WadBuilder, WadCompression, WadSubchunk, compress, decompress};

fn archive() -> Vec<u8> {
    WadBuilder::new()
        .with_chunk("test.bin")
        .build_to_bytes(|_, out| {
            out.write_all(b"payload").map_err(rs_io::Error::from)?;
            Ok(())
        })
        .unwrap()
}

#[test]
fn changing_provider_with_equal_compressed_lengths_is_rejected() {
    assert_eq!(
        compress(b"A", WadCompression::Zstd, 3).unwrap().len(),
        compress(b"B", WadCompression::Zstd, 3).unwrap().len()
    );
    let mut calls = 0;
    let result = WadBuilder::new()
        .with_chunk("test.bin")
        .build_to_bytes(|_, out| {
            calls += 1;
            out.write_all(if calls == 1 { b"A" } else { b"B" })
                .map_err(rs_io::Error::from)?;
            Ok(())
        });
    assert!(result.is_err());
}

#[test]
fn toc_only_save_fails_before_writing_but_streaming_reads_work() {
    let bytes = archive();
    let mut reader = Cursor::new(&bytes);
    let toc = Wad::from_reader_toc(&mut reader).unwrap();
    assert_eq!(
        toc.chunk_data_from(&mut reader, &toc.chunks[0]).unwrap(),
        b"payload"
    );
    let mut output = Vec::new();
    assert!(toc.to_writer(&mut output).is_err());
    assert!(output.is_empty());
    let full = Wad::from_bytes(&bytes).unwrap();
    assert_eq!(full.to_bytes().unwrap(), bytes);
}

#[test]
fn empty_archive_can_still_be_written() {
    let bytes = WadBuilder::new()
        .build_to_bytes(|_, _| unreachable!())
        .unwrap();
    assert_eq!(Wad::from_bytes(&bytes).unwrap().to_bytes().unwrap(), bytes);
}

#[test]
fn out_of_bounds_payload_is_rejected_before_writing() {
    let mut wad = Wad::from_bytes(&archive()).unwrap();
    wad.chunks[0].data_offset = u32::MAX;
    let mut output = Vec::new();
    assert!(wad.to_writer(&mut output).is_err());
    assert!(output.is_empty());
}

#[test]
fn decoded_size_must_match_for_every_supported_compression() {
    for compression in [
        WadCompression::None,
        WadCompression::Gzip,
        WadCompression::Zstd,
    ] {
        for payload in [b"ABC".as_slice(), b""] {
            let raw = compress(payload, compression, 3).unwrap();
            assert_eq!(
                decompress(&raw, compression, payload.len()).unwrap(),
                payload
            );
            assert!(decompress(&raw, compression, payload.len() + 1).is_err());
            if !payload.is_empty() {
                assert!(decompress(&raw, compression, 1).is_err());
                assert!(decompress(&raw, compression, 0).is_err());
            }
        }
    }
}

#[test]
fn multi_frame_and_prefix_sizes_are_checked() {
    let mut raw = b"PREFIX".to_vec();
    raw.extend(compress(b"ABC", WadCompression::Zstd, 3).unwrap());
    raw.extend(compress(b"DEF", WadCompression::Zstd, 3).unwrap());
    assert_eq!(
        decompress(&raw, WadCompression::ZstdMulti, 12).unwrap(),
        b"PREFIXABCDEF"
    );
    for size in [0, 5, 6, 8, 9, 11, 13] {
        assert!(decompress(&raw, WadCompression::ZstdMulti, size).is_err());
    }
}

#[test]
fn subchunk_table_must_cover_input_and_match_decoded_sizes() {
    let raw = compress(b"ABC", WadCompression::Zstd, 3).unwrap();
    for size in [0, 1, 2, 4] {
        let toc = [WadSubchunk {
            compressed_size: raw.len() as u32,
            uncompressed_size: size,
            checksum: 0,
        }];
        assert!(rs_wad::decompress_zstd_multi_with_toc(&raw, size as usize, &toc).is_err());
    }
    let toc = [WadSubchunk {
        compressed_size: raw.len() as u32,
        uncompressed_size: 3,
        checksum: 0,
    }];
    let mut trailing = raw.clone();
    trailing.push(0);
    assert!(rs_wad::decompress_zstd_multi_with_toc(&trailing, 3, &toc).is_err());
    assert_eq!(
        rs_wad::decompress_zstd_multi_with_toc(&raw, 3, &toc).unwrap(),
        b"ABC"
    );
}

#[test]
fn hostile_declared_sizes_fail_without_preallocating_them() {
    let raw = compress(b"ABC", WadCompression::Zstd, 3).unwrap();
    assert!(decompress(&raw, WadCompression::Zstd, usize::MAX).is_err());
    assert!(decompress(&raw, WadCompression::Zstd, u32::MAX as usize).is_err());
}

#[test]
fn gzip_trailer_is_validated_after_the_expected_output() {
    let mut raw = compress(b"ABC", WadCompression::Gzip, 3).unwrap();
    let crc_offset = raw.len() - 8;
    raw[crc_offset] ^= 1;
    assert!(decompress(&raw, WadCompression::Gzip, 3).is_err());
}

#[test]
fn concatenated_gzip_members_cannot_hide_excess_output() {
    let mut raw = compress(b"ABC", WadCompression::Gzip, 3).unwrap();
    raw.extend(compress(b"DEF", WadCompression::Gzip, 3).unwrap());
    assert!(decompress(&raw, WadCompression::Gzip, 3).is_err());
    assert_eq!(
        decompress(&raw, WadCompression::Gzip, 6).unwrap(),
        b"ABCDEF"
    );
}
