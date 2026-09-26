//! `fittle fpack | funpack`: lossless tile compression and its inverse.
//! Outputs are new files (`x.fits` → `x.fits.fz` and back); sources are
//! never replaced, and every output is decoded again before it is kept.

use std::path::{Path, PathBuf};

use clap::{Args as ClapArgs, ValueEnum};
use fittle_image::fpack::{self, Method, PackError, PackOptions, PackReport};
use serde::Serialize;

use crate::exit;
use crate::fmt::{bad, good, muted};

#[derive(Clone, Copy, ValueEnum)]
pub enum MethodArg {
    Rice,
    Gzip1,
    Gzip2,
}

#[derive(ClapArgs)]
pub struct PackArgs {
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Output directory, or a file name for a single input (default: beside the source)
    #[arg(short, long, value_name = "PATH")]
    out: Option<PathBuf>,
    /// Compression (default: RICE_1 for integers, GZIP_2 for floats)
    #[arg(long, value_enum)]
    method: Option<MethodArg>,
    /// Image rows per tile
    #[arg(long, value_name = "N")]
    tile_rows: Option<usize>,
    /// Print JSON (schema fittle.pack/1)
    #[arg(long)]
    json: bool,
}

#[derive(ClapArgs)]
pub struct UnpackArgs {
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Output directory, or a file name for a single input (default: beside the source)
    #[arg(short, long, value_name = "PATH")]
    out: Option<PathBuf>,
    /// Print JSON (schema fittle.pack/1)
    #[arg(long)]
    json: bool,
}

#[derive(Serialize)]
struct Doc {
    schema: &'static str,
    files: Vec<Row>,
}

#[derive(Serialize)]
struct Row {
    source: String,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    report: Option<PackReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn size(b: u64) -> String {
    match b {
        0..1_000_000 => format!("{:.0} kB", b as f64 / 1e3),
        _ => format!("{:.1} MB", b as f64 / 1e6),
    }
}

fn run(
    files: &[PathBuf],
    out: &Option<PathBuf>,
    json: bool,
    name: fn(&Path) -> PathBuf,
    op: impl Fn(&Path, &Path) -> Result<PackReport, PackError>,
) -> u8 {
    let to_file = out
        .as_ref()
        .is_some_and(|p| !p.is_dir() && p.extension().is_some());
    if to_file && files.len() > 1 {
        eprintln!("fittle: -o names a file; give a directory for several inputs");
        return exit::VALIDATION;
    }
    if let Some(d) = out.as_ref().filter(|_| !to_file) {
        if let Err(e) = std::fs::create_dir_all(d) {
            eprintln!("fittle: {}: {e}", d.display());
            return exit::ERROR;
        }
    }
    let mut code = exit::OK;
    let mut rows = Vec::new();
    for f in files {
        let dest = match out {
            Some(p) if to_file => p.clone(),
            Some(d) => d.join(name(f).file_name().expect("file name")),
            None => name(f),
        };
        let r = op(f, &dest);
        let (report, error) = match r {
            Ok(r) => (Some(r), None),
            Err(e) => {
                code = code.max(if matches!(e, PackError::Invalid(_)) {
                    exit::VALIDATION
                } else {
                    exit::ERROR
                });
                (None, Some(e.to_string()))
            }
        };
        if !json {
            match (&report, &error) {
                (Some(r), _) => println!(
                    "{} {} {}",
                    good("✓"),
                    r.path,
                    muted(&format!(
                        "{} → {} ({:.0}%), verified",
                        size(r.bytes_in),
                        size(r.bytes_out),
                        100.0 * r.bytes_out as f64 / r.bytes_in.max(1) as f64
                    ))
                ),
                (_, Some(e)) => eprintln!("{} {}: {e}", bad("✗"), f.display()),
                _ => {}
            }
        }
        rows.push(Row {
            source: f.to_string_lossy().into(),
            report,
            error,
        });
    }
    if json {
        let doc = Doc {
            schema: "fittle.pack/1",
            files: rows,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&doc).expect("serializable")
        );
    }
    code
}

pub fn run_fpack(a: PackArgs) -> u8 {
    let opts = PackOptions {
        method: a.method.map(|m| match m {
            MethodArg::Rice => Method::Rice,
            MethodArg::Gzip1 => Method::Gzip1,
            MethodArg::Gzip2 => Method::Gzip2,
        }),
        tile_rows: a.tile_rows,
    };
    run(&a.files, &a.out, a.json, fpack::packed_name, |s, d| {
        fpack::fpack(s, d, &opts)
    })
}

pub fn run_funpack(a: UnpackArgs) -> u8 {
    run(
        &a.files,
        &a.out,
        a.json,
        fpack::unpacked_name,
        fpack::funpack,
    )
}
