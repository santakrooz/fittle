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
    /// Print setup for Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf
    #[arg(long)]
    setup: bool,
    /// Start the server once, list its tools, and stop (checks the setup works)
    #[arg(long)]
    check: bool,
    /// With --setup / --check: print JSON
    #[arg(long)]
    json: bool,
}

pub fn run_mcp(a: McpArgs) -> u8 {
    if a.setup || a.check {
        let bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fittle"));
        let roots: Vec<String> = if a.root.is_empty() {
            fittle_mcp::Roots::new(vec![])
                .dirs()
                .iter()
                .map(|d| d.to_string_lossy().into_owned())
                .collect()
        } else {
            a.root
                .iter()
                .map(|r| {
                    r.canonicalize()
                        .unwrap_or(r.clone())
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        };
        if a.check {
            let r = fittle_mcp::setup::self_test(&bin, &roots);
            if a.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&r).expect("serializable")
                );
            } else if r.ok {
                println!(
                    "{} {} answered with {} tools in {} ms",
                    good("✓"),
                    r.server.as_deref().unwrap_or("fittle"),
                    r.tools.len(),
                    r.elapsed_ms
                );
            } else {
                eprintln!("{} {}", bad("✗"), r.error.as_deref().unwrap_or("failed"));
            }
            return if r.ok { exit::OK } else { exit::ERROR };
        }
        let s = fittle_mcp::setup::snippets(&bin, &roots);
        if a.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&s).expect("serializable")
            );
            return exit::OK;
        }
        println!(
            "{}",
            muted(
                "Fittle's MCP server runs locally over stdio; clients start it with this command (no URL)."
            )
        );
        for x in s {
            println!("\n{}  {}", good(x.client), muted(&x.how));
            if let Some(f) = &x.file {
                println!("  {}", muted(&format!("file: {f}")));
            }
            for line in x.text.lines() {
                println!("  {line}");
            }
        }
        return exit::OK;
    }
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

#[derive(ClapArgs)]
pub struct MatchArgs {
    /// Folder of lights (subfolders included)
    lights: PathBuf,
    /// Calibration library folder (subfolders included)
    #[arg(long, value_name = "DIR")]
    library: PathBuf,
    /// Print JSON (schema fittle.calmatch/1)
    #[arg(long)]
    json: bool,
}

pub fn run_match(a: MatchArgs) -> u8 {
    use fittle_scan::calmatch::{GroupStatus, KindMatch, MatchStatus};
    let list =
        |p: &PathBuf| fittle_scan::list_recursive(p).map_err(|e| format!("{}: {e}", p.display()));
    let (lights, library) = match (list(&a.lights), list(&a.library)) {
        (Ok(l), Ok(c)) => (l, c),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("fittle: {e}");
            return exit::ERROR;
        }
    };
    let m = fittle_scan::calmatch::match_calibration(
        &a.lights.to_string_lossy(),
        &lights,
        &a.library.to_string_lossy(),
        &library,
    );
    if a.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&m).expect("serializable")
        );
        return exit::OK;
    }
    println!(
        "{} light group(s) · {} ready · {} partial · {} missing · {} calibration frames in {} sets",
        m.groups.len(),
        good(&m.ready.to_string()),
        warn(&m.partial.to_string()),
        bad(&m.missing.to_string()),
        m.library_frames,
        m.sets
    );
    let cell = |k: &KindMatch| -> String {
        let name = k
            .set
            .as_ref()
            .map_or("none".to_string(), |s| s.name.clone());
        match k.status {
            MatchStatus::Ok => good(&name),
            MatchStatus::Warn => warn(&format!("{name} ({})", k.notes.join("; "))),
            MatchStatus::Bad => bad(&format!("{name} ({})", k.notes.join("; "))),
            MatchStatus::None => bad("none"),
        }
    };
    for g in &m.groups {
        let status = match g.status {
            GroupStatus::Ready => good("Ready"),
            GroupStatus::Partial => warn("Partial"),
            GroupStatus::Missing => bad("Missing"),
        };
        let mut head = Vec::new();
        if let Some(x) = g.gain {
            head.push(format!("Gain {x}"));
        }
        if let Some(x) = g.exposure_s {
            head.push(format!("{x} s"));
        }
        if let Some(x) = &g.filter {
            head.push(x.clone());
        }
        head.push(format!("{} subs", g.subs));
        println!("\n{}  {status}", head.join(" · "));
        println!("  darks  {}", cell(&g.darks));
        println!("  flats  {}", cell(&g.flats));
        println!("  bias   {}", cell(&g.bias));
        if let Some(w) = &g.why {
            println!("  {}", muted(w));
        }
    }
    exit::OK
}

#[derive(ClapArgs)]
pub struct OrganizeArgs {
    /// Folder to organize (subfolders included)
    folder: Option<PathBuf>,
    /// Folder template, e.g. 'object/filter/night' or '{object}/{filter}/{night}'
    #[arg(long, value_name = "TEMPLATE")]
    by: Option<String>,
    /// Also rename files, e.g. '{object}_{filter}_{exptime}s_{seq}'
    #[arg(long, value_name = "TEMPLATE")]
    rename: Option<String>,
    /// Show the plan; move nothing
    #[arg(long)]
    dry_run: bool,
    /// Reverse a previous run from its fittle-organize-*.json manifest
    #[arg(long, value_name = "MANIFEST", conflicts_with_all = ["folder", "by", "rename"])]
    undo: Option<PathBuf>,
    /// Print JSON (schema fittle.organize/1)
    #[arg(long)]
    json: bool,
}

#[derive(ClapArgs)]
pub struct RenameArgs {
    /// FITS files to rename in place
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// File-name template (extension kept), e.g. '{object}_{filter}_{exptime}s_{seq}'
    #[arg(long, value_name = "TEMPLATE")]
    template: String,
    /// Show the plan; rename nothing
    #[arg(long)]
    dry_run: bool,
    /// Print JSON (schema fittle.organize/1)
    #[arg(long)]
    json: bool,
}

/// `object/filter/night` → `{object}/{filter}/{night}`; templates pass through.
fn folder_template(by: &str) -> String {
    by.split('/')
        .map(|seg| {
            let seg = seg.trim();
            if seg.contains('{') || seg.is_empty() {
                seg.to_string()
            } else {
                format!("{{{seg}}}")
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn show_plan(p: &fittle_scan::organize::Plan, dry_run: bool, json: bool) -> u8 {
    if json {
        println!("{}", serde_json::to_string_pretty(p).expect("serializable"));
        return exit::OK;
    }
    let root = std::path::Path::new(&p.root);
    let rel = |s: &str| {
        std::path::Path::new(s)
            .strip_prefix(root)
            .map_or(s.to_string(), |r| r.to_string_lossy().into_owned())
    };
    let failed = p.moves.iter().filter(|m| m.error.is_some()).count();
    for m in p
        .moves
        .iter()
        .filter(|m| !m.companion)
        .take(if dry_run { 20 } else { 0 })
    {
        println!("  {} {} {}", rel(&m.from), muted("→"), rel(&m.to));
    }
    let files = p.moves.iter().filter(|m| !m.companion).count();
    let comp = p.moves.len() - files;
    println!(
        "{} {} file(s){}, {} already in place, {} new folder(s)",
        if dry_run { "Would move" } else { "Moved" },
        files
            - if dry_run {
                0
            } else {
                p.moves
                    .iter()
                    .filter(|m| !m.companion && m.error.is_some())
                    .count()
            },
        if comp > 0 {
            format!(" and {comp} companion file(s)")
        } else {
            String::new()
        },
        p.unchanged,
        p.new_folders.len()
    );
    if !p.missing_tokens.is_empty() {
        println!(
            "{} some files lack {}; written as 'unknown'",
            warn("!"),
            p.missing_tokens
                .iter()
                .map(|t| format!("{{{t}}}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    for m in p.moves.iter().filter(|m| m.error.is_some()) {
        eprintln!(
            "{} {}: {}",
            bad("✗"),
            rel(&m.from),
            m.error.as_deref().unwrap_or("")
        );
    }
    if let Some(man) = &p.manifest {
        println!("Undo with: fittle organize --undo \"{man}\"");
    }
    if dry_run && files > 0 {
        println!("{}", muted("Run again without --dry-run to move them."));
    }
    if failed > 0 { exit::ERROR } else { exit::OK }
}

pub fn run_organize(a: OrganizeArgs) -> u8 {
    use fittle_scan::organize::{Spec, apply, plan, undo_plan};
    if let Some(m) = &a.undo {
        return match undo_plan(m) {
            Ok(p) if a.dry_run => show_plan(&p, true, a.json),
            Ok(p) => show_plan(&apply(p), false, a.json),
            Err(e) => {
                eprintln!("fittle: {}: {e}", m.display());
                exit::ERROR
            }
        };
    }
    let Some(folder) = a.folder else {
        eprintln!("fittle: give a folder (or --undo <manifest>)");
        return exit::VALIDATION;
    };
    if a.by.is_none() && a.rename.is_none() {
        eprintln!("fittle: give --by and/or --rename");
        return exit::VALIDATION;
    }
    let entries = match fittle_scan::list_recursive(&folder) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("fittle: {}: {e}", folder.display());
            return exit::ERROR;
        }
    };
    let spec = Spec {
        by: a.by.as_deref().map(folder_template).unwrap_or_default(),
        rename: a.rename,
    };
    let root = folder.canonicalize().unwrap_or(folder);
    let p = plan(&root, &entries, &spec);
    if a.dry_run {
        show_plan(&p, true, a.json)
    } else {
        show_plan(&apply(p), false, a.json)
    }
}

pub fn run_rename(a: RenameArgs) -> u8 {
    use fittle_scan::organize::{Spec, apply, plan};
    let entries: Vec<fittle_scan::Entry> = match a
        .files
        .iter()
        .map(|f| {
            fittle_scan::entry_for(f)
                .ok_or_else(|| format!("{}: not a readable FITS file", f.display()))
        })
        .collect::<Result<_, _>>()
    {
        Ok(e) => e,
        Err(e) => {
            eprintln!("fittle: {e}");
            return exit::VALIDATION;
        }
    };
    let root = a.files[0]
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let p = plan(
        &root,
        &entries,
        &Spec {
            by: String::new(),
            rename: Some(a.template),
        },
    );
    if a.dry_run {
        show_plan(&p, true, a.json)
    } else {
        show_plan(&apply(p), false, a.json)
    }
}
