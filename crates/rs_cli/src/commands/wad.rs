#![forbid(unsafe_code)]
/*!
The `wad` subcommands. `list` prints the chunk table as a table, JSON, CSV, or flat list, with
optional summary statistics. `extract` writes each chunk to a directory, naming files by their
resolved path when a hash dictionary supplies one and by `<16-hex>.bin` otherwise, with type
and regex filters. All work is done in-process through `rs_wad`.
*/

use std::path::{Path, PathBuf};

use regex::Regex;
use ritoshark::prelude::Parse;
use rs_hash::HashMapper;

use crate::error::{CliError, Result};
use crate::hashes;

fn name_for(hash: u64, mapper: &HashMapper) -> Option<String> {
    mapper.get(hash).map(|s| s.to_string())
}

/// List chunks across one or more archives.
pub fn list(
    archives: &[PathBuf],
    format: &str,
    stats: bool,
    hashes_flag: Option<&Path>,
) -> Result<()> {
    let mapper = hashes::load(hashes_flag);
    for archive in archives {
        let wad = ritoshark::wad::Wad::from_path(archive)?;
        match format {
            "json" => {
                let arr: Vec<_> = wad
                    .chunks
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "hash": format!("{:016x}", c.path_hash),
                            "name": name_for(c.path_hash, &mapper),
                            "compressed": c.compressed_size,
                            "uncompressed": c.uncompressed_size,
                            "compression": format!("{:?}", c.compression),
                        })
                    })
                    .collect();
                println!("{}", serde_json::json!({ "chunks": arr }));
            }
            "csv" => {
                println!("hash,name,compressed,uncompressed,compression");
                for c in &wad.chunks {
                    println!(
                        "{:016x},{},{},{},{:?}",
                        c.path_hash,
                        name_for(c.path_hash, &mapper).unwrap_or_default(),
                        c.compressed_size,
                        c.uncompressed_size,
                        c.compression
                    );
                }
            }
            "flat" => {
                for c in &wad.chunks {
                    println!(
                        "{}",
                        name_for(c.path_hash, &mapper)
                            .unwrap_or_else(|| format!("{:016x}", c.path_hash))
                    );
                }
            }
            _ => {
                for c in &wad.chunks {
                    println!(
                        "{:016x}  {} -> {}  {:?}  {}",
                        c.path_hash,
                        c.compressed_size,
                        c.uncompressed_size,
                        c.compression,
                        name_for(c.path_hash, &mapper).unwrap_or_default()
                    );
                }
            }
        }
        if stats {
            let total_c: u64 = wad.chunks.iter().map(|c| c.compressed_size as u64).sum();
            let total_u: u64 = wad.chunks.iter().map(|c| c.uncompressed_size as u64).sum();
            eprintln!(
                "{}: {} chunks, {} -> {} bytes",
                archive.display(),
                wad.chunks.len(),
                total_c,
                total_u
            );
        }
    }
    Ok(())
}

/// Extract chunks to a directory.
pub fn extract(
    archives: &[PathBuf],
    out: &Path,
    filter_types: &[String],
    pattern: Option<&str>,
    overwrite: bool,
    hashes_flag: Option<&Path>,
) -> Result<()> {
    let mapper = hashes::load(hashes_flag);
    let re = match pattern {
        Some(p) => Some(Regex::new(p).map_err(|e| CliError::msg(format!("bad regex: {e}")))?),
        None => None,
    };
    std::fs::create_dir_all(out)?;
    for archive in archives {
        let wad = ritoshark::wad::Wad::from_path(archive)?;
        for chunk in &wad.chunks {
            let name = name_for(chunk.path_hash, &mapper);
            if let Some(re) = &re {
                let candidate = name
                    .clone()
                    .unwrap_or_else(|| format!("{:016x}", chunk.path_hash));
                if !re.is_match(&candidate) {
                    continue;
                }
            }
            let ext = name
                .as_deref()
                .and_then(|n| n.rsplit('.').next())
                .map(|e| e.to_ascii_lowercase())
                .unwrap_or_else(|| "bin".to_string());
            if !filter_types.is_empty()
                && !filter_types.iter().any(|t| t.eq_ignore_ascii_case(&ext))
            {
                continue;
            }
            let rel = name
                .clone()
                .unwrap_or_else(|| format!("{:016x}.{ext}", chunk.path_hash));
            let safe = match crate::pathsafe::safe_relative(&rel) {
                Some(p) => p,
                None => {
                    eprintln!("skipping unsafe path: {rel}");
                    continue;
                }
            };
            let dest = out.join(&safe);
            if dest.exists() && !overwrite {
                continue;
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let data = wad.chunk_data(chunk)?;
            std::fs::write(&dest, data)?;
        }
    }
    Ok(())
}
