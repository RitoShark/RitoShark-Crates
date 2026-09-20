/*!
Resolves and loads hash dictionaries for name resolution. The lookup order is the explicit
`--hashes` value, then the `RITOSHARK_HASHES` environment variable, then a `hashes` or
`lol-hashes.lmdb` directory beside the executable, and finally the shared RitoShark dictionary in
app data - which is downloaded on first use if no tool has installed it yet (see `hash_fetch`).

The effect is that names resolve out of the box: none of these commands need a `--hashes` flag,
and an explicit one is only for pointing at a dictionary somewhere else.

Two dictionary formats are accepted, chosen by what the path actually is:

  * an LMDB environment (a directory containing `data.mdb`) - RitoShark's combined
    `lol-hashes.lmdb`, one file covering both the WAD and bin hash spaces;
  * CDTB text dictionaries - a directory of the conventional `hashes.*.txt` set, or a single
    such file.

Loading is best-effort: missing or unreadable dictionaries leave hashes raw rather than failing
the command that asked for names.
*/

use std::path::{Path, PathBuf};

use rs_hash::HashMapper;

const CDTB_FILES: &[&str] = &[
    "hashes.binentries.txt",
    "hashes.binhashes.txt",
    "hashes.bintypes.txt",
    "hashes.binfields.txt",
    "hashes.game.txt",
    "hashes.lcu.txt",
];

/// Names tried beside the executable when no dictionary is given, LMDB first: it is one
/// directory covering both hash spaces, so it is the better answer when both are present.
const NEARBY: &[&str] = &["lol-hashes.lmdb", "hashes.lmdb", "hashes"];

/// Resolve a dictionary the user pointed us at: the flag, the `RITOSHARK_HASHES` env var, or a
/// dictionary directory next to the running executable, in that order. Returns `None` when none
/// of those is set, leaving the shared dictionary to `load`.
pub fn resolve(flag: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = flag {
        return Some(p.to_path_buf());
    }
    if let Ok(env) = std::env::var("RITOSHARK_HASHES") {
        if !env.is_empty() {
            return Some(PathBuf::from(env));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in NEARBY {
                let candidate = dir.join(name);
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Load a mapper, fetching the shared dictionary on first use when nothing else is configured.
pub fn load(flag: Option<&Path>) -> HashMapper {
    load_with(flag, false)
}

/// As [`load`], but `force_refresh` re-downloads the shared dictionary even when one is present.
/// Only meaningful when no explicit dictionary was given: a path the user chose is theirs, and
/// this is not entitled to overwrite it.
pub fn load_with(flag: Option<&Path>, force_refresh: bool) -> HashMapper {
    let mut mapper = HashMapper::new();
    // Nothing configured: use the dictionary every RitoShark tool shares, downloading it once if
    // this machine has not got it yet. This is what makes names appear without a flag.
    let chosen = match resolve(flag) {
        Some(p) => Some(p),
        None => crate::hash_fetch::ensure(force_refresh),
    };
    let Some(path) = chosen else {
        return mapper;
    };
    if crate::lmdb_hashes::is_lmdb_dir(&path) {
        crate::lmdb_hashes::merge(&mut mapper, &path);
    } else if path.is_dir() {
        // A directory may ALSO be a parent holding the LMDB env (the layout the downloader
        // writes: `<dir>/lol-hashes.lmdb/data.mdb`), so look one level down before falling back
        // to the CDTB file set. Without this, pointing `--hashes` at the containing folder
        // silently resolves nothing.
        let nested = NEARBY
            .iter()
            .map(|n| path.join(n))
            .find(|p| crate::lmdb_hashes::is_lmdb_dir(p));
        if let Some(env) = nested {
            crate::lmdb_hashes::merge(&mut mapper, &env);
        } else {
            for name in CDTB_FILES {
                merge_file(&mut mapper, &path.join(name));
            }
        }
    } else {
        merge_file(&mut mapper, &path);
    }
    mapper
}

fn merge_file(mapper: &mut HashMapper, path: &Path) {
    if let Ok(file) = std::fs::File::open(path) {
        let _ = mapper.load_text(std::io::BufReader::new(file));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn loads_cdtb_dir_merging_files() {
        let dir = std::env::temp_dir().join("rs_cli_hashes_test");
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join("hashes.binfields.txt"), "811c9dc5 fieldName\n").unwrap();
        fs::write(
            dir.join("hashes.game.txt"),
            "0123456789abcdef Common/Path\n",
        )
        .unwrap();
        let mapper = load(Some(dir.as_path()));
        assert_eq!(mapper.get(0x811c9dc5), Some("fieldName"));
        assert_eq!(mapper.get(0x0123456789abcdef), Some("Common/Path"));
    }
}
