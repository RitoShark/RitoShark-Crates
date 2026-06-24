#![forbid(unsafe_code)]
/*!
The `audio extract` command writes every `.wem` from a `.wpk` or `.bnk` container into a
directory, naming each by its entry name when present and by id or index otherwise.  Container
parsing is done in-process through `rs_audio`.

Path safety: every candidate filename is passed through [`crate::pathsafe::safe_relative`] so
that adversarial entry names (containing `..`, `/`, or absolute roots) cannot escape the output
directory.

Collision handling: a [`std::collections::HashSet`] tracks every relative path already written in
this invocation.  When a collision is detected the entry is renamed by inserting an incrementing
counter before the extension (`name (1).wem`, `name (2).wem`, …) until a free slot is found.
*/

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::{CliError, Result};

/// Return the extension of `p` as a `&str`, or `None`.
fn ext_str(p: &Path) -> Option<&str> {
    p.extension().and_then(|e| e.to_str())
}

/// Return the first path derived from `base` (a relative `PathBuf`) that is
/// not present in `seen`.  If `base` itself is free it is returned unchanged;
/// otherwise successive `(1)`, `(2)`, … suffixes are tried.  The result is
/// inserted into `seen` before returning.
fn dedup(base: PathBuf, seen: &mut HashSet<PathBuf>) -> PathBuf {
    if seen.insert(base.clone()) {
        return base;
    }
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext_part = ext_str(&base).map(|e| format!(".{e}")).unwrap_or_default();
    let parent = base.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut n: u32 = 1;
    loop {
        let candidate = parent.join(format!("{stem} ({n}){ext_part}"));
        if seen.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

pub fn extract(input: &Path, out: &Path) -> Result<()> {
    use ritoshark::prelude::*;
    std::fs::create_dir_all(out)?;
    let kind = ritoshark::file::detect_path(input)?;
    let mut seen: HashSet<PathBuf> = HashSet::new();
    match kind {
        ritoshark::file::FileKind::Wpk => {
            let wpk = ritoshark::audio::Wpk::from_path(input)?;
            for (idx, (id, name, data)) in wpk.wems().into_iter().enumerate() {
                // Naming precedence: non-empty name → id-based → index-based.
                let raw = if !name.is_empty() {
                    name.to_string()
                } else if let Some(id) = id {
                    format!("{id}.wem")
                } else {
                    format!("{idx}.wem")
                };
                let safe = match crate::pathsafe::safe_relative(&raw) {
                    Some(p) => p,
                    None => {
                        eprintln!("audio extract: skipping unsafe path: {raw}");
                        continue;
                    }
                };
                let rel = dedup(safe, &mut seen);
                if rel.to_string_lossy() != raw {
                    eprintln!(
                        "audio extract: collision — renamed '{}' → '{}'",
                        raw,
                        rel.display()
                    );
                }
                std::fs::write(out.join(&rel), data)?;
            }
        }
        ritoshark::file::FileKind::Bnk => {
            let bnk = ritoshark::audio::Bnk::from_path(input)?;
            for (id, data) in bnk.wems() {
                let raw = format!("{id}.wem");
                // Bnk ids are numeric so safe_relative always succeeds, but
                // route through the guard for uniformity.
                let safe = match crate::pathsafe::safe_relative(&raw) {
                    Some(p) => p,
                    None => {
                        eprintln!("audio extract: skipping unsafe path: {raw}");
                        continue;
                    }
                };
                let rel = dedup(safe, &mut seen);
                if rel.to_string_lossy() != raw {
                    eprintln!(
                        "audio extract: collision — renamed '{}' → '{}'",
                        raw,
                        rel.display()
                    );
                }
                std::fs::write(out.join(&rel), data)?;
            }
        }
        other => return Err(CliError::msg(format!("not an audio container: {other:?}"))),
    }
    Ok(())
}
