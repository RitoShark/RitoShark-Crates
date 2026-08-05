/*!
The manifest file-size field is 64 bits wide. FlatBuffers packs a table's eight-byte fields together
and aligns them, so in a real file entry the size slot sits at offset 32, exactly eight bytes after
the directory id at 24 and eight before the locale flags at 40. Reading it as a `u32` returns the
correct number for every file below four gibibytes and silently wraps above, which is why the
narrower read went unnoticed: League's largest shipped file is already 3.11 GiB, inside the range
where both widths agree.
*/

use std::path::PathBuf;

use rs_io::Parse;
use rs_rman::{FileEntry, Rman};

fn sample(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sample-files")
        .join(name);
    path.is_file().then_some(path)
}

const MANIFESTS: &[&str] = &[
    "7D6C65378829C6AA.manifest",
    "DAFB5FDD5647079F.manifest",
    "F8FBA48750270222.manifest",
];

/// Narrowing the field back to `u32` breaks this binding at compile time.
fn assert_size_is_64_bit(entry: &FileEntry) -> u64 {
    let size: u64 = entry.size;
    size
}

#[test]
fn file_sizes_are_read_as_64_bit_values() {
    let mut checked = 0;
    let mut largest = 0u64;

    for name in MANIFESTS {
        let Some(path) = sample(name) else {
            eprintln!("missing sample {name}; skipping");
            continue;
        };
        let manifest = Rman::from_path(&path).expect("parse manifest");
        assert!(!manifest.files.is_empty(), "{name}: no file entries");

        for entry in &manifest.files {
            largest = largest.max(assert_size_is_64_bit(entry));
        }
        checked += 1;
    }

    if checked == 0 {
        eprintln!("no manifest fixtures present; nothing verified");
        return;
    }

    /* League already ships a file within one gibibyte of the 32-bit ceiling, so the narrower read
    is not a hypothetical risk: the next size increase of this asset wraps it. */
    assert!(
        largest > u64::from(u32::MAX) / 2,
        "expected a fixture file above 2 GiB, largest was {largest}"
    );
    eprintln!("largest file across {checked} manifests: {largest} bytes");
}

/// The one entry that pins the exact value, so a future change to the field offset or width that
/// happened to stay under 4 GiB would still be caught.
#[test]
fn the_largest_known_file_parses_exactly() {
    let Some(path) = sample("F8FBA48750270222.manifest") else {
        eprintln!("missing sample manifest; skipping");
        return;
    };
    let manifest = Rman::from_path(&path).expect("parse manifest");
    let largest = manifest
        .files
        .iter()
        .map(|entry| entry.size)
        .max()
        .expect("at least one file");

    assert_eq!(largest, 3_337_364_776, "largest file size changed");
}
