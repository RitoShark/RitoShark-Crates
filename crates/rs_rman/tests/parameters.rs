use std::path::PathBuf;

use rs_io::Parse;
use rs_rman::{ChunkHashType, Rman};

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
fn parameters_table_present_and_resolvable() {
    let mut tested = 0;
    for name in MANIFESTS {
        let Some(path) = sample(name) else {
            eprintln!("skip {name}: not present");
            continue;
        };
        tested += 1;
        let rman = Rman::from_path(&path).expect("parse");

        assert!(
            !rman.parameters.is_empty(),
            "{name}: parameters table empty"
        );

        // Every WAD file with a param_index resolves to an in-range entry whose
        // hash_type is a known variant.
        let mut resolved = 0;
        for file in rman.files.iter().filter(|f| f.param_index.is_some()) {
            let ht = rman.file_hash_type(file);
            assert!(
                ht.is_some(),
                "{name}: {:?} param_index out of range",
                file.name
            );
            resolved += 1;
        }
        assert!(resolved > 0, "{name}: no files carried a param_index");

        // Live LoL manifests use SHA256 today; assert at least one resolves to a
        // concrete algorithm (guards against an all-None silent failure).
        let any_concrete = rman.parameters.iter().any(|p| {
            matches!(
                p.hash_type,
                Some(ChunkHashType::Sha256 | ChunkHashType::Hkdf | ChunkHashType::Blake3)
            )
        });
        assert!(any_concrete, "{name}: no concrete hash type in parameters");

        eprintln!(
            "{name}: {} parameters, {resolved} files resolved",
            rman.parameters.len()
        );
        for p in &rman.parameters {
            eprintln!(
                "  param: {:?} min={} max={}",
                p.hash_type, p.min_chunk_size, p.max_chunk_size
            );
        }
    }
    if tested == 0 {
        eprintln!("no sample manifests present; nothing to verify");
    }
}
