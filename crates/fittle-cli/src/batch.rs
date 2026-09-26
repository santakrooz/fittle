//! `fittle keys` (how keywords vary across files) and `fittle rig`
//! (saved sets of header values, applied as edits).

use std::path::PathBuf;

use clap::{Args as ClapArgs, Subcommand};
use fittle_core::edit::Options;
use fittle_scan::batch::{self, Spread};

use crate::exit;
use crate::fmt::{bad, good, muted, warn};

#[derive(ClapArgs)]
pub struct KeysArgs {
    /// Files and/or folders (folders include subfolders)
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
    /// Only keywords that differ between files
    #[arg(long)]
    differ: bool,
    /// Print JSON (schema fittle.batch/1)
    #[arg(long)]
    json: bool,
}

pub fn run_keys(a: KeysArgs) -> u8 {
    let paths = match batch::resolve(&a.inputs) {
        Ok(p) if !p.is_empty() => p,
        Ok(_) => {
            eprintln!("fittle: no FITS files");
            return exit::VALIDATION;
        }
        Err(e) => {
            eprintln!("fittle: {e}");
            return exit::ERROR;
        }
    };
    let d = batch::distribution(&paths);
    if a.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&d).expect("serializable")
        );
        return exit::OK;
    }
    println!(
        "{} files{}",
        d.files,
        if d.unreadable.is_empty() {
            String::new()
        } else {
            format!(", {} unreadable", d.unreadable.len())
        }
    );
    for k in d
        .keys
        .iter()
        .filter(|k| !a.differ || k.spread != Spread::Same || k.present < d.files)
    {
        let vals = match k.spread {
            Spread::Same => k.values[0].text.clone(),
            Spread::Range => format!("{} – {}", k.min.unwrap_or(0.0), k.max.unwrap_or(0.0)),
            Spread::Unique => format!("{} … ({} distinct)", k.values[0].text, k.distinct),
            Spread::Mixed => k
                .values
                .iter()
                .map(|v| format!("{} ({})", v.text, v.count))
                .collect::<Vec<_>>()
                .join(" · "),
        };
        let tag = match k.spread {
            Spread::Same => good("same"),
            Spread::Mixed => bad("mixed"),
            Spread::Range => warn("range"),
            Spread::Unique => muted("unique"),
        };
        let missing = if k.present < d.files {
            warn(&format!(" (missing on {})", d.files - k.present))
        } else {
            String::new()
        };
        println!("  {:<9} {tag:<8} {vals}{missing}", k.keyword);
    }
    exit::OK
}

#[derive(ClapArgs)]
pub struct RigArgs {
    #[command(subcommand)]
    cmd: RigCmd,
}

#[derive(Subcommand)]
enum RigCmd {
    /// List built-in and saved rigs
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show one rig's values
    Show { name: String },
    /// Save a rig from a file's header (default keys: scope, camera, optics, filter, gain)
    Save {
        name: String,
        #[arg(long, value_name = "FILE")]
        from: PathBuf,
        /// Keywords to capture, comma-separated
        #[arg(long, value_delimiter = ',')]
        keys: Vec<String>,
    },
    /// Delete a saved rig
    Delete { name: String },
    /// Set a rig's values on files (dry run unless --apply)
    Apply {
        name: String,
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Write the changes (default: show the plan)
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        no_backup: bool,
        #[arg(long)]
        json: bool,
    },
}

pub fn run_rig(a: RigArgs) -> u8 {
    use fittle_core::rigs;
    match a.cmd {
        RigCmd::List { json } => {
            let all = rigs::all();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&all).expect("serializable")
                );
            } else {
                for r in all {
                    println!(
                        "{}{}",
                        r.name,
                        if r.builtin {
                            muted("  (built-in)")
                        } else {
                            String::new()
                        }
                    );
                }
            }
            exit::OK
        }
        RigCmd::Show { name } => match rigs::find(&name) {
            Some(r) => {
                for (k, v) in &r.values {
                    println!("{k:<9} {v}");
                }
                exit::OK
            }
            None => {
                eprintln!("fittle: no rig named '{name}' (see `fittle rig list`)");
                exit::VALIDATION
            }
        },
        RigCmd::Save { name, from, keys } => {
            let fits = match fittle_core::Fits::open(&from) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("fittle: {}: {e}", from.display());
                    return exit::ERROR;
                }
            };
            let h = fits.hdus[fittle_core::edit::default_hdu(&fits)].header();
            let keys: Vec<&str> = if keys.is_empty() {
                rigs::RIG_KEYS.to_vec()
            } else {
                keys.iter().map(String::as_str).collect()
            };
            let rig = rigs::from_header(&name, h, &keys);
            if rig.values.is_empty() {
                eprintln!("fittle: none of those keywords are in {}", from.display());
                return exit::VALIDATION;
            }
            match rigs::save(rig.clone()) {
                Ok(()) => {
                    println!(
                        "{} saved '{name}': {}",
                        good("✓"),
                        rig.values.keys().cloned().collect::<Vec<_>>().join(", ")
                    );
                    exit::OK
                }
                Err(e) => {
                    eprintln!("fittle: {e}");
                    exit::VALIDATION
                }
            }
        }
        RigCmd::Delete { name } => match rigs::delete(&name) {
            Ok(true) => exit::OK,
            Ok(false) => {
                eprintln!("fittle: no saved rig named '{name}'");
                exit::VALIDATION
            }
            Err(e) => {
                eprintln!("fittle: {e}");
                exit::ERROR
            }
        },
        RigCmd::Apply {
            name,
            inputs,
            apply,
            no_backup,
            json,
        } => {
            let Some(rig) = rigs::find(&name) else {
                eprintln!("fittle: no rig named '{name}'");
                return exit::VALIDATION;
            };
            let ops = match rig.ops() {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("fittle: {e}");
                    return exit::VALIDATION;
                }
            };
            let paths = match batch::resolve(&inputs) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("fittle: {e}");
                    return exit::ERROR;
                }
            };
            let opts = Options {
                backup: !no_backup,
                history: true,
                checksum: true,
                hdu: None,
            };
            let plan = batch::plan_batch(&paths, &ops, &opts);
            if json && !apply {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&plan).expect("serializable")
                );
                return if plan.errors.is_empty() {
                    exit::OK
                } else {
                    exit::VALIDATION
                };
            }
            for (p, e) in &plan.errors {
                eprintln!("{} {p}: {e}", bad("✗"));
            }
            if !plan.errors.is_empty() {
                return exit::VALIDATION;
            }
            println!(
                "{} of {} files change ({} in place, {} rewritten); backups {:.1} MB; ~{:.0} s",
                plan.changed,
                plan.files,
                plan.in_place,
                plan.rewrite,
                plan.backup_bytes as f64 / 1e6,
                plan.est_seconds.ceil()
            );
            for n in &plan.notes {
                println!("  {} {n}", warn("!"));
            }
            if !apply {
                println!("{}", muted("Dry run. Add --apply to write."));
                return exit::OK;
            }
            let errs = batch::apply_batch(&paths, &ops, &opts);
            for (p, e) in &errs {
                eprintln!("{} {p}: {e}", bad("✗"));
            }
            if errs.is_empty() {
                println!("{} wrote {} files", good("✓"), plan.changed);
                exit::OK
            } else {
                exit::ERROR
            }
        }
    }
}
