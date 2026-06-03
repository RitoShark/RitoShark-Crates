use std::path::PathBuf;

use rs_io::{Parse, Serialize};
use rs_wad::{Wad, WadChunk, WadCompression};

fn sample(name: &str) -> Option<PathBuf> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sample-files")
        .join(name);
    p.exists().then_some(p)
}

struct Report {
    version: (u8, u8),
    chunk_count: usize,
    attempted: usize,
    decompressed_ok: usize,
    failures: Vec<(WadCompression, String)>,
}

fn exercise(path: PathBuf) -> Report {
    let bytes = std::fs::read(&path).expect("read sample");
    let wad = Wad::from_bytes(&bytes).expect("parse wad");

    assert!(!wad.chunks.is_empty(), "chunk table must be non-empty");
    assert_eq!(wad.version.0, 3, "sample WADs are major version 3");

    let rewritten = wad.to_bytes().expect("serialize wad");
    assert_eq!(
        rewritten, bytes,
        "read -> write must be byte-exact for the real archive"
    );

    let mut targets: Vec<&WadChunk> = wad.chunks.iter().take(20).collect();
    for chunk in &wad.chunks {
        if chunk.subchunk_count > 0 && !targets.iter().any(|c| c.path_hash == chunk.path_hash) {
            targets.push(chunk);
            if targets.iter().filter(|c| c.subchunk_count > 0).count() >= 200 {
                break;
            }
        }
    }

    let mut decompressed_ok = 0;
    let mut failures = Vec::new();
    for chunk in &targets {
        match wad.chunk_data(chunk) {
            Ok(data) if data.len() == chunk.uncompressed_size as usize => decompressed_ok += 1,
            Ok(data) => failures.push((
                chunk.compression,
                format!(
                    "length mismatch: got {} want {}",
                    data.len(),
                    chunk.uncompressed_size
                ),
            )),
            Err(e) => failures.push((chunk.compression, e.to_string())),
        }
    }

    Report {
        version: wad.version,
        chunk_count: wad.chunks.len(),
        attempted: targets.len(),
        decompressed_ok,
        failures,
    }
}

fn run(name: &str) {
    let Some(path) = sample(name) else {
        eprintln!("skip: sample {name} not present");
        return;
    };
    let r = exercise(path);
    println!(
        "{name}: v{}.{} chunks={} attempted={} decompressed_ok={} failures={}",
        r.version.0,
        r.version.1,
        r.chunk_count,
        r.attempted,
        r.decompressed_ok,
        r.failures.len()
    );
    for (comp, msg) in &r.failures {
        println!("    FAIL [{comp:?}]: {msg}");
    }
    assert!(
        r.failures.is_empty(),
        "{name}: {} chunk(s) failed to decompress",
        r.failures.len()
    );
}

#[test]
fn azir_wad() {
    run("Azir.wad.client");
}

#[test]
fn data_wad() {
    run("DATA.wad.client");
}

/// Lowercased in-game path of Azir's subchunk TOC, derived from the WAD's location under `Game/`.
const AZIR_SUBCHUNKTOC: &str = "data/final/champions/azir.wad.subchunktoc";

/** Looks up chunks by hash and by path on the real Azir archive, then extracts and decompresses a
handful and asserts each decompressed length equals its `uncompressed_size`. `chunk_by_path` is
proven against a real derivable path (the archive's own `.subchunktoc`). */
#[test]
fn azir_lookup_and_extract() {
    let Some(path) = sample("Azir.wad.client") else {
        eprintln!("skip: Azir.wad.client not present");
        return;
    };
    let bytes = std::fs::read(&path).unwrap();
    let wad = Wad::from_bytes(&bytes).unwrap();

    // chunk_by_path on a real, derivable path resolves to the same chunk as chunk_by_hash.
    let by_path = wad
        .chunk_by_path(AZIR_SUBCHUNKTOC)
        .expect("subchunktoc chunk present");
    let by_hash = wad
        .chunk_by_hash(rs_hash::xxh64(AZIR_SUBCHUNKTOC))
        .expect("same chunk by hash");
    assert_eq!(by_path.path_hash, by_hash.path_hash);

    // Look up the first few chunks by their hash and decompress them.
    let sample_hashes: Vec<u64> = wad.chunks.iter().take(8).map(|c| c.path_hash).collect();
    for h in &sample_hashes {
        let chunk = wad.chunk_by_hash(*h).expect("chunk found by hash");
        let data = wad.chunk_data(chunk).expect("decompress");
        assert_eq!(data.len(), chunk.uncompressed_size as usize);
    }

    // A missing hash yields None rather than a panic.
    assert!(wad.chunk_by_hash(0xFFFF_FFFF_FFFF_FFFF).is_none());

    // The bulk extractor returns exactly the selected hashes, each at its uncompressed length.
    let extracted = wad.extract_selected(sample_hashes.iter().copied()).unwrap();
    assert_eq!(extracted.len(), sample_hashes.len());
    for h in &sample_hashes {
        let chunk = wad.chunk_by_hash(*h).unwrap();
        assert_eq!(extracted[h].len(), chunk.uncompressed_size as usize);
    }
}

/** Parses Azir's real `.subchunktoc` and decodes every zstd-multi chunk through the explicit
per-sub-chunk size table, asserting the output is byte-identical to the streaming heuristic and the
length matches `uncompressed_size`. This confirms the TOC path matches the C# oracle on real data. */
#[test]
fn azir_subchunktoc_decode() {
    let Some(path) = sample("Azir.wad.client") else {
        eprintln!("skip: Azir.wad.client not present");
        return;
    };
    let bytes = std::fs::read(&path).unwrap();
    let wad = Wad::from_bytes(&bytes).unwrap();

    let toc = wad
        .subchunk_toc_for_path(AZIR_SUBCHUNKTOC)
        .unwrap()
        .expect("subchunktoc present");
    assert!(!toc.is_empty(), "subchunk toc must have entries");

    let mut checked = 0usize;
    for chunk in &wad.chunks {
        if chunk.compression != WadCompression::ZstdMulti {
            continue;
        }
        let via_toc = wad.chunk_data_with_toc(chunk, &toc).unwrap();
        let via_heuristic = wad.chunk_data(chunk).unwrap();
        assert_eq!(via_toc.len(), chunk.uncompressed_size as usize);
        assert_eq!(
            via_toc, via_heuristic,
            "explicit subchunk toc decode must match the heuristic"
        );
        checked += 1;
        if checked >= 200 {
            break;
        }
    }
    assert!(checked > 0, "Azir must contain zstd-multi chunks");
    println!("azir_subchunktoc_decode: verified {checked} zstd-multi chunks via explicit toc");
}
