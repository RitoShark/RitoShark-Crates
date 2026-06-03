use std::path::PathBuf;

use rs_io::Parse;
use rs_rman::Rman;

/// Locate a sample `.manifest` under the workspace's flat `sample-files` directory.
/// Returns `None` (so the test skips) when the file is absent.
fn sample(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sample-files")
        .join(name);
    if path.is_file() { Some(path) } else { None }
}

const MANIFESTS: &[&str] = &[
    "7D6C65378829C6AA.manifest",
    "DAFB5FDD5647079F.manifest",
    "F8FBA48750270222.manifest",
];

#[test]
fn parses_real_manifests() {
    let mut tested = 0;
    for name in MANIFESTS {
        let Some(path) = sample(name) else {
            eprintln!("skip {name}: not present");
            continue;
        };
        tested += 1;

        let rman = match Rman::from_path(&path) {
            Ok(r) => r,
            Err(e) => panic!("{name}: parse failed: {e}"),
        };

        assert_eq!(rman.version.0, 2, "{name}: major version");
        assert!(!rman.bundles.is_empty(), "{name}: no bundles");
        assert!(!rman.files.is_empty(), "{name}: no files");
        assert!(!rman.directories.is_empty(), "{name}: no directories");

        let chunk_total: usize = rman.bundles.iter().map(|b| b.chunks.len()).sum();

        let paths = rman.file_paths();
        assert_eq!(paths.len(), rman.files.len(), "{name}: path count");
        assert!(
            paths.iter().all(|(p, _)| !p.is_empty()),
            "{name}: empty path produced"
        );

        eprintln!(
            "{name}: version {:?} flags {:#06x} id {:#018x}",
            rman.version, rman.flags, rman.manifest_id
        );
        eprintln!(
            "  bundles {}  chunks {}  files {}  directories {}",
            rman.bundles.len(),
            chunk_total,
            rman.files.len(),
            rman.directories.len()
        );
        for (p, size) in paths.iter().take(3) {
            eprintln!("  sample: {p}  ({size} bytes)");
        }
    }

    if tested == 0 {
        eprintln!("no sample manifests present; nothing to verify");
    }
}
