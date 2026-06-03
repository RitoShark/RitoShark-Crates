#![forbid(unsafe_code)]
/*!
The `ritoshark` command-line front end: a thin binary over the RitoShark crates. It detects a
file's format, renders a `.bin` document as `#PROP_text`, lists the chunk table of a `.wad`
archive, and decodes a `.tex` texture to a PNG. Every subcommand calls the library through the
umbrella crate and reports failures as the underlying error.
*/

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use ritoshark::prelude::Parse;
use rs_hash::HashMapper;

type Error = Box<dyn std::error::Error>;

#[derive(Parser)]
#[command(name = "ritoshark", about = "RitoShark command-line tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Identify a file's format from its magic bytes.
    Detect {
        file: PathBuf,
    },
    /// Operate on `.bin`/PROP documents.
    #[command(subcommand)]
    Bin(BinCommand),
    /// Operate on `.wad` archives.
    #[command(subcommand)]
    Wad(WadCommand),
    /// Operate on `.tex` textures.
    #[command(subcommand)]
    Tex(TexCommand),
}

#[derive(Subcommand)]
enum BinCommand {
    /// Render a `.bin` document as `#PROP_text`.
    ToText {
        input: PathBuf,
        /// Write the text to this file instead of stdout.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Resolve hashes to names using this CDTB-style dictionary.
        #[arg(long)]
        hashes: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum WadCommand {
    /// List the chunk table of an archive.
    List {
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum TexCommand {
    /// Decode a texture's full-resolution mip to a PNG.
    ToPng {
        input: PathBuf,
        output: PathBuf,
    },
}

fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    match cli.command {
        Command::Detect { file } => detect(&file),
        Command::Bin(BinCommand::ToText {
            input,
            output,
            hashes,
        }) => bin_to_text(&input, output.as_deref(), hashes.as_deref()),
        Command::Wad(WadCommand::List { file }) => wad_list(&file),
        Command::Tex(TexCommand::ToPng { input, output }) => tex_to_png(&input, &output),
    }
}

fn detect(file: &std::path::Path) -> Result<(), Error> {
    let kind = ritoshark::file::detect_path(file)?;
    println!("{kind:?}");
    Ok(())
}

fn bin_to_text(
    input: &std::path::Path,
    output: Option<&std::path::Path>,
    hashes: Option<&std::path::Path>,
) -> Result<(), Error> {
    let bin = ritoshark::bin::Bin::from_path(input)?;
    let mapper = match hashes {
        Some(path) => Some(HashMapper::load_file(path)?),
        None => None,
    };
    let text = ritoshark::bin::to_text(&bin, mapper.as_ref());
    match output {
        Some(path) => std::fs::write(path, text)?,
        None => print!("{text}"),
    }
    Ok(())
}

fn wad_list(file: &std::path::Path) -> Result<(), Error> {
    let wad = ritoshark::wad::Wad::from_path(file)?;
    println!("{} chunks", wad.chunks.len());
    for chunk in &wad.chunks {
        println!(
            "{:016x}  {} -> {}  {:?}",
            chunk.path_hash, chunk.compressed_size, chunk.uncompressed_size, chunk.compression
        );
    }
    Ok(())
}

fn tex_to_png(input: &std::path::Path, output: &std::path::Path) -> Result<(), Error> {
    let texture = ritoshark::tex::Texture::from_path(input)?;
    let image = texture.decode_rgba()?;
    image.save(output)?;
    Ok(())
}
