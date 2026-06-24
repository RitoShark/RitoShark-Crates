#![forbid(unsafe_code)]
/*!
Resolves and loads CDTB hash dictionaries for name resolution. The lookup order is the
explicit `--hashes` value, then the `RITOSHARK_HASHES` environment variable, then a `hashes`
directory beside the executable. A directory loads the conventional CDTB file set; a single
file loads just that dictionary. Loading is best-effort — missing files leave hashes raw.
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

/// Resolve the hash source from the flag, the `RITOSHARK_HASHES` env var, or a `hashes`
/// directory next to the running executable, in that order.
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
            let candidate = dir.join("hashes");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Load a mapper from a resolved path. A directory merges the conventional CDTB files that
/// exist; a file loads that single dictionary. Returns an empty mapper if nothing is found.
pub fn load(flag: Option<&Path>) -> HashMapper {
    let mut mapper = HashMapper::new();
    let Some(path) = resolve(flag) else {
        return mapper;
    };
    if path.is_dir() {
        for name in CDTB_FILES {
            merge_file(&mut mapper, &path.join(name));
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
