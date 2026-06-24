#![forbid(unsafe_code)]
/*!
The `audio extract` command writes every `.wem` from a `.wpk` or `.bnk` container into a
directory, naming each by its entry name when present and by id or index otherwise. Container
parsing is done in-process through `rs_audio`.
*/

use std::path::Path;

use crate::error::{CliError, Result};

pub fn extract(input: &Path, out: &Path) -> Result<()> {
    use ritoshark::prelude::*;
    std::fs::create_dir_all(out)?;
    let kind = ritoshark::file::detect_path(input)?;
    match kind {
        ritoshark::file::FileKind::Wpk => {
            let wpk = ritoshark::audio::Wpk::from_path(input)?;
            for (id, name, data) in wpk.wems() {
                let fname = if !name.is_empty() {
                    name.to_string()
                } else if let Some(id) = id {
                    format!("{id}.wem")
                } else {
                    "unnamed.wem".to_string()
                };
                std::fs::write(out.join(fname), data)?;
            }
        }
        ritoshark::file::FileKind::Bnk => {
            let bnk = ritoshark::audio::Bnk::from_path(input)?;
            for (id, data) in bnk.wems() {
                std::fs::write(out.join(format!("{id}.wem")), data)?;
            }
        }
        other => return Err(CliError::msg(format!("not an audio container: {other:?}"))),
    }
    Ok(())
}
