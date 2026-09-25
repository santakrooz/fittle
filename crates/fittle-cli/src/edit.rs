//! `fittle set | unset | rename-key | scrub`
//!
//! Every file is planned and validated before any is written, so a bad value
//! never leaves a folder half-edited. `--dry-run` shows the diff only.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;
use fittle_core::edit::{
    ChangeKind, EditError, NewValue, Op, Options, Plan, check_keyword, parse_assignment, plan,
};
use fittle_core::write::{Method, WriteReport, apply};
use serde::Serialize;

use crate::exit;
use crate::fmt::{bad, good, muted, warn};

/// Flags shared by every writing command.
#[derive(ClapArgs, Clone)]
pub struct WriteFlags {
    /// Show what would change; write nothing
    #[arg(long)]
    dry_run: bool,
    /// Don't keep a .bak copy of the original
    #[arg(long)]
    no_backup: bool,
    /// Don't add a HISTORY line describing the change
    #[arg(long)]
    no_history: bool,
    /// Don't recompute CHECKSUM/DATASUM
    #[arg(long)]
    no_checksum: bool,
    /// Edit this HDU (default: the image HDU)
    #[arg(long, value_name = "N")]
    hdu: Option<usize>,
    /// Print JSON (schema fittle.edit/1)
    #[arg(long)]
    json: bool,
}

impl WriteFlags {
    fn options(&self) -> Options {
        Options {
            backup: !self.no_backup,
            history: !self.no_history,
            checksum: !self.no_checksum,
            hdu: self.hdu,
        }
    }
}

#[derive(ClapArgs)]
pub struct SetArgs {
    /// Files, then KEY=VALUE assignments (value type is inferred; quote with '…' for a string)
    #[arg(required = true, value_name = "FILES… KEY=VALUE…")]
    args: Vec<String>,
    /// Comment for the keywords being set
    #[arg(long)]
    comment: Option<String>,
    /// Apply a template: JSON object or KEY=VALUE lines (# comments allowed)
    #[arg(long, value_name = "FILE")]
    from: Option<PathBuf>,
    #[command(flatten)]
    flags: WriteFlags,
}

#[derive(ClapArgs)]
pub struct UnsetArgs {
    /// Files, then keywords to remove
    #[arg(required = true, value_name = "FILES… KEY…")]
    args: Vec<String>,
    #[command(flatten)]
    flags: WriteFlags,
}

#[derive(ClapArgs)]
pub struct RenameArgs {
    /// Files, then OLD and NEW keyword names
    #[arg(required = true, num_args = 3.., value_name = "FILES… OLD NEW")]
    args: Vec<String>,
    #[command(flatten)]
    flags: WriteFlags,
}

#[derive(ClapArgs)]
pub struct ScrubArgs {
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Remove site coordinates, observer names and serial numbers
    #[arg(long, required = true)]
    privacy: bool,
    #[command(flatten)]
    flags: WriteFlags,
}

fn invalid(msg: impl std::fmt::Display) -> u8 {
    eprintln!("fittle: {msg}");
    exit::VALIDATION
}

/// Existing paths are files; everything else must parse as the command's operands.
fn split_files(args: &[String]) -> (Vec<PathBuf>, Vec<String>) {
    let (files, rest): (Vec<&String>, Vec<&String>) =
        args.iter().partition(|a| Path::new(a.as_str()).is_file());
    (
        files.into_iter().map(PathBuf::from).collect(),
        rest.into_iter().cloned().collect(),
    )
}

fn template(path: &Path) -> Result<Vec<(String, NewValue)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&text) {
        return map
            .into_iter()
            .map(|(k, v)| {
                let key = k.to_ascii_uppercase();
                check_keyword(&key).map_err(|e| e.to_string())?;
                let value = match v {
                    serde_json::Value::Bool(b) => NewValue::Logical(b),
                    serde_json::Value::Number(n) if n.is_i64() => {
                        NewValue::Integer(n.as_i64().unwrap())
                    }
                    serde_json::Value::Number(n) => NewValue::Float(n.as_f64().unwrap_or(0.0)),
                    serde_json::Value::String(s) => NewValue::String(s),
                    other => return Err(format!("{key}: unsupported value {other}")),
                };
                Ok((key, value))
            })
            .collect();
    }
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            parse_assignment(l).ok_or_else(|| format!("{}: not KEY=VALUE: {l}", path.display()))
        })
        .collect()
}

pub fn run_set(a: SetArgs) -> u8 {
    let (files, rest) = split_files(&a.args);
    let mut assignments = Vec::new();
    if let Some(t) = &a.from {
        match template(t) {
            Ok(mut list) => assignments.append(&mut list),
            Err(e) => return invalid(e),
        }
    }
    for r in &rest {
        match parse_assignment(r) {
            Some(kv) => assignments.push(kv),
            None => return invalid(format!("{r:?} is neither an existing file nor KEY=VALUE")),
        }
    }
    if assignments.is_empty() {
        return invalid("nothing to set: give KEY=VALUE or --from FILE");
    }
    let ops: Vec<Op> = assignments
        .into_iter()
        .map(|(key, value)| Op::Set {
            key,
            value,
            comment: a.comment.clone(),
        })
        .collect();
    run(&files, |_| Ok(ops.clone()), &a.flags)
}

pub fn run_unset(a: UnsetArgs) -> u8 {
    let (files, keys) = split_files(&a.args);
    let mut ops = Vec::new();
    for k in keys {
        let key = k.to_ascii_uppercase();
        if let Err(e) = check_keyword(&key) {
            return invalid(format!(
                "{k:?} is neither an existing file nor a keyword ({e})"
            ));
        }
        ops.push(Op::Unset { key });
    }
    if ops.is_empty() {
        return invalid("no keywords to remove");
    }
    run(&files, |_| Ok(ops.clone()), &a.flags)
}

pub fn run_rename(a: RenameArgs) -> u8 {
    let n = a.args.len();
    let (files, from, to) = (&a.args[..n - 2], &a.args[n - 2], &a.args[n - 1]);
    let (files, stray) = split_files(files);
    if let Some(s) = stray.first() {
        return invalid(format!("{s:?} is not an existing file"));
    }
    let op = Op::Rename {
        from: from.clone(),
        to: to.clone(),
    };
    run(&files, |_| Ok(vec![op.clone()]), &a.flags)
}

pub fn run_scrub(a: ScrubArgs) -> u8 {
    let _ = a.privacy;
    run(
        &a.files,
        |p| {
            let fits = fittle_core::Fits::open(p).map_err(|e| e.to_string())?;
            Ok(fittle_core::privacy::scrub_ops(&fits, &p.to_string_lossy()))
        },
        &a.flags,
    )
}

#[derive(Serialize)]
struct Doc<'a, T: Serialize> {
    schema: &'static str,
    dry_run: bool,
    files: &'a [T],
}

fn run(
    files: &[PathBuf],
    ops_for: impl Fn(&Path) -> Result<Vec<Op>, String>,
    flags: &WriteFlags,
) -> u8 {
    if files.is_empty() {
        return invalid("no files given");
    }
    let opts = flags.options();
    // Phase 1: plan and validate everything; write nothing on any error.
    let mut plans: Vec<(PathBuf, Vec<Op>, Plan)> = Vec::new();
    for f in files {
        let ops = match ops_for(f) {
            Ok(o) => o,
            Err(e) => return invalid(e),
        };
        match plan(f, &ops, &opts) {
            Ok(p) => plans.push((f.clone(), ops, p)),
            Err(EditError::Invalid(e)) => return invalid(format!("{}: {e}", f.display())),
            Err(e) => {
                eprintln!("fittle: {}: {e}", f.display());
                return exit::ERROR;
            }
        }
    }
    let mut out = io::stdout().lock();
    if flags.dry_run {
        let list: Vec<&Plan> = plans.iter().map(|(_, _, p)| p).collect();
        let r = if flags.json {
            serde_json::to_writer_pretty(
                &mut out,
                &Doc {
                    schema: "fittle.edit/1",
                    dry_run: true,
                    files: &list,
                },
            )
            .map_err(io::Error::from)
            .and_then(|_| writeln!(out))
        } else {
            list.iter()
                .try_for_each(|p| write_plan(&mut out, p, None))
                .and_then(|_| {
                    let n = list.iter().filter(|p| !p.changes.is_empty()).count();
                    writeln!(
                        out,
                        "{}",
                        muted(&format!(
                            "dry run: {n} of {} file(s) would change; nothing written",
                            list.len()
                        ))
                    )
                })
        };
        return if r.is_ok() { exit::OK } else { exit::ERROR };
    }

    // Phase 2: write.
    let mut reports: Vec<WriteReport> = Vec::new();
    let mut code = exit::OK;
    for (f, ops, _) in &plans {
        match apply(f, ops, &opts) {
            Ok(r) => {
                if !flags.json {
                    let _ = write_plan(&mut out, &r.plan, Some(&r));
                }
                reports.push(r);
            }
            Err(e) => {
                eprintln!("fittle: {}: {}", f.display(), bad(&e.to_string()));
                code = exit::ERROR;
            }
        }
    }
    if flags.json {
        let _ = serde_json::to_writer_pretty(
            &mut out,
            &Doc {
                schema: "fittle.edit/1",
                dry_run: false,
                files: &reports,
            },
        );
        let _ = writeln!(out);
    } else if plans.len() > 1 {
        let written = reports.iter().filter(|r| r.method != Method::None).count();
        let _ = writeln!(
            out,
            "{} file(s): {written} written, {} unchanged",
            plans.len(),
            reports.len() - written
        );
    }
    code
}

fn write_plan(out: &mut impl Write, p: &Plan, r: Option<&WriteReport>) -> io::Result<()> {
    let name = p.path.rsplit(['/', '\\']).next().unwrap_or(&p.path);
    let how = if p.changes.is_empty() {
        "no changes".to_string()
    } else if p.in_place {
        format!(
            "in place (header stays {} block{})",
            p.header_blocks_after,
            if p.header_blocks_after == 1 { "" } else { "s" }
        )
    } else {
        format!(
            "rewrite (header {} → {} blocks)",
            p.header_blocks_before, p.header_blocks_after
        )
    };
    writeln!(out, "{name} {}", muted(&format!("· HDU {} · {how}", p.hdu)))?;
    for c in &p.changes {
        match c.kind {
            ChangeKind::Added => writeln!(
                out,
                "  {}",
                good(&format!(
                    "+ {} = {}",
                    c.key,
                    c.after.as_deref().unwrap_or("")
                ))
            )?,
            ChangeKind::Removed => writeln!(
                out,
                "  {}",
                bad(&format!(
                    "− {} = {}",
                    c.key,
                    c.before.as_deref().unwrap_or("")
                ))
            )?,
            ChangeKind::Modified => {
                writeln!(
                    out,
                    "  {}",
                    bad(&format!(
                        "− {} = {}",
                        c.key,
                        c.before.as_deref().unwrap_or("")
                    ))
                )?;
                writeln!(
                    out,
                    "  {}",
                    good(&format!(
                        "+ {} = {}",
                        c.key,
                        c.after.as_deref().unwrap_or("")
                    ))
                )?;
            }
            ChangeKind::Renamed => {
                writeln!(
                    out,
                    "  {}",
                    bad(&format!("− {}", c.before.as_deref().unwrap_or("")))
                )?;
                writeln!(
                    out,
                    "  {}",
                    good(&format!("+ {}", c.after.as_deref().unwrap_or("")))
                )?;
            }
            ChangeKind::History => writeln!(
                out,
                "  {}",
                good(&format!("+ HISTORY {}", c.after.as_deref().unwrap_or("")))
            )?,
        }
    }
    for w in &p.warnings {
        writeln!(out, "  {} {w}", warn("⚠"))?;
    }
    for c in &p.consequences {
        writeln!(out, "  {} {}", warn("!"), muted(c))?;
    }
    if let Some(r) = r {
        if r.method != Method::None {
            let mut parts = vec![format!(
                "data unchanged (sha256 {}…)",
                &r.untouched_sha256[..12]
            )];
            if let Some(c) = &r.checksum {
                parts.push(format!("CHECKSUM {c}"));
            }
            if let Some(b) = &r.backup {
                parts.push(format!(
                    "backup {}",
                    b.rsplit(['/', '\\']).next().unwrap_or(b)
                ));
            }
            writeln!(
                out,
                "  {} {}",
                good("✓ written"),
                muted(&format!("· {}", parts.join(" · ")))
            )?;
        }
    }
    Ok(())
}
