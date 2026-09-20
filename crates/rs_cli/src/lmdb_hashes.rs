/*!
Loads RitoShark's combined LMDB hash dictionary into a [`HashMapper`].

The CDTB text dictionaries are six files totalling tens of millions of lines; the LMDB build
(`RitoShark/lmdb-hashes`, published as `lol-hashes-combined.zst`) packs the same data into one
environment with two named sub-databases:

| db    | key                    | value           | covers                         |
|-------|------------------------|-----------------|--------------------------------|
| `wad` | u64, big-endian        | UTF-8 path      | XXH64 WAD paths                |
| `bin` | u32, big-endian        | UTF-8 name      | FNV1a-32 bin fields/classes    |

Both are read whole into the mapper, which is keyed by `u64` with 32-bit hashes occupying the low
bits, so the two spaces coexist exactly as they do when the text dictionaries are merged.

This module is the only place in the CLI that needs `unsafe`: `heed` opens an environment by
memory-mapping it, and a mapped file that another process truncates is unsound in a way Rust
cannot check. Reading is otherwise ordinary and the environment is opened read-only.
*/

use std::path::Path;

use rs_hash::HashMapper;

/// A directory holding an LMDB environment, which is what `heed` opens.
pub fn is_lmdb_dir(path: &Path) -> bool {
    path.is_dir() && path.join("data.mdb").is_file()
}

/// Decode a big-endian key of either width. The `wad` db uses 8 bytes and `bin` uses 4; anything
/// else is a key this build does not know how to read, and is skipped rather than guessed at.
fn key_of(bytes: &[u8]) -> Option<u64> {
    match bytes.len() {
        8 => Some(u64::from_be_bytes(bytes.try_into().ok()?)),
        4 => Some(u32::from_be_bytes(bytes.try_into().ok()?) as u64),
        _ => None,
    }
}

/// Read every entry of one named sub-database into `mapper`, returning how many landed.
/// A missing database is not an error: an older or partial build may ship only one of the two.
fn drain_db(
    env: &heed::Env<heed::WithoutTls>,
    txn: &heed::RoTxn,
    name: &str,
    mapper: &mut HashMapper,
) -> heed::Result<usize> {
    type Raw = heed::types::Bytes;
    let Some(db) = env.open_database::<Raw, Raw>(txn, Some(name))? else {
        return Ok(0);
    };
    let mut count = 0;
    for entry in db.iter(txn)? {
        let (key, value) = entry?;
        let Some(hash) = key_of(key) else { continue };
        // Names are ASCII paths in practice; a lossy decode keeps one bad row from failing the load.
        mapper.insert(hash, String::from_utf8_lossy(value).into_owned());
        count += 1;
    }
    Ok(count)
}

/// Merge an LMDB hash environment into `mapper`. Returns the number of names loaded, or `None`
/// if the environment could not be opened at all.
pub fn merge(mapper: &mut HashMapper, path: &Path) -> Option<usize> {
    // The published DB is ~200 MB of data; the map size only reserves address space, so a
    // generous bound costs nothing and leaves room for the dictionary to keep growing.
    const MAP_SIZE: usize = 8 * 1024 * 1024 * 1024;

    // SAFETY: opening an LMDB environment maps the file. The contract we must uphold is that no
    // other process truncates or writes it underneath us; this is a read-only open of a hash
    // dictionary that is only ever replaced wholesale by a new download, never edited in place.
    #[allow(unsafe_code)]
    let env = unsafe {
        // `read_txn_without_tls` consumes the builder and changes its type parameter, so it has to
        // come before the by-reference setters rather than partway down the chain.
        heed::EnvOpenOptions::new()
            .read_txn_without_tls()
            .map_size(MAP_SIZE)
            .max_dbs(2)
            .flags(heed::EnvFlags::READ_ONLY | heed::EnvFlags::NO_LOCK)
            .open(path)
    }
    .ok()?;

    let txn = env.read_txn().ok()?;
    let mut total = 0;
    for name in ["wad", "bin"] {
        total += drain_db(&env, &txn, name, mapper).unwrap_or(0);
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_both_key_widths_big_endian() {
        assert_eq!(key_of(&[0, 0, 0, 0, 0, 0, 0, 1]), Some(1));
        assert_eq!(key_of(&[0x81, 0x1c, 0x9d, 0xc5]), Some(0x811c9dc5));
        assert_eq!(key_of(&[1, 2, 3]), None);
    }

    #[test]
    fn a_plain_directory_is_not_an_lmdb_env() {
        let dir = std::env::temp_dir().join("rs_cli_lmdb_probe");
        let _ = std::fs::create_dir_all(&dir);
        assert!(!is_lmdb_dir(&dir));
    }
}
