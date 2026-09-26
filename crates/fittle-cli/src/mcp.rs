//! `fittle mcp` (MCP server on stdio) and `fittle scan` (folder summary).

use std::path::PathBuf;

use clap::Args as ClapArgs;

use crate::exit;
use crate::fmt::{bad, good, muted, warn};

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
    /// Also grade every light sub (stars, HFR, background, trails)
    #[arg(long)]
    grade: bool,
    /// Grading limits, e.g. 'hfr>3.5,stars<50' (implies --grade)
    #[arg(long, value_name = "RULES")]
    reject: Option<String>,
    /// Write a session report: md, html or json (schema fittle.report/1)
    #[arg(long, value_name = "FORMAT")]
    report: Option<String>,
    /// Write an AstroBin acquisition CSV (kept subs when graded)
    #[arg(long)]
    astrobin: bool,
    /// Output file for --report / --astrobin (default: stdout); never overwritten
    #[arg(short, long, value_name = "FILE")]
    out: Option<PathBuf>,
}

fn emit(text: &str, out: Option<&PathBuf>) -> u8 {
    let Some(p) = out else {
        print!("{text}");
        return exit::OK;
    };
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(p)
    {
        Ok(mut f) => {
            use std::io::Write;
            if let Err(e) = f.write_all(text.as_bytes()) {
                eprintln!("fittle: {}: {e}", p.display());
                return exit::ERROR;
            }
            eprintln!("{} {}", good("✓"), p.display());
            exit::OK
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            eprintln!(
                "fittle: {} already exists; Fittle never overwrites",
                p.display()
            );
            exit::VALIDATION
        }
        Err(e) => {
            eprintln!("fittle: {}: {e}", p.display());
            exit::ERROR
        }
    }
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
    let rules = match a
        .reject
        .as_deref()
        .map(fittle_scan::grade::Rules::parse)
        .transpose()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("fittle: {e}");
            return exit::VALIDATION;
        }
    };
    let graded = a.grade || rules.is_some();
    if a.report.is_some() || a.astrobin || graded {
        let rules = rules.unwrap_or_default();
        let r = fittle_scan::report::report(
            &a.folder.to_string_lossy(),
            &entries,
            graded.then_some(&rules),
        );
        if a.astrobin {
            return emit(
                &fittle_scan::report::astrobin_csv(&r, &entries),
                a.out.as_ref(),
            );
        }
        let text = match a.report.as_deref() {
            Some("md" | "markdown") | None => fittle_scan::report::markdown(&r),
            Some("html") => fittle_scan::report::html(&r),
            Some("json") => serde_json::to_string_pretty(&r).expect("serializable") + "\n",
            Some(other) => {
                eprintln!("fittle: report format '{other}': use md, html or json");
                return exit::VALIDATION;
            }
        };
        if a.report.is_some() || a.json {
            return emit(&text, a.out.as_ref());
        }
    }
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

#[derive(ClapArgs)]
pub struct GradeArgs {
    folder: PathBuf,
    /// Include subfolders
    #[arg(short, long)]
    recursive: bool,
    /// Override limits, e.g. 'hfr>3.5,stars<50,bg>0.2,ecc>0.6,trails'
    #[arg(long, value_name = "RULES")]
    reject: Option<String>,
    /// Move suggested rejects into _rejected/ beside them (renames only)
    #[arg(long)]
    move_rejects: bool,
    /// Print JSON (schema fittle.grade/1)
    #[arg(long)]
    json: bool,
}

pub fn run_grade(a: GradeArgs) -> u8 {
    let rules = match a
        .reject
        .as_deref()
        .map(fittle_scan::grade::Rules::parse)
        .transpose()
    {
        Ok(r) => r.unwrap_or_default(),
        Err(e) => {
            eprintln!("fittle: {e}");
            return exit::VALIDATION;
        }
    };
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
    let g = fittle_scan::grade::grade(&a.folder.to_string_lossy(), &entries, &rules);
    let moves = a.move_rejects.then(|| {
        let paths: Vec<&str> = g
            .subs
            .iter()
            .filter(|s| s.reject)
            .map(|s| s.path.as_str())
            .collect();
        fittle_scan::grade::move_rejects(&paths, false)
    });
    if a.json {
        let mut v = serde_json::to_value(&g).expect("serializable");
        if let Some(m) = &moves {
            v["moves"] = serde_json::to_value(m).expect("serializable");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&v).expect("serializable")
        );
        return exit::OK;
    }
    println!(
        "{} subs graded in {:.1} s · kept {} · {} · median HFR {}",
        g.subs.len(),
        g.elapsed_ms as f64 / 1000.0,
        good(&g.kept.to_string()),
        if g.rejected > 0 {
            bad(&format!("{} suggested rejects", g.rejected))
        } else {
            "no rejects".into()
        },
        g.median_hfr.map_or("—".into(), |h| format!("{h:.2} px")),
    );
    println!(
        "Captured {} · usable {}",
        hours(g.captured_s),
        hours(g.usable_s)
    );
    for t in &g.groups {
        println!(
            "  {} · {} · {}: {} subs, limits HFR > {} · stars < {:.0} · background > {:.4}",
            t.object,
            t.filter,
            t.exposure_s.map_or("?".into(), |e| format!("{e} s")),
            t.subs,
            t.hfr_max.map_or("—".into(), |h| format!("{h:.2}")),
            t.stars_min,
            t.background_max
        );
    }
    let mut worst: Vec<&fittle_scan::grade::SubGrade> =
        g.subs.iter().filter(|s| !s.reasons.is_empty()).collect();
    worst.sort_by(|a, b| b.badness.total_cmp(&a.badness));
    if !worst.is_empty() {
        println!("{}", muted("Worst subs:"));
    }
    for s in worst.iter().take(10) {
        let st = s.stats.as_ref();
        println!(
            "  {} {:<48} stars {:>4}  HFR {:>5}  bkg {:.4}  {}",
            if s.reject { bad("✗") } else { warn("!") },
            s.name,
            st.map_or(0, |x| x.stars),
            st.and_then(|x| x.hfr)
                .map_or("—".into(), |h| format!("{h:.2}")),
            st.map_or(0.0, |x| x.background),
            s.reasons
                .iter()
                .map(|r| serde_json::to_value(r)
                    .ok()
                    .and_then(|v| v.as_str().map(|s| s.replace('_', " ")))
                    .unwrap_or_default())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if let Some(m) = moves {
        let ok = m.iter().filter(|m| m.error.is_none()).count();
        println!("Moved {ok} of {} rejects into _rejected/", m.len());
        for e in m.iter().filter(|m| m.error.is_some()) {
            eprintln!("  {}: {}", e.from, e.error.as_deref().unwrap_or(""));
        }
    } else if g.rejected > 0 {
        println!(
            "{}",
            muted("Run again with --move-rejects to move them into _rejected/.")
        );
    }
    exit::OK
}
