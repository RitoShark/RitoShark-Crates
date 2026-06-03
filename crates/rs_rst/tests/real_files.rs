use std::path::{Path, PathBuf};

use rs_io::{Parse, Serialize};
use rs_rst::Rst;

fn sample_path(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sample-files")
        .join(name);
    path.exists().then_some(path)
}

fn header(bytes: &[u8]) -> (String, u8) {
    let magic = String::from_utf8_lossy(&bytes[..3]).into_owned();
    (magic, bytes[3])
}

fn check_file(name: &str) {
    let Some(path) = sample_path(name) else {
        eprintln!("skip {name}: sample file not present");
        return;
    };

    let bytes = std::fs::read(&path).expect("read sample bytes");
    let (magic, version) = header(&bytes);
    println!("{name}: magic={magic:?} version={version}");

    let rst = Rst::from_path(&path).expect("parse real RST");
    assert_eq!(rst.version, version);
    assert!(
        !rst.entries.is_empty(),
        "{name}: expected at least one entry"
    );
    println!("{name}: {} entries", rst.entries.len());

    let rewritten = rst.to_bytes().expect("re-serialize");
    assert_eq!(
        bytes.len(),
        rewritten.len(),
        "{name}: byte length differs ({} vs {})",
        bytes.len(),
        rewritten.len()
    );
    assert!(bytes == rewritten, "{name}: byte-exact round-trip failed");
    println!("{name}: byte-exact round-trip OK");

    let mut non_empty = 0usize;
    for (hash, _) in rst.entries.iter().take(16) {
        if let Some(s) = rst.get_by_hash(*hash) {
            if !s.is_empty() {
                non_empty += 1;
            }
        }
    }
    assert!(
        non_empty > 0,
        "{name}: no non-empty strings via get_by_hash"
    );
    println!("{name}: {non_empty}/16 sampled lookups non-empty");
}

#[test]
fn bootstrap_stringtable() {
    check_file("bootstrap.stringtable");
}

#[test]
fn lol_stringtable() {
    check_file("lol.stringtable");
}
