#![forbid(unsafe_code)]

use std::path::Path;

use crate::error::Result;

/// Print just the detected `FileKind` for a file.
pub fn detect(file: &Path, json: bool) -> Result<()> {
    let kind = ritoshark::file::detect_path(file)?;
    if json {
        println!("{{\"kind\":\"{kind:?}\"}}");
    } else {
        println!("{kind:?}");
    }
    Ok(())
}
