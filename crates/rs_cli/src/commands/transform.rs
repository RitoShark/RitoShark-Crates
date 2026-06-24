#![forbid(unsafe_code)]
/*!
The generic `transform` command. It detects the input by extension, routes to the matching
in-process conversion, and — when no output path is given — derives one by swapping to the
opposite representation. Recursive mode walks a directory and converts every file whose
extension matches the input format. All conversion is done through the RitoShark crates.
*/

use std::path::{Path, PathBuf};

use rs_hash::HashMapper;
use walkdir::WalkDir;

use crate::error::{CliError, Result};
use crate::hashes;

const TEXT_EXTS: &[&str] = &["ritobin", "txt", "py"];
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "tga", "bmp", "webp"];

fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Pick the opposite representation's output path when none is supplied.
fn default_output(input: &Path) -> Result<PathBuf> {
    let ext = ext_of(input);
    let new_ext = if ext == "bin" {
        "ritobin"
    } else if TEXT_EXTS.contains(&ext.as_str()) {
        "bin"
    } else if ext == "tex" {
        "png"
    } else if IMAGE_EXTS.contains(&ext.as_str()) || ext == "dds" {
        "tex"
    } else {
        return Err(CliError::UnsupportedConversion {
            from: ext,
            to: "?".into(),
        });
    };
    Ok(input.with_extension(new_ext))
}

/// Convert a single file. Returns the written output path.
pub fn convert_one(
    input: &Path,
    output: &Path,
    keep_hashed: bool,
    mapper: &HashMapper,
) -> Result<PathBuf> {
    use ritoshark::prelude::*;
    let in_ext = ext_of(input);
    let out_ext = ext_of(output);

    if in_ext == "bin" && TEXT_EXTS.contains(&out_ext.as_str()) {
        let bin = ritoshark::bin::Bin::from_path(input)?;
        let m = if keep_hashed { None } else { Some(mapper) };
        let text = ritoshark::bin::to_text(&bin, m);
        std::fs::write(output, text)?;
    } else if TEXT_EXTS.contains(&in_ext.as_str()) && out_ext == "bin" {
        let text = std::fs::read_to_string(input)?;
        let bin = ritoshark::bin::from_text(&text, None)
            .map_err(|e| CliError::Message(format!("parse text: {e}")))?;
        bin.to_path(output)?;
    } else if in_ext == "tex" && IMAGE_EXTS.contains(&out_ext.as_str()) {
        let tex = ritoshark::tex::Texture::from_path(input)?;
        let img = tex
            .decode_rgba()
            .map_err(|e| CliError::Message(format!("decode tex: {e}")))?;
        img.save(output)
            .map_err(|e| CliError::Message(format!("write image: {e}")))?;
    } else if in_ext == "tex" && out_ext == "dds" {
        let tex = ritoshark::tex::Texture::from_path(input)?;
        tex.save_dds(output)
            .map_err(|e| CliError::Message(format!("write dds: {e}")))?;
    } else if IMAGE_EXTS.contains(&in_ext.as_str()) && out_ext == "tex" {
        let img = image::open(input)
            .map_err(|e| CliError::Message(format!("open image: {e}")))?
            .to_rgba8();
        let tex = ritoshark::tex::Texture::encode(&img, ritoshark::tex::TexFormat::Bc3, true)
            .map_err(|e| CliError::Message(format!("encode tex: {e}")))?;
        tex.to_path(output)?;
    } else if in_ext == "dds" && out_ext == "tex" {
        let img = ritoshark::tex::read_dds(input)
            .map_err(|e| CliError::Message(format!("read dds: {e}")))?;
        let tex = ritoshark::tex::Texture::encode(&img, ritoshark::tex::TexFormat::Bc3, true)
            .map_err(|e| CliError::Message(format!("encode tex: {e}")))?;
        tex.to_path(output)?;
    } else {
        return Err(CliError::UnsupportedConversion {
            from: in_ext,
            to: out_ext,
        });
    }
    Ok(output.to_path_buf())
}

/// Run the `transform` command for a file or (with `recursive`) a directory.
pub fn run(
    input: &Path,
    output: Option<&Path>,
    recursive: bool,
    keep_hashed: bool,
    hashes_flag: Option<&Path>,
) -> Result<()> {
    let mapper = hashes::load(hashes_flag);
    if recursive {
        if !input.is_dir() {
            return Err(CliError::msg(
                "recursive transform requires a directory input",
            ));
        }
        let mut failed = false;
        for entry in WalkDir::new(input).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path();
            let out = match default_output(p) {
                Ok(o) => o,
                Err(_) => continue,
            };
            if let Err(e) = convert_one(p, &out, keep_hashed, &mapper) {
                eprintln!("error: {}: {e}", p.display());
                failed = true;
            }
        }
        if failed {
            return Err(CliError::msg("one or more files failed to convert"));
        }
        Ok(())
    } else {
        let out = match output {
            Some(o) => o.to_path_buf(),
            None => default_output(input)?,
        };
        convert_one(input, &out, keep_hashed, &mapper)?;
        Ok(())
    }
}
