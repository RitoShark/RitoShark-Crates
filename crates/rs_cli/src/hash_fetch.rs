/*!
Finds RitoShark's combined LMDB hash dictionary, downloading it once if it is not installed.

The same dictionary is used by the Celestial launcher and by `pyritocrash`, and all three put it
in one place, so whichever installs it first serves the others and the ~280 MB payload is
downloaded once per machine rather than once per tool:

    %APPDATA%/com.divineskins.celestial/lmdb-hashes/lol-hashes.lmdb/data.mdb
    ~/.local/share/com.divineskins.celestial/... on Linux, ~/Library/Application Support/... on macOS

The release asset is a raw zstd-compressed `data.mdb` (`lol-hashes-combined.zst`), not an archive,
so installing it is: download, decompress, write, rename into place.

Download happens only when the dictionary is missing. This tool never checks GitHub for updates on
its own - the launcher does that on a 24h clock and writes the result into the same directory -
because a CLI that stalls on a network request before printing a WAD listing is a worse tool than
one with a slightly stale dictionary. `--refresh-hashes` forces a re-download when that matters.
*/

use std::io::Read;
use std::path::{Path, PathBuf};

/// The published dictionary: one zstd-compressed `data.mdb`.
const RELEASE_ZST: &str =
    "https://github.com/RitoShark/lmdb-hashes/releases/latest/download/lol-hashes-combined.zst";

/// Refuse a decompressed payload larger than this. A zstd stream can inflate enormously, and the
/// only thing we are willing to write here is a hash dictionary of a known rough size.
const MAX_INFLATED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

const USER_AGENT: &str = concat!("rs_cli/", env!("CARGO_PKG_VERSION"));

/// The platform's application-data root, before the app's own folder is appended.
///
/// Written as one function per platform rather than `cfg` attributes on a shared `let`: with the
/// latter, a target that is neither Windows nor Unix defines no binding at all and the function
/// stops compiling. Here every target gets a definition, and the fallback is a real answer rather
/// than a build error.
#[cfg(windows)]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
}

#[cfg(not(any(windows, unix)))]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// The shared app-data root the launcher uses, so a dictionary either tool downloads serves both.
fn app_data_root() -> Option<PathBuf> {
    platform_data_root().map(|b| b.join("com.divineskins.celestial"))
}

/// Where the dictionary lives when nothing points somewhere else.
pub fn default_dir() -> Option<PathBuf> {
    app_data_root().map(|d| d.join("lmdb-hashes").join("lol-hashes.lmdb"))
}

/// An installed dictionary is a directory holding `data.mdb`.
pub fn installed(dir: &Path) -> bool {
    dir.join("data.mdb").is_file()
}

/// Download and decompress the dictionary, returning the raw `data.mdb` bytes.
fn fetch() -> Result<Vec<u8>, String> {
    let response = ureq::get(RELEASE_ZST)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("Could not reach the hash dictionary release: {e}"))?;

    let mut compressed = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut compressed)
        .map_err(|e| format!("Could not download the hash dictionary: {e}"))?;

    // Bound the output rather than trusting the stream's own size hint.
    let mut decoder = zstd::stream::Decoder::new(compressed.as_slice())
        .map_err(|e| format!("Could not decompress the hash dictionary: {e}"))?;
    let mut out = Vec::new();
    decoder
        .by_ref()
        .take(MAX_INFLATED_BYTES)
        .read_to_end(&mut out)
        .map_err(|e| format!("Could not decompress the hash dictionary: {e}"))?;
    if out.len() as u64 >= MAX_INFLATED_BYTES {
        return Err(
            "The hash dictionary exceeds the decompression cap; refusing to write it".into(),
        );
    }
    Ok(out)
}

/// Write `bytes` as the directory's `data.mdb`, via a temporary file so an interrupted download
/// cannot leave a half-written dictionary that later opens as a corrupt environment.
fn install(dir: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let tmp = dir.join("data.mdb.new");
    std::fs::write(&tmp, bytes).map_err(|e| format!("Could not write the hash dictionary: {e}"))?;
    std::fs::rename(&tmp, dir.join("data.mdb"))
        .map_err(|e| format!("Could not install the hash dictionary: {e}"))
}

/// THE ENTRY POINT: the dictionary directory to read, fetching it first if it is absent.
///
/// Returns `None` when there is no dictionary and none could be fetched, which every caller
/// treats as "show raw hashes" rather than as a failure: resolving names is a convenience, and a
/// machine with no network should still be able to list a WAD.
pub fn ensure(force: bool) -> Option<PathBuf> {
    let dir = default_dir()?;
    if installed(&dir) && !force {
        return Some(dir);
    }
    eprintln!("Downloading the RitoShark hash dictionary (~280 MB, one time)...");
    match fetch().and_then(|bytes| install(&dir, &bytes)) {
        Ok(()) => {
            eprintln!("Hash dictionary installed at {}", dir.display());
            Some(dir)
        }
        Err(e) => {
            eprintln!("{e}");
            eprintln!("Continuing with raw hashes.");
            // A failed refresh must not discard a dictionary that is already there and usable.
            installed(&dir).then_some(dir)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Asserts the shape of the path, NOT that this machine has one: a build environment with
    // neither APPDATA nor HOME set is a legitimate `None`, and a test that fails there would be
    // testing the runner rather than the code.
    #[test]
    fn default_dir_ends_at_the_shared_lmdb_location() {
        let Some(dir) = default_dir() else { return };
        assert!(dir.ends_with("lol-hashes.lmdb"), "{}", dir.display());
        assert!(dir.parent().is_some_and(|p| p.ends_with("lmdb-hashes")));
    }

    #[test]
    fn an_empty_directory_is_not_installed() {
        let dir = std::env::temp_dir().join("rs_cli_hash_fetch_probe");
        let _ = std::fs::create_dir_all(&dir);
        assert!(!installed(&dir));
    }
}
