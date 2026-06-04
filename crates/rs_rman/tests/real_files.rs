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

        verify_flags(name, &rman);
        verify_file_chunks(name, &rman);
    }

    if tested == 0 {
        eprintln!("no sample manifests present; nothing to verify");
    }
}

/// The flags table parses (when present), flag ids are within mask range, every flag name
/// is non-empty, and files that carry a mask resolve to known flag names.
fn verify_flags(name: &str, rman: &Rman) {
    eprintln!("  flags: {} entries", rman.file_flags.len());
    for flag in &rman.file_flags {
        assert!(!flag.name.is_empty(), "{name}: empty flag name");
        assert!(
            flag.id < 64,
            "{name}: flag id {} out of mask range",
            flag.id
        );
    }
    let names: Vec<&str> = rman.file_flags.iter().map(|f| f.name.as_str()).collect();
    eprintln!("    {names:?}");

    if rman.file_flags.is_empty() {
        return;
    }

    // Pick the first file that carries a flag mask and show its resolved tags.
    if let Some(file) = rman.files.iter().find(|f| f.flags_mask.is_some()) {
        let tags = rman.file_flag_names(file);
        eprintln!("  file {:?} flags: {tags:?}", file.name);
    }

    // The first flag's name must select at least the files whose mask has that bit.
    let first = &rman.file_flags[0];
    let bit = 1u64 << first.id;
    let expected = rman
        .files
        .iter()
        .filter(|f| f.flags_mask.is_some_and(|m| m & bit != 0))
        .count();
    let got = rman.files_with_flag(&first.name).len();
    assert_eq!(
        expected, got,
        "{name}: files_with_flag({}) count",
        first.name
    );
}

/// `file_chunks` returns the file's chunks in order, every range maps to a real bundle, the
/// compressed ranges within each bundle are contiguous (this chunk starts where the previous
/// one in the same bundle ended), and the uncompressed sizes sum to the declared file size.
fn verify_file_chunks(name: &str, rman: &Rman) {
    use std::collections::HashMap;

    let index = rman.chunk_index();

    // Per-bundle ordered chunk start offsets, to assert contiguity within a bundle.
    let mut bundle_offsets: HashMap<u64, Vec<(u32, u32)>> = HashMap::new();
    for bundle in &rman.bundles {
        let mut running = 0u32;
        let mut spans = Vec::with_capacity(bundle.chunks.len());
        for chunk in &bundle.chunks {
            spans.push((running, chunk.compressed_size));
            running = running.saturating_add(chunk.compressed_size);
        }
        bundle_offsets.insert(bundle.id, spans);
    }

    let mut shown = 0usize;
    let mut files_checked = 0usize;

    for file in &rman.files {
        if file.chunk_ids.is_empty() {
            continue;
        }
        let ranges = Rman::file_chunks_for(file, &index);
        assert_eq!(
            ranges.len(),
            file.chunk_ids.len(),
            "{name}: file {:?} dropped chunks",
            file.name
        );

        // Order preserved and every range is a real, contiguous bundle chunk.
        let mut uncompressed_total: u64 = 0;
        for (range, &id) in ranges.iter().zip(&file.chunk_ids) {
            assert_eq!(range.chunk_id, id, "{name}: chunk order");
            let spans = &bundle_offsets[&range.bundle_id];
            assert!(
                spans.contains(&(range.offset_in_bundle, range.compressed_size)),
                "{name}: chunk {id:#x} range not contiguous in bundle {:#x}",
                range.bundle_id
            );
            uncompressed_total += range.uncompressed_size as u64;
        }

        assert_eq!(
            uncompressed_total, file.size as u64,
            "{name}: file {:?} uncompressed chunk sizes do not sum to file size",
            file.name
        );

        files_checked += 1;
        if shown < 2 {
            eprintln!(
                "  file {:?} ({} bytes): {} chunks across bundles; first chunk bundle {:#x} @ {} (+{} comp / {} uncomp)",
                file.name,
                file.size,
                ranges.len(),
                ranges[0].bundle_id,
                ranges[0].offset_in_bundle,
                ranges[0].compressed_size,
                ranges[0].uncompressed_size,
            );
            shown += 1;
        }
    }

    eprintln!("  file_chunks verified on {files_checked} files");
}
