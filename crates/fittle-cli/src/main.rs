//! `fittle` command-line front end. Formatting only; all logic lives in the
//! `fittle-*` crates.

mod diff;
mod edit;
mod fmt;
mod header;
mod info;

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
    /// Explain files: frame type, rig, exposure, site and derived facts
    Info(info::Args),
    /// Show a file's header records
    Header(header::Args),
    /// Compare two headers, with calibration impact
    Diff(diff::Args),
    /// Set keywords (KEY=VALUE) in one or more files
    Set(edit::SetArgs),
    /// Remove keywords from one or more files
    Unset(edit::UnsetArgs),
    /// Rename a keyword in one or more files
    RenameKey(edit::RenameArgs),
    /// Remove site coordinates, observer names and serial numbers
    Scrub(edit::ScrubArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Info(args) => info::run(args),
        Command::Header(args) => header::run(args),
        Command::Diff(args) => diff::run(args),
        Command::Set(args) => edit::run_set(args),
        Command::Unset(args) => edit::run_unset(args),
        Command::RenameKey(args) => edit::run_rename(args),
        Command::Scrub(args) => edit::run_scrub(args),
    };
    ExitCode::from(code)
}
