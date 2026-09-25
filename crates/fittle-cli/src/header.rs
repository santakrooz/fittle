//! `fittle header`

use std::io::{self, Write};
use std::path::PathBuf;

use clap::Args as ClapArgs;
use fittle_core::dict::{self, Group};
use fittle_core::{Card, Fits, HEADER_SCHEMA, Hdu, HduKind, HeaderDoc, Severity, Value};

use crate::exit;

#[derive(ClapArgs)]
pub struct Args {
    /// FITS file to read
    file: PathBuf,
    /// Print the exact 80-character records in file order
    #[arg(long, conflicts_with = "json")]
    raw: bool,
    /// Print JSON (schema fittle.header/1)
    #[arg(long)]
    json: bool,
    /// Only this HDU (0 = primary)
    #[arg(long, value_name = "N")]
    hdu: Option<usize>,
    /// Only keywords containing this text (case-insensitive)
    #[arg(long, value_name = "KEY")]
    grep: Option<String>,
}

pub fn run(args: Args) -> u8 {
    let fits = match Fits::open(&args.file) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("fittle: {}: {e}", args.file.display());
            return exit::ERROR;
        }
    };
    if let Some(n) = args.hdu {
        if n >= fits.hdus.len() {
            eprintln!(
                "fittle: --hdu {n} out of range; file has {} HDU(s)",
                fits.hdus.len()
            );
            return exit::VALIDATION;
        }
    }
    let view = fits.filtered(args.hdu, args.grep.as_deref());
    let path = args.file.to_string_lossy();

    let mut out = io::stdout().lock();
    let result = if args.json {
        let doc = HeaderDoc {
            schema: HEADER_SCHEMA,
            path: &path,
            fits: &view,
        };
        serde_json::to_writer_pretty(&mut out, &doc)
            .map_err(io::Error::from)
            .and_then(|_| writeln!(out))
    } else if args.raw {
        write_raw(&mut out, &view)
    } else {
        write_grouped(&mut out, &path, &view)
    };
    match result {
        Ok(()) => exit::OK,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => exit::OK,
        Err(e) => {
            eprintln!("fittle: {e}");
            exit::ERROR
        }
    }
}

fn write_raw(out: &mut impl Write, fits: &Fits) -> io::Result<()> {
    let many = fits.hdus.len() > 1;
    for hdu in &fits.hdus {
        if many {
            writeln!(out, "# HDU {}", hdu.index)?;
        }
        for card in &hdu.cards.cards {
            for line in &card.raw {
                writeln!(out, "{line}")?;
            }
        }
        writeln!(out, "{:<80}", "END")?;
    }
    Ok(())
}

fn write_grouped(out: &mut impl Write, path: &str, fits: &Fits) -> io::Result<()> {
    let n = fits.hdus.len();
    writeln!(
        out,
        "{path} · {} · {n} HDU{}",
        bytes(fits.file_bytes),
        if n == 1 { "" } else { "s" }
    )?;
    for hdu in &fits.hdus {
        writeln!(out)?;
        writeln!(out, "{}", hdu_summary(hdu))?;
        let (commentary, values): (Vec<&Card>, Vec<&Card>) = hdu
            .cards
            .cards
            .iter()
            .partition(|c| c.value.is_commentary());
        for group in Group::ALL {
            let cards: Vec<&Card> = values
                .iter()
                .copied()
                .filter(|c| group_of(c) == group)
                .collect();
            if cards.is_empty() {
                continue;
            }
            writeln!(out, "  {}", group.label().to_uppercase())?;
            for c in cards {
                writeln!(out, "    {}", value_line(c))?;
            }
        }
        let notes: Vec<&Card> = commentary
            .into_iter()
            .filter(|c| !c.value_text.is_empty())
            .collect();
        if !notes.is_empty() {
            writeln!(out, "  HISTORY & COMMENTS")?;
            for c in notes {
                let key = if c.keyword.is_empty() {
                    "(blank)"
                } else {
                    &c.keyword
                };
                writeln!(out, "    {key:<8}  {}", c.value_text)?;
            }
        }
    }
    if !fits.issues.is_empty() {
        writeln!(out)?;
        writeln!(out, "ISSUES")?;
        for i in &fits.issues {
            let mark = match i.severity {
                Severity::Error => "✗",
                Severity::Warning => "⚠",
                Severity::Info => "·",
            };
            let at = match (i.hdu, i.record) {
                (Some(h), Some(r)) => format!(" (HDU {h}, record {r})"),
                (Some(h), None) => format!(" (HDU {h})"),
                _ => String::new(),
            };
            writeln!(out, "  {mark} {}{at}: {}", i.code, i.message)?;
        }
    }
    Ok(())
}

fn group_of(c: &Card) -> Group {
    dict::lookup(&c.keyword).map_or(Group::Other, |i| i.group)
}

fn value_line(c: &Card) -> String {
    let info = dict::lookup(&c.keyword);
    let mut value = match &c.value {
        Value::String(s) => s.clone(),
        Value::Logical(b) => if *b { "T" } else { "F" }.into(),
        Value::Undefined => "(undefined)".into(),
        _ => c.value_text.clone(),
    };
    if let Some(unit) = info.and_then(|i| i.unit) {
        if matches!(c.value, Value::Integer(_) | Value::Float(_)) {
            value = format!("{value} {unit}");
        }
    }
    let value = ellipsize(&value, 32);
    let label = match (info, &c.comment) {
        (Some(i), _) => i.label.to_string(),
        (None, Some(comment)) => comment.clone(),
        (None, None) => String::new(),
    };
    let lock = if info.is_some_and(|i| i.structural) {
        " 🔒"
    } else {
        ""
    };
    format!("{:<8}  {value:<32}  {label}{lock}", c.keyword)
        .trim_end()
        .to_string()
}

fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

fn hdu_summary(hdu: &Hdu) -> String {
    let kind = match &hdu.kind {
        HduKind::Primary => "primary".to_string(),
        HduKind::Image => "image".into(),
        HduKind::Table => "ascii table".into(),
        HduKind::BinTable => "binary table".into(),
        HduKind::CompressedImage => "compressed image".into(),
        HduKind::Other(s) => format!("extension {s:?}"),
    };
    let mut parts = vec![format!("HDU {} · {kind}", hdu.index)];
    if let Some(name) = hdu.cards.string("EXTNAME") {
        parts.push(name.to_string());
    }
    if !hdu.shape.is_empty() {
        if let Some(b) = hdu.bitpix {
            parts.push(bitpix_label(b, hdu.cards.float("BZERO")));
        }
        parts.push(
            hdu.shape
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(" × "),
        );
    }
    let blocks = hdu.header_blocks;
    parts.push(format!(
        "header {blocks} block{}",
        if blocks == 1 { "" } else { "s" }
    ));
    if hdu.data_bytes > 0 {
        parts.push(format!("data {}", bytes(hdu.data_bytes)));
    }
    parts.join(" · ")
}

fn bitpix_label(bitpix: i64, bzero: Option<f64>) -> String {
    match (bitpix, bzero) {
        (8, _) => "8-bit uint".into(),
        (16, Some(32768.0)) => "16-bit uint".into(),
        (32, Some(2147483648.0)) => "32-bit uint".into(),
        (b, _) if b > 0 => format!("{b}-bit int"),
        (b, _) => format!("{}-bit float", -b),
    }
}

fn bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}
