//! Network-gated: fetches one real chunk by range from Riot's CDN and validates it.
//! Skips when sample manifests are absent or when HEXTECH_NET_TESTS is unset.
//! Requires the `verify` feature.

#[cfg(not(feature = "verify"))]
fn main() {}

#[cfg(feature = "verify")]
use std::path::PathBuf;

#[cfg(feature = "verify")]
use rs_io::Parse;
#[cfg(feature = "verify")]
use rs_rman::{validate_chunk, Rman};

#[cfg(feature = "verify")]
fn sample(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sample-files")
        .join(name);
    if path.is_file() { Some(path) } else { None }
}

#[cfg(feature = "verify")]
#[test]
fn validates_one_real_chunk() {
    if std::env::var("HEXTECH_NET_TESTS").is_err() {
        eprintln!("skip: set HEXTECH_NET_TESTS=1 to run the networked chunk test");
        return;
    }
    let Some(path) = sample("7D6C65378829C6AA.manifest") else {
        eprintln!("skip: sample manifest absent");
        return;
    };
    let rman = Rman::from_path(&path).expect("parse");
    let index = rman.chunk_index();

    // First file that has both a resolvable hash type and at least one chunk.
    let (file, ht) = rman
        .files
        .iter()
        .find_map(|f| {
            let ht = rman.file_hash_type(f)?;
            if f.chunk_ids.is_empty() { None } else { Some((f, ht)) }
        })
        .expect("a validatable file");

    let range = &Rman::file_chunks_for(file, &index)[0];
    let url = format!(
        "https://lol.dyn.riotcdn.net/channels/public/bundles/{:016X}.bundle",
        range.bundle_id
    );
    let start = range.offset_in_bundle as u64;
    let end = start + range.compressed_size as u64 - 1;

    let resp = ureq::get(&url)
        .set("Range", &format!("bytes={start}-{end}"))
        .call()
        .expect("range request");
    let mut compressed = Vec::with_capacity(range.compressed_size as usize);
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut compressed).expect("read body");

    assert_eq!(compressed.len(), range.compressed_size as usize, "short read");
    let decompressed = zstd::stream::decode_all(&compressed[..]).expect("zstd decode");
    assert!(
        validate_chunk(&decompressed, range.chunk_id, ht).expect("supported hash type"),
        "chunk {:#x} failed {ht:?} validation",
        range.chunk_id
    );
    eprintln!("validated chunk {:#x} of {:?} as {ht:?}", range.chunk_id, file.name);
}
