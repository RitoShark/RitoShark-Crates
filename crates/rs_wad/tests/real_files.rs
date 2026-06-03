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
