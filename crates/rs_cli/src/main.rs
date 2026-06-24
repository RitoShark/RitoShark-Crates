#![forbid(unsafe_code)]
/*!
The `rs_cli` command-line front end: a thin binary over the RitoShark crates. It detects a
file's format, reads per-format info, transforms between formats, and runs format-specific
actions for bin, wad, tex, rst, and audio. Every subcommand calls the library through the
umbrella crate and reports failures via miette. It never invokes any external program.
*/

mod commands;
mod error;
mod hashes;
mod output;
mod pathsafe;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use error::Result;

#[derive(Parser)]
#[command(name = "rs_cli", about = "RitoShark command-line tools", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Identify a file's format from its magic bytes.
    Detect {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Detect a file and print a per-format summary.
    Read {
        file: PathBuf,
        #[arg(long)]
        json: bool,
        /// Hash dictionary directory or file for name resolution.
        #[arg(long)]
        hashes: Option<PathBuf>,
    },
    /// Convert a file (or directory with -r) between formats.
    Transform {
        input: PathBuf,
        output: Option<PathBuf>,
        #[arg(short, long)]
        recursive: bool,
        #[arg(short = 'k', long)]
        keep_hashed: bool,
        #[arg(long)]
        hashes: Option<PathBuf>,
    },
    /// Operate on `.bin`/PROP documents.
    #[command(subcommand)]
    Bin(BinCmd),
    /// Operate on `.wad` archives.
    #[command(subcommand)]
    Wad(WadCmd),
    /// Operate on `.tex` textures.
    #[command(subcommand)]
    Tex(TexCmd),
    /// Operate on `.stringtable` (RST) files.
    #[command(subcommand)]
    Rst(RstCmd),
    /// Operate on `.wpk`/`.bnk` audio containers.
    #[command(subcommand)]
    Audio(AudioCmd),
}

#[derive(Subcommand)]
enum BinCmd {
    /// Convert .bin <-> text.
    Convert {
        input: PathBuf,
        output: Option<PathBuf>,
        #[arg(short, long)]
        recursive: bool,
        #[arg(short = 'k', long)]
        keep_hashed: bool,
        #[arg(long)]
        hashes: Option<PathBuf>,
    },
    /// Unified diff of two bins/texts.
    Diff {
        a: PathBuf,
        b: PathBuf,
        #[arg(short = 'C', long, default_value_t = 3)]
        context: usize,
        #[arg(long)]
        no_color: bool,
    },
}

#[derive(Subcommand)]
enum WadCmd {
    /// List chunks in one or more archives.
    List {
        archives: Vec<PathBuf>,
        #[arg(short = 'F', long, default_value = "table")]
        format: String,
        #[arg(long, default_value_t = true)]
        stats: bool,
        #[arg(long)]
        hashes: Option<PathBuf>,
    },
    /// Extract chunks to a directory.
    Extract {
        archives: Vec<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(short = 'f', long = "filter-type", num_args = 0..)]
        filter_type: Vec<String>,
        #[arg(short = 'x', long)]
        pattern: Option<String>,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        hashes: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum TexCmd {
    /// Print texture metadata.
    Info {
        input: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Decode a texture to an image or DDS.
    Decode {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, default_value_t = 0)]
        mip: u32,
    },
    /// Encode an image into a texture.
    Encode {
        input: PathBuf,
        #[arg(short = 'f', long)]
        format: String,
        #[arg(short = 'm', long, default_value_t = true)]
        mipmaps: bool,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum RstCmd {
    /// List entries.
    List {
        input: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum AudioCmd {
    /// Extract .wem files.
    Extract {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Detect { file, json } => commands::read::detect(&file, json),
        Command::Read { file, json, hashes } => {
            commands::read::read(&file, json, hashes.as_deref())
        }
        Command::Transform {
            input,
            output,
            recursive,
            keep_hashed,
            hashes,
        } => commands::transform::run(
            &input,
            output.as_deref(),
            recursive,
            keep_hashed,
            hashes.as_deref(),
        ),
        Command::Bin(BinCmd::Convert {
            input,
            output,
            recursive,
            keep_hashed,
            hashes,
        }) => commands::bin::convert(
            &input,
            output.as_deref(),
            recursive,
            keep_hashed,
            hashes.as_deref(),
        ),
        Command::Bin(BinCmd::Diff {
            a,
            b,
            context,
            no_color,
        }) => commands::bin::diff(&a, &b, context, no_color),
        Command::Wad(WadCmd::List {
            archives,
            format,
            stats,
            hashes,
        }) => commands::wad::list(&archives, &format, stats, hashes.as_deref()),
        Command::Wad(WadCmd::Extract {
            archives,
            output,
            filter_type,
            pattern,
            overwrite,
            hashes,
        }) => commands::wad::extract(
            &archives,
            &output,
            &filter_type,
            pattern.as_deref(),
            overwrite,
            hashes.as_deref(),
        ),
        Command::Tex(TexCmd::Info { input, json }) => commands::tex::info(&input, json),
        Command::Tex(TexCmd::Decode { input, output, mip }) => {
            commands::tex::decode(&input, output.as_deref(), mip)
        }
        Command::Tex(TexCmd::Encode {
            input,
            format,
            mipmaps,
            output,
        }) => commands::tex::encode(&input, &format, mipmaps, output.as_deref()),
        Command::Rst(RstCmd::List { input, json }) => commands::rst::list(&input, json),
        Command::Audio(AudioCmd::Extract { input, output }) => {
            commands::audio::extract(&input, &output)
        }
    }
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => {}
        Err(error::CliError::UnknownFormat(_)) => {
            eprintln!("error: unknown or undetectable format");
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("{:?}", miette::Report::new(e));
            std::process::exit(1);
        }
    }
}
