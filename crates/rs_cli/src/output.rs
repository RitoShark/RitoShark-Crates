#![forbid(unsafe_code)]
/*!
Renders per-format summaries for the `read`/`info` commands, in both a human one-line-per-fact
form and a stable JSON object. The CLI builds JSON here from public accessors so no format
crate needs a serde dependency.
*/

use ritoshark::file::FileKind;
use rs_hash::HashMapper;
use serde_json::{Value, json};

use crate::error::Result;

/// Build a JSON summary for a detected file, reading it with the matching format type.
pub fn summary_json(kind: FileKind, file: &std::path::Path, _mapper: &HashMapper) -> Result<Value> {
    use ritoshark::prelude::*;
    let v = match kind {
        FileKind::PropBin | FileKind::PatchBin => {
            let bin = ritoshark::bin::Bin::from_path(file)?;
            json!({
                "kind": format!("{kind:?}"),
                "patch": bin.is_patch,
                "version": bin.version,
                "linked": bin.linked.len(),
                "entries": bin.entries.len(),
                "patches": bin.patches.len(),
            })
        }
        FileKind::Wad => {
            let wad = ritoshark::wad::Wad::from_path(file)?;
            json!({ "kind": "Wad", "chunks": wad.chunks.len() })
        }
        FileKind::Tex => {
            let tex = ritoshark::tex::Texture::from_path(file)?;
            json!({
                "kind": "Tex",
                "width": tex.width,
                "height": tex.height,
                "format": format!("{:?}", tex.format),
                "mips": tex.mips.len(),
            })
        }
        FileKind::Rst => {
            let rst = ritoshark::rst::Rst::from_path(file)?;
            json!({ "kind": "Rst", "version": rst.version, "entries": rst.entries.len() })
        }
        FileKind::Unknown => {
            return Err(crate::error::CliError::UnknownFormat(
                file.display().to_string(),
            ));
        }
        other => json!({ "kind": format!("{other:?}") }),
    };
    Ok(v)
}

/// Render a JSON summary as human-readable `key: value` lines.
pub fn human_from_json(value: &Value) -> String {
    let mut out = String::new();
    if let Value::Object(map) = value {
        for (k, v) in map {
            out.push_str(&format!("{k}: {}\n", render_scalar(v)));
        }
    }
    out
}

fn render_scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
