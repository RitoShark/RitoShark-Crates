//! Exercises the parser against real `.mapgeo` sample files. The files live outside the crate in
//! a gitignored sample directory, so every test skips cleanly when they are absent. For each file
//! we first read the `OEGM` magic and the on-disk version, then attempt a full parse. Supported
//! versions (14, 17, 18) must parse with a non-empty model list and round-trip the *entire* file
//! byte-for-byte; any other version must be reported as `Error::UnsupportedVersion` carrying that
//! exact version (never a panic, never a different error).

use std::path::{Path, PathBuf};

use rs_io::{Parse, Serialize};
use rs_mapgeo::{Error, MapGeometry};

const SUPPORTED: &[u32] = &[14, 17, 18];

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Sample-Files")
}

fn read_sample(name: &str) -> Option<Vec<u8>> {
    std::fs::read(sample_dir().join(name)).ok()
}

fn on_disk_version(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
}

fn check_sample(name: &str) {
    let Some(bytes) = read_sample(name) else {
        eprintln!("skipping {name}: sample file not present");
        return;
    };

    assert!(bytes.len() >= 8, "{name}: too small to hold a header");
    assert_eq!(&bytes[..4], b"OEGM", "{name}: unexpected magic");
    let version = on_disk_version(&bytes);
    eprintln!("{name}: OEGM version {version}");

    match MapGeometry::from_bytes(&bytes) {
        Ok(geo) => {
            assert!(
                SUPPORTED.contains(&version),
                "{name}: parsed an unsupported version {version}"
            );
            assert_eq!(geo.version, version);
            assert!(
                !geo.models.is_empty(),
                "{name}: parsed but model list is empty"
            );
            eprintln!(
                "{name}: parsed v{version} - {} models, {} vertex buffers, {} index buffers, \
                 {} scene graphs, {} planar reflectors",
                geo.models.len(),
                geo.vertex_buffers.len(),
                geo.index_buffers.len(),
                geo.scene_graphs.len(),
                geo.planar_reflectors.len(),
            );

            let out = geo.to_bytes().expect("re-serialize parsed file");
            assert_eq!(
                out.len(),
                bytes.len(),
                "{name}: re-serialized length {} != source length {}",
                out.len(),
                bytes.len()
            );
            if let Some(offset) = out.iter().zip(&bytes).position(|(a, b)| a != b) {
                panic!("{name}: re-serialized output diverges at byte {offset}");
            }
            eprintln!(
                "{name}: byte-exact full round-trip over {} bytes",
                out.len()
            );
        }
        Err(Error::UnsupportedVersion(reported)) => {
            assert!(
                !SUPPORTED.contains(&version),
                "{name}: supported version {version} must not be rejected"
            );
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
