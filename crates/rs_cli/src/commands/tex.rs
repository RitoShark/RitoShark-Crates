#![forbid(unsafe_code)]
/*!
The `tex` subcommands. `info` prints texture metadata (optionally as JSON). `decode` writes an
image or DDS chosen by the output extension. `encode` compresses a standard image into a `.tex`
with a chosen block format and mipmap setting. All work uses `rs_tex` in-process.
*/

use std::path::Path;

use crate::error::{CliError, Result};

/// Parsed encode-format choice: either a block-compressed format (routed to
/// [`ritoshark::tex::Texture::encode`]) or uncompressed BGRA8 (routed to
/// [`ritoshark::tex::Texture::from_rgba_bgra8`]).
enum EncodeFormat {
    Bc(ritoshark::tex::TexFormat),
    Bgra8,
}

/// Parse a user-supplied format string (case-insensitive) into an [`EncodeFormat`].
/// Accepted values: `bc1`, `bc3`, `bc5`, `bc7`, `bgra8`.
fn parse_format(name: &str) -> Result<EncodeFormat> {
    use ritoshark::tex::TexFormat;
    Ok(match name.to_ascii_lowercase().as_str() {
        "bc1" => EncodeFormat::Bc(TexFormat::Bc1),
        "bc3" => EncodeFormat::Bc(TexFormat::Bc3),
        "bc5" => EncodeFormat::Bc(TexFormat::Bc5),
        "bc7" => EncodeFormat::Bc(TexFormat::Bc7),
        "bgra8" => EncodeFormat::Bgra8,
        other => return Err(CliError::msg(format!("unknown tex format: {other}"))),
    })
}

pub fn info(input: &Path, json: bool) -> Result<()> {
    use ritoshark::prelude::*;
    let tex = ritoshark::tex::Texture::from_path(input)?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "width": tex.width, "height": tex.height,
                "format": format!("{:?}", tex.format), "mips": tex.mips.len(),
            })
        );
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
    let out = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| input.with_extension("png"));
    let ext = out
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    if ext == "dds" {
        tex.save_dds(&out)
            .map_err(|e| CliError::msg(format!("write dds: {e}")))?;
    } else {
        let img = tex
            .decode_rgba()
            .map_err(|e| CliError::msg(format!("decode: {e}")))?;
        img.save(&out)
            .map_err(|e| CliError::msg(format!("write image: {e}")))?;
    }
    Ok(())
}

pub fn encode(input: &Path, format: &str, mipmaps: bool, output: Option<&Path>) -> Result<()> {
    use ritoshark::prelude::*;
    let fmt = parse_format(format)?;
    let img = image::open(input)
        .map_err(|e| CliError::msg(format!("open image: {e}")))?
        .to_rgba8();
    let tex = match fmt {
        EncodeFormat::Bc(tex_fmt) => ritoshark::tex::Texture::encode(&img, tex_fmt, mipmaps)
            .map_err(|e| CliError::msg(format!("encode: {e}")))?,
        // `from_rgba_bgra8` is infallible; mipmaps flag is intentionally ignored —
        // uncompressed BGRA8 is always a single full-resolution surface.
        EncodeFormat::Bgra8 => ritoshark::tex::Texture::from_rgba_bgra8(&img),
    };
    let out = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| input.with_extension("tex"));
    tex.to_path(&out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_format_accepts_known_formats() {
        // BC formats — exact case and upper-case variants must both succeed.
        assert!(matches!(parse_format("bc1"), Ok(EncodeFormat::Bc(_))));
        assert!(matches!(parse_format("BC1"), Ok(EncodeFormat::Bc(_))));
        assert!(matches!(parse_format("bc3"), Ok(EncodeFormat::Bc(_))));
        assert!(matches!(parse_format("BC3"), Ok(EncodeFormat::Bc(_))));
        assert!(matches!(parse_format("bc5"), Ok(EncodeFormat::Bc(_))));
        assert!(matches!(parse_format("bc7"), Ok(EncodeFormat::Bc(_))));
        // Uncompressed BGRA8 must succeed and route to the Bgra8 variant.
        assert!(matches!(parse_format("bgra8"), Ok(EncodeFormat::Bgra8)));
        assert!(matches!(parse_format("BGRA8"), Ok(EncodeFormat::Bgra8)));
    }

    #[test]
    fn parse_format_rejects_unknown_strings() {
        assert!(parse_format("garbage").is_err());
        assert!(parse_format("dxt5").is_err());
        assert!(parse_format("").is_err());
        assert!(parse_format("bc2").is_err());
    }
}
