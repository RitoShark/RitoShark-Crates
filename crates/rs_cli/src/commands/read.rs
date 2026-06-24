#![forbid(unsafe_code)]

use std::path::Path;

use crate::error::Result;
use crate::{hashes, output};

/// Print just the detected `FileKind`.
pub fn detect(file: &Path, json: bool) -> Result<()> {
    let kind = ritoshark::file::detect_path(file)?;
    if json {
        println!("{}", serde_json::json!({ "kind": format!("{kind:?}") }));
    } else {
        println!("{kind:?}");
    }
    Ok(())
}

/// Detect a file and print a per-format summary, human-readable or JSON.
pub fn read(file: &Path, json: bool, hashes_flag: Option<&Path>) -> Result<()> {
    let kind = ritoshark::file::detect_path(file)?;
    let mapper = hashes::load(hashes_flag);
    let value = output::summary_json(kind, file, &mapper)?;
    if json {
        println!("{value}");
    } else {
        print!("{}", output::human_from_json(&value));
    }
    Ok(())
}
