#![forbid(unsafe_code)]
/*!
The `tex` subcommands. `info` prints texture metadata (optionally as JSON). `decode` writes an
image or DDS chosen by the output extension. `encode` compresses a standard image into a `.tex`
with a chosen block format and mipmap setting. All work uses `rs_tex` in-process.
*/

use std::path::Path;

use crate::error::{CliError, Result};

fn parse_format(name: &str) -> Result<ritoshark::tex::TexFormat> {
    use ritoshark::tex::TexFormat;
    Ok(match name.to_ascii_lowercase().as_str() {
        "bc1" => TexFormat::Bc1,
        "bc3" => TexFormat::Bc3,
        "bc7" => TexFormat::Bc7,
        "bgra8" => TexFormat::Bgra8,
        other => return Err(CliError::msg(format!("unknown tex format: {other}"))),
    })
}

pub fn info(input: &Path, json: bool) -> Result<()> {
    use ritoshark::prelude::*;
    let tex = ritoshark::tex::Texture::from_path(input)?;
    if json {
        println!("{}", serde_json::json!({
            "width": tex.width, "height": tex.height,
            "format": format!("{:?}", tex.format), "mips": tex.mips.len(),
        }));
    } else {
        println!("width: {}", tex.width);
        println!("height: {}", tex.height);
        println!("format: {:?}", tex.format);
        println!("mips: {}", tex.mips.len());
    }
    Ok(())
}

pub fn decode(input: &Path, output: Option<&Path>, _mip: u32) -> Result<()> {
    use ritoshark::prelude::*;
    let tex = ritoshark::tex::Texture::from_path(input)?;
    let out = output.map(|p| p.to_path_buf()).unwrap_or_else(|| input.with_extension("png"));
    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
    if ext == "dds" {
        tex.save_dds(&out).map_err(|e| CliError::msg(format!("write dds: {e}")))?;
    } else {
        let img = tex.decode_rgba().map_err(|e| CliError::msg(format!("decode: {e}")))?;
        img.save(&out).map_err(|e| CliError::msg(format!("write image: {e}")))?;
    }
    Ok(())
}

pub fn encode(input: &Path, format: &str, mipmaps: bool, output: Option<&Path>) -> Result<()> {
    use ritoshark::prelude::*;
    let fmt = parse_format(format)?;
    let img = image::open(input)
        .map_err(|e| CliError::msg(format!("open image: {e}")))?
        .to_rgba8();
    let tex = ritoshark::tex::Texture::encode(&img, fmt, mipmaps)
        .map_err(|e| CliError::msg(format!("encode: {e}")))?;
    let out = output.map(|p| p.to_path_buf()).unwrap_or_else(|| input.with_extension("tex"));
    tex.to_path(&out)?;
    Ok(())
}
