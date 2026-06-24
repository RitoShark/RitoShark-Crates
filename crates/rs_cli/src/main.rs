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
