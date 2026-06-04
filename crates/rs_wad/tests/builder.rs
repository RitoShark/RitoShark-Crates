use std::io::Write;

use rs_io::Parse;
use rs_wad::{Wad, WadBuilder, WadCompression};

/// Writes `data` into the provider's sink, mapping the io error into the wad error chain.
fn put(w: &mut dyn Write, data: &[u8]) -> rs_wad::Result<()> {
    w.write_all(data)
        .map_err(rs_io::Error::from)
        .map_err(rs_wad::Error::from)?;
    Ok(())
}

/// Builds a v3.4 WAD from the named files, then proves every chunk parses and decodes back to its
/// exact input bytes — the build → parse → decode round-trip that is the spec's correctness bar.
#[test]
fn builds_and_roundtrips_chunks() {
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("data/a.txt", b"hello world".to_vec()),
        ("data/big.bin", vec![0x42u8; 5000]),
        ("data/empty", Vec::new()),
    ];

    let mut builder = WadBuilder::new();
    for (path, _) in &files {
        builder.add_chunk(path);
    }
    let bytes = builder
        .build_to_bytes(|hash, w| {
            let (_, data) = files
                .iter()
                .find(|(p, _)| rs_hash::xxh64(p) == hash)
                .expect("provider asked for an unregistered hash");
            put(w, data)
        })
        .unwrap();

    let wad = Wad::from_bytes(&bytes).unwrap();
    assert_eq!(wad.version, (3, 4));
    assert_eq!(wad.chunks.len(), files.len());

    // TOC must be sorted ascending by path hash, or League refuses to mount the archive.
    assert!(wad.chunks.windows(2).all(|w| w[0].path_hash < w[1].path_hash));

    for (path, data) in &files {
        let chunk = wad.chunk_by_path(path).expect("chunk missing from built wad");
        assert_eq!(chunk.compression, WadCompression::Zstd);
        assert_eq!(chunk.uncompressed_size as usize, data.len());
        assert_eq!(wad.chunk_data(chunk).unwrap(), *data);
    }
}

/// Two chunks with identical contents must share one copy in the data section, with the later one
/// flagged `is_duplicated` and pointing at the earlier one's offset.
#[test]
fn identical_chunks_are_deduplicated() {
    let payload = vec![0xABu8; 4096];
    let mut builder = WadBuilder::new();
    builder.add_chunk("a/one.bin");
    builder.add_chunk("a/two.bin");

    let bytes = builder
        .build_to_bytes(|_hash, w| put(w, &payload))
        .unwrap();

    let wad = Wad::from_bytes(&bytes).unwrap();
    let one = *wad.chunk_by_path("a/one.bin").unwrap();
    let two = *wad.chunk_by_path("a/two.bin").unwrap();

    assert_eq!(one.data_offset, two.data_offset);
    assert_eq!(one.compressed_size, two.compressed_size);
    assert_eq!(one.checksum, two.checksum);
    assert!(one.is_duplicated ^ two.is_duplicated, "exactly one is the duplicate");
    assert_eq!(wad.chunk_data(&one).unwrap(), payload);
    assert_eq!(wad.chunk_data(&two).unwrap(), payload);
}

/// The builder-style `with_chunk` / `with_version` chain produces the same archive as the mutating
/// API, and the chunk checksum is the xxh3-64 of the stored compressed bytes (per v3.4).
#[test]
fn builder_style_and_checksum() {
    let data = b"checksum me please, the whole compressed frame".to_vec();
    let bytes = WadBuilder::new()
        .with_version(3, 4)
        .with_chunk("x/y.txt")
        .build_to_bytes(|_h, w| put(w, &data))
        .unwrap();

    let wad = Wad::from_bytes(&bytes).unwrap();
    let chunk = wad.chunk_by_path("x/y.txt").unwrap();
    let raw = wad.chunk_raw(chunk).unwrap();
    assert_eq!(chunk.checksum, rs_hash::xxh3_64_bytes(raw));
    assert_eq!(wad.chunk_data(chunk).unwrap(), data);
}
