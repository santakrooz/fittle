//! `fittle mcp` (MCP server on stdio) and `fittle scan` (folder summary).

use std::path::PathBuf;

use clap::Args as ClapArgs;

use crate::exit;
use crate::fmt::{muted, warn};

#[derive(ClapArgs)]
pub struct McpArgs {
    /// Folder the server may read and write under (repeatable). Default:
    /// FITTLE_MCP_ROOTS, else your home folder.
    #[arg(long, value_name = "DIR")]
    root: Vec<PathBuf>,
}

pub fn run_mcp(a: McpArgs) -> u8 {
    let roots = fittle_mcp::Roots::new(a.root);
    // stdout carries the protocol; say where we are on stderr only.
    eprintln!(
        "fittle mcp: serving on stdio; allowed: {}",
        roots
            .dirs()
            .iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    match fittle_mcp::run_stdio(roots) {
        Ok(()) => exit::OK,
        Err(e) => {
            eprintln!("fittle mcp: {e}");
            exit::ERROR
        }
    }
}

#[derive(ClapArgs)]
pub struct ScanArgs {
    folder: PathBuf,
    /// Include subfolders
    #[arg(short, long)]
    recursive: bool,
    /// Print JSON (schema fittle.scan/1), with one row per file
    #[arg(long)]
    json: bool,
}

fn hours(s: f64) -> String {
    let m = (s / 60.0).round() as i64;
    match m {
        0 => format!("{} s", s.round()),
        1..60 => format!("{m} min"),
        _ => format!("{}h {:02}m", m / 60, m % 60),
    }
}

pub fn run_scan(a: ScanArgs) -> u8 {
    let entries = match if a.recursive {
        fittle_scan::list_recursive(&a.folder)
    } else {
        fittle_scan::list(&a.folder)
    } {
        Ok(e) => e,
        Err(e) => {
            eprintln!("fittle: {}: {e}", a.folder.display());
            return exit::ERROR;
        }
    };
    let s = fittle_scan::summarize(&a.folder.to_string_lossy(), &entries);
    if a.json {
        let mut v = serde_json::to_value(&s).expect("serializable");
        v["entries"] = serde_json::to_value(&entries).expect("serializable");
        println!(
            "{}",
            serde_json::to_string_pretty(&v).expect("serializable")
        );
        return exit::OK;
    }
    let mut parts = vec![format!("{} files", s.files)];
    parts.extend(s.frames.iter().map(|(k, n)| format!("{n} {k}")));
    if s.stacks > 0 {
        parts.push(format!("{} stack(s)", s.stacks));
    }
    println!("{}", parts.join(" · "));
    if !s.nights.is_empty() {
        println!(
            "{}",
            muted(&format!(
                "{} night(s): {}",
                s.nights.len(),
                s.nights.join(", ")
            ))
        );
    }
    for t in &s.targets {
        let subs: Vec<String> = t.exposures_s.iter().map(|e| format!("{e} s")).collect();
        println!(
            "  {} · {}: {} subs ({}) = {}",
            t.object,
            t.filter,
            t.subs,
            subs.join(", "),
            hours(t.integration_s)
        );
    }
    if s.total_light_s > 0.0 {
        println!("Total light: {}", hours(s.total_light_s));
    }
    for w in &s.warnings {
        println!("{} {w}", warn("!"));
    }
    exit::OK
}
