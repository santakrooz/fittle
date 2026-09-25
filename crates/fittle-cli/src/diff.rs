//! `fittle diff`

use std::io::{self, Write};
use std::path::PathBuf;

use clap::Args as ClapArgs;
use fittle_core::diff::{Impact, Level, Row, Status, diff_files};

use crate::exit;
use crate::fmt::{bad, good, muted, warn};

#[derive(ClapArgs)]
pub struct Args {
    a: PathBuf,
    b: PathBuf,
    /// Show every keyword, not just differences and calibration checks
    #[arg(long)]
    all: bool,
    /// Compare this HDU in both files (default: the image HDU)
    #[arg(long, value_name = "N")]
    hdu: Option<usize>,
    /// Print JSON (schema fittle.diff/1)
    #[arg(long)]
    json: bool,
}

pub fn run(args: Args) -> u8 {
    let (a, b) = (args.a.to_string_lossy(), args.b.to_string_lossy());
    let d = match diff_files(&a, &b, args.hdu) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("fittle: {e}");
            return exit::ERROR;
        }
    };
    let mut out = io::stdout().lock();
    let r = if args.json {
        serde_json::to_writer_pretty(&mut out, &d)
            .map_err(io::Error::from)
            .and_then(|_| writeln!(out))
    } else {
        (|| {
            let name = |p: &str| p.rsplit(['/', '\\']).next().unwrap_or(p).to_string();
            writeln!(
                out,
                "{} {}  {}",
                muted("A"),
                name(&d.a.path),
                muted(&d.a.verdict)
            )?;
            writeln!(
                out,
                "{} {}  {}",
                muted("B"),
                name(&d.b.path),
                muted(&d.b.verdict)
            )?;
            writeln!(out)?;
            let mut rows: Vec<Row> = d
                .rows
                .iter()
                .filter(|r| args.all || r.is_notable())
                .cloned()
                .collect();
            if !args.all {
                // One line for the whole plate solution.
                let wcs: Vec<&Row> = rows.iter().filter(|r| r.group == Some("wcs")).collect();
                if !wcs.is_empty() {
                    let side = |f: fn(&Row) -> &Option<String>| {
                        let n = wcs.iter().filter(|r| f(r).is_some()).count();
                        (n > 0).then(|| format!("solved ({n} keys)"))
                    };
                    let summary = Row {
                        keyword: "WCS".into(),
                        a: side(|r| &r.a),
                        b: side(|r| &r.b),
                        status: Status::Changed,
                        impact: Some(Impact {
                            level: Level::Info,
                            text: "plate solution differs".into(),
                        }),
                        group: None,
                    };
                    rows.retain(|r| r.group != Some("wcs"));
                    rows.push(summary);
                }
                // Impact first (most severe), then other differences, in file order.
                rows.sort_by_key(|r| {
                    std::cmp::Reverse(r.impact.as_ref().map(|i| (i.level != Level::Info, i.level)))
                });
            }
            let w = |s: &Option<String>| s.as_deref().unwrap_or("—").chars().count();
            let wa = rows.iter().map(|r| w(&r.a)).max().unwrap_or(1).clamp(1, 34);
            let wb = rows.iter().map(|r| w(&r.b)).max().unwrap_or(1).clamp(1, 34);
            writeln!(
                out,
                "  {}",
                muted(&format!(
                    "{:<9} {:<wa$}  {:<wb$}  IMPACT",
                    "KEYWORD", "A", "B"
                ))
            )?;
            for r in &rows {
                let cell = |s: &Option<String>, width: usize| {
                    let s = s.as_deref().unwrap_or("—");
                    let t: String = s.chars().take(width).collect();
                    format!("{t:<width$}")
                };
                let impact = r.impact.as_ref().map_or(String::new(), |i| match i.level {
                    Level::Ok => good(&i.text),
                    Level::Info => muted(&i.text),
                    Level::Warning => warn(&i.text),
                    Level::Blocker => bad(&i.text),
                });
                let key = if r.status == Status::Same {
                    muted(&format!("{:<9}", r.keyword))
                } else {
                    format!("{:<9}", r.keyword)
                };
                writeln!(
                    out,
                    "  {key} {}  {}  {impact}",
                    cell(&r.a, wa),
                    cell(&r.b, wb)
                )?;
            }
            writeln!(out)?;
            let summary = format!("{} of {} keywords differ", d.differences, d.rows.len());
            if d.blockers > 0 {
                writeln!(
                    out,
                    "  {} · {}",
                    summary,
                    bad(&format!("{} block calibration", d.blockers))
                )
            } else {
                writeln!(out, "  {summary}")
            }
        })()
    };
    match r {
        Ok(()) => exit::OK,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => exit::OK,
        Err(e) => {
            eprintln!("fittle: {e}");
            exit::ERROR
        }
    }
}
