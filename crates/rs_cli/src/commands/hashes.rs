#![forbid(unsafe_code)]
/*!
The `hashes` subcommands: report on the hash dictionary, install it, or force a re-download.

Every other subcommand installs the dictionary implicitly on first use, so these exist for the
cases where that is not enough: checking where it went and whether it loaded, warming it up before
going offline, and picking up a newer release without waiting for a tool that checks on a clock.
*/

use crate::error::Result;
use crate::{hash_fetch, hashes, lmdb_hashes};

/// Where the dictionary is, whether it is installed, and how many names it actually yields.
pub fn status() -> Result<()> {
    let configured = hashes::resolve(None);
    let shared = hash_fetch::default_dir();

    match (&configured, &shared) {
        (Some(p), _) => println!("Dictionary: {} (configured)", p.display()),
        (None, Some(p)) => println!("Dictionary: {} (shared)", p.display()),
        (None, None) => println!("Dictionary: no location could be resolved on this system"),
    }

    let path = configured.or(shared);
    let Some(path) = path else { return Ok(()) };

    if !hash_fetch::installed(&path) && !lmdb_hashes::is_lmdb_dir(&path) && !path.exists() {
        println!("Installed: no - run `rs_cli hashes install`, or just run any command");
        return Ok(());
    }

    // Load it for real rather than reporting on the file: a present-but-unreadable dictionary is
    // exactly the case a status command exists to catch.
    let mapper = hashes::load(Some(path.as_path()));
    if mapper.is_empty() {
        println!("Installed: present, but no names could be read from it");
    } else {
        println!("Installed: yes, {} names", mapper.len());
    }
    Ok(())
}

/// Install the dictionary, or re-download it when `force`.
pub fn install(force: bool) -> Result<()> {
    match hash_fetch::ensure(force) {
        Some(dir) => {
            let mapper = hashes::load(Some(dir.as_path()));
            println!("{} names ready at {}", mapper.len(), dir.display());
        }
        None => println!("The dictionary could not be installed."),
    }
    Ok(())
}
