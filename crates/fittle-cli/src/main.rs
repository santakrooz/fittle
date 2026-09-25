//! `fittle` command-line front end. Formatting only; all logic lives in the
//! `fittle-*` crates.

mod header;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Exit codes shared by every command.
pub mod exit {
    pub const OK: u8 = 0;
    pub const ERROR: u8 = 1;
    pub const VALIDATION: u8 = 2;
}

#[derive(Parser)]
#[command(
    name = "fittle",
    version,
    about = "Explain, inspect and convert astrophotography FITS files"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show a file's header records
    Header(header::Args),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Header(args) => header::run(args),
    };
    ExitCode::from(code)
}
