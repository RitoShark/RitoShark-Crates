//! Exercises the parser against real `.mapgeo` sample files. The files live outside the crate in
//! a gitignored `sample-files/` directory, so every test skips cleanly when they are absent. For
//! each file we first read the `OEGM` magic and the on-disk version, then attempt a full parse:
//! version 17 must parse with a non-empty model list, every other version must be reported as
//! `Error::UnsupportedVersion` carrying that exact version (never a panic, never a different error).

use std::path::{Path, PathBuf};

use rs_io::{Parse, Serialize};
use rs_mapgeo::{Error, MapGeometry};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-files")
}

fn read_sample(name: &str) -> Option<Vec<u8>> {
    let path = sample_dir().join(name);
    std::fs::read(&path).ok()
}

fn magic_and_version(bytes: &[u8]) -> (&[u8], u32) {
    let magic = &bytes[..4];
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    (magic, version)
}

fn check_sample(name: &str) {
    let Some(bytes) = read_sample(name) else {
        eprintln!("skipping {name}: sample file not present");
        return;
    };

    assert!(bytes.len() >= 8, "{name}: too small to hold a header");
    let (magic, version) = magic_and_version(&bytes);
    assert_eq!(magic, b"OEGM", "{name}: unexpected magic {magic:?}");
    eprintln!("{name}: OEGM version {version}");

    match MapGeometry::from_bytes(&bytes) {
        Ok(geo) => {
            assert_eq!(version, 17, "{name}: only version 17 should parse");
            assert_eq!(geo.version, 17);
            assert!(
                !geo.models.is_empty(),
                "{name}: parsed but model list is empty"
            );
            eprintln!(
                "{name}: parsed v17 - {} models, {} vertex buffers, {} index buffers",
                geo.models.len(),
                geo.vertex_buffers.len(),
                geo.index_buffers.len(),
            );

            // The writer reproduces every field the reader consumed; because the reader stops after
            // the model list, the re-serialized bytes must equal the original file's prefix exactly.
            let out = geo.to_bytes().expect("re-serialize parsed v17");
            assert!(
                out.len() <= bytes.len(),
                "{name}: writer emitted more bytes than the source file"
            );
            if let Some(offset) = out.iter().zip(&bytes).position(|(a, b)| a != b) {
                panic!(
                    "{name}: re-serialized prefix diverges at byte {offset} \
                     (wrote {} bytes, source {} bytes)",
                    out.len(),
                    bytes.len()
                );
            }
            eprintln!(
                "{name}: byte-exact prefix round-trip over {} of {} bytes",
                out.len(),
                bytes.len()
            );
        }
        Err(Error::UnsupportedVersion(reported)) => {
            assert_ne!(version, 17, "{name}: version 17 must not be rejected");
            assert_eq!(
                reported, version,
                "{name}: UnsupportedVersion should carry the on-disk version"
            );
            eprintln!("{name}: correctly reported Unsupported(version {reported})");
        }
        Err(other) => panic!("{name}: unexpected error {other:?}"),
    }
}

#[test]
fn bloom_mapgeo() {
    check_sample("bloom.mapgeo");
}

#[test]
fn spectator_only_banners_mapgeo() {
    check_sample("spectator_only_banners.mapgeo");
}

#[test]
fn ultbook_mapgeo() {
    check_sample("ultbook.mapgeo");
}
