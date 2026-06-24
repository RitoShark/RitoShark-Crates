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
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Detect { file, json } => commands::read::detect(&file, json),
    }
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();
    run(cli).map_err(Into::into)
}
