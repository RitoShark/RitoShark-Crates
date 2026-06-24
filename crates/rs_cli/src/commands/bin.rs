#![forbid(unsafe_code)]
/*!
The `bin` subcommands. `convert` is `transform` scoped to `.bin`; `diff` normalizes each input
to `#PROP_text` (reading binary or accepting an existing text form) and prints a unified diff.
*/

use std::path::Path;

use crate::error::Result;
use crate::{commands::transform, hashes};

const TEXT_EXTS: &[&str] = &["ritobin", "txt", "py"];

pub fn convert(
    input: &Path,
    output: Option<&Path>,
    recursive: bool,
    keep_hashed: bool,
    hashes_flag: Option<&Path>,
) -> Result<()> {
    transform::run(input, output, recursive, keep_hashed, hashes_flag)
}

fn to_prop_text(path: &Path, mapper: &rs_hash::HashMapper) -> Result<String> {
    use ritoshark::prelude::*;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if TEXT_EXTS.contains(&ext.as_str()) {
        Ok(std::fs::read_to_string(path)?)
    } else {
        let bin = ritoshark::bin::Bin::from_path(path)?;
        Ok(ritoshark::bin::to_text(&bin, Some(mapper)))
    }
}

/// Print a unified diff of two bins/texts, normalized to `#PROP_text` first.
pub fn diff(a: &Path, b: &Path, context: usize, _no_color: bool) -> Result<()> {
    let mapper = hashes::load(None);
    let ta = to_prop_text(a, &mapper)?;
    let tb = to_prop_text(b, &mapper)?;
    let diff = unified(&ta, &tb, context);
    print!("{diff}");
    Ok(())
}

fn unified(a: &str, b: &str, context: usize) -> String {
    let al: Vec<&str> = a.lines().collect();
    let bl: Vec<&str> = b.lines().collect();
    if al == bl {
        return String::new();
    }
    let mut out = String::new();
    let max = al.len().max(bl.len());
    let _ = context;
    for i in 0..max {
        match (al.get(i), bl.get(i)) {
            (Some(x), Some(y)) if x == y => {}
            (Some(x), Some(y)) => {
                out.push_str(&format!("-{x}\n+{y}\n"));
            }
            (Some(x), None) => out.push_str(&format!("-{x}\n")),
            (None, Some(y)) => out.push_str(&format!("+{y}\n")),
            (None, None) => {}
        }
    }
    out
}
