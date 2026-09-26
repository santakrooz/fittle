//! `fittle` command-line front end. Formatting only; all logic lives in the
//! `fittle-*` crates.

mod diff;
mod edit;
mod export;
mod fmt;
mod header;
mod info;
mod mcp;
mod pack;

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
    /// Export to PNG, JPEG, WebP, TIFF or FITS with a stretch and transforms
    Export(export::ExportArgs),
    /// Crop to a rectangle (new FITS file)
    Crop(export::CropArgs),
    /// Rotate by quarter turns (new FITS file)
    Rotate(export::RotateArgs),
    /// Mirror horizontally or vertically (new FITS file)
    Flip(export::FlipArgs),
    /// Software-bin N×N (new FITS file)
    Bin(export::BinArgs),
    /// Resample to a long edge (new FITS file)
    Resize(export::ResizeArgs),
    /// Debayer a colour (CFA) frame to RGB (new FITS file)
    Debayer(export::DebayerArgs),
    /// Compress images losslessly (x.fits → x.fits.fz)
    Fpack(pack::PackArgs),
    /// Expand compressed images (x.fits.fz → x.fits)
    Funpack(pack::UnpackArgs),
    /// Summarize a folder: frames, nights, integration per target and filter
    Scan(mcp::ScanArgs),
    /// Run the MCP server on stdio (for Claude and other agents)
    Mcp(mcp::McpArgs),
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
        Command::Export(args) => export::run_export(args),
        Command::Crop(args) => export::run_crop(args),
        Command::Rotate(args) => export::run_rotate(args),
        Command::Flip(args) => export::run_flip(args),
        Command::Bin(args) => export::run_bin(args),
        Command::Resize(args) => export::run_resize(args),
        Command::Debayer(args) => export::run_debayer(args),
        Command::Fpack(args) => pack::run_fpack(args),
        Command::Funpack(args) => pack::run_funpack(args),
        Command::Scan(args) => mcp::run_scan(args),
        Command::Mcp(args) => mcp::run_mcp(args),
    };
    ExitCode::from(code)
}
