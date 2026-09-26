//! Session report: the folder summary, per-night rows, consistency checks
//! and (optionally) grading, with Markdown, HTML and AstroBin CSV output.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::Serialize;

use crate::grade::{Grading, Reason, Rules, grade};
use crate::{Entry, Summary, summarize};

/// Schema id for `fittle scan --report json` and MCP reports.
pub const REPORT_SCHEMA: &str = "fittle.report/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Warn,
    Bad,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    pub status: Status,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Night {
    pub night: String,
    pub subs: usize,
    pub captured_s: f64,
    /// Set when graded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usable_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejects: Option<usize>,
    /// ok (< 5 % rejected), warn (< 20 %), bad; `ok` when not graded.
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub summary: Summary,
    pub nights: Vec<Night>,
    pub checks: Vec<Check>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grading: Option<Grading>,
}

fn lights(entries: &[Entry]) -> Vec<&Entry> {
    entries
        .iter()
        .filter(|e| e.frame == Some(fittle_core::classify::FrameKind::Light) && !e.integrated)
        .collect()
}

fn fmt_num(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Plain-language checks over the light subs.
pub fn checks(entries: &[Entry]) -> Vec<Check> {
    let subs = lights(entries);
    let n = subs.len();
    let mut out = Vec::new();
    let mut push = |status, text: String| out.push(Check { status, text });
    if n == 0 {
        push(Status::Warn, "No light subs in this folder".into());
        return out;
    }
    // A value shared by all subs, or the split.
    fn tally<T: Ord + Clone>(vals: impl Iterator<Item = Option<T>>) -> (BTreeMap<T, usize>, usize) {
        let mut m = BTreeMap::new();
        let mut missing = 0;
        for v in vals {
            match v {
                Some(v) => *m.entry(v).or_insert(0) += 1,
                None => missing += 1,
            }
        }
        (m, missing)
    }
    let night_of = |pred: &dyn Fn(&Entry) -> bool| -> String {
        let nights: BTreeSet<&str> = subs
            .iter()
            .filter(|e| pred(e))
            .filter_map(|e| e.night.as_deref())
            .collect();
        match nights.len() {
            0 => String::new(),
            1 => format!(" ({})", nights.iter().next().unwrap()),
            k => format!(" ({k} nights)"),
        }
    };

    let (exp, _) = tally(
        subs.iter()
            .map(|e| e.exposure_s.map(|x| (x * 1000.0).round() as i64)),
    );
    match exp.len() {
        0 => push(Status::Warn, "No exposure time on any sub".into()),
        1 => push(
            Status::Ok,
            format!(
                "Exposure {} s on all subs",
                fmt_num(*exp.keys().next().unwrap() as f64 / 1000.0)
            ),
        ),
        _ => push(
            Status::Warn,
            format!(
                "Mixed sub lengths: {}",
                exp.iter()
                    .map(|(e, c)| format!("{} s × {c}", fmt_num(*e as f64 / 1000.0)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
    }
    let (filters, nofilter) = tally(subs.iter().map(|e| e.filter.clone()));
    match (filters.len(), nofilter) {
        (0, _) => push(Status::Ok, "No filter recorded (OSC or unfiltered)".into()),
        (1, 0) => push(
            Status::Ok,
            format!("Filter {} on all subs", filters.keys().next().unwrap()),
        ),
        _ => push(
            Status::Ok,
            format!(
                "Filters: {}{}",
                filters
                    .iter()
                    .map(|(f, c)| format!("{f} × {c}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                if nofilter > 0 {
                    format!(", none × {nofilter}")
                } else {
                    String::new()
                }
            ),
        ),
    }
    let (gains, _) = tally(
        subs.iter()
            .map(|e| e.gain.map(|g| (g * 100.0).round() as i64)),
    );
    if gains.len() > 1 {
        let common = gains
            .iter()
            .max_by_key(|(_, c)| **c)
            .map(|(g, _)| *g)
            .unwrap();
        for (g, c) in gains.iter().filter(|(g, _)| **g != common) {
            let at = night_of(&|e: &Entry| e.gain.map(|x| (x * 100.0).round() as i64) == Some(*g));
            push(
                Status::Warn,
                format!("Gain {} on {c} subs{at}", fmt_num(*g as f64 / 100.0)),
            );
        }
    } else if let Some(g) = gains.keys().next() {
        push(
            Status::Ok,
            format!("Gain {} on all subs", fmt_num(*g as f64 / 100.0)),
        );
    }
    let temps: Vec<f64> = subs.iter().filter_map(|e| e.sensor_temp_c).collect();
    if !temps.is_empty() {
        let lo = temps.iter().copied().fold(f64::MAX, f64::min);
        let hi = temps.iter().copied().fold(f64::MIN, f64::max);
        let cooled = subs.iter().any(|e| e.set_temp_c.is_some());
        let status = if hi - lo > 4.0 {
            Status::Warn
        } else {
            Status::Ok
        };
        push(
            status,
            format!(
                "Sensor temp {}{} °C{}",
                lo.round(),
                if hi.round() > lo.round() {
                    format!(" to {}", hi.round())
                } else {
                    String::new()
                },
                if cooled { "" } else { ", uncooled" }
            ),
        );
    }
    let damaged = entries
        .iter()
        .filter(|e| e.error.is_some() || e.frame.is_none())
        .count();
    let mut seen = BTreeMap::new();
    let dupes = subs
        .iter()
        .filter_map(|e| Some((e.date_obs.clone()?, e.bytes)))
        .filter(|k| {
            let c = seen.entry(k.clone()).or_insert(0);
            *c += 1;
            *c == 2
        })
        .count();
    match (damaged, dupes) {
        (0, 0) => push(Status::Ok, "No truncated or duplicate files".into()),
        _ => {
            if damaged > 0 {
                push(
                    Status::Bad,
                    format!("{damaged} damaged or unreadable file(s)"),
                );
            }
            if dupes > 0 {
                push(
                    Status::Warn,
                    format!("{dupes} likely duplicate sub(s) (same time and size)"),
                );
            }
        }
    }
    let no_focal = subs
        .iter()
        .filter(|e| e.focal_mm.is_none_or(|f| f <= 0.0))
        .count();
    if no_focal > 0 {
        push(
            Status::Bad,
            format!(
                "FOCALLEN missing or zero on {no_focal} sub(s); stackers can't compute pixel scale"
            ),
        );
    }
    out
}

fn nights(entries: &[Entry], grading: Option<&Grading>) -> Vec<Night> {
    let mut by: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    for e in lights(entries) {
        let n = by
            .entry(e.night.clone().unwrap_or_else(|| "(unknown)".into()))
            .or_default();
        n.0 += 1;
        n.1 += e.exposure_s.unwrap_or(0.0);
    }
    by.into_iter()
        .map(|(night, (subs, captured_s))| {
            let (usable_s, rejects) = match grading {
                Some(g) => {
                    let mine: Vec<_> = g
                        .subs
                        .iter()
                        .filter(|s| s.night.as_deref().unwrap_or("(unknown)") == night)
                        .collect();
                    let rej = mine.iter().filter(|s| s.reject).count();
                    let usable = mine
                        .iter()
                        .filter(|s| !s.reject)
                        .filter_map(|s| s.exposure_s)
                        .sum();
                    (Some(usable), Some(rej))
                }
                None => (None, None),
            };
            let frac = rejects.map_or(0.0, |r| r as f64 / subs.max(1) as f64);
            Night {
                night,
                subs,
                captured_s,
                usable_s,
                rejects,
                status: if frac < 0.05 {
                    Status::Ok
                } else if frac < 0.2 {
                    Status::Warn
                } else {
                    Status::Bad
                },
            }
        })
        .collect()
}

/// Build the report; `rules` = Some(..) also grades every light sub.
pub fn report(folder: &str, entries: &[Entry], rules: Option<&Rules>) -> Report {
    let grading = rules.map(|r| grade(folder, entries, r));
    Report {
        schema: REPORT_SCHEMA,
        summary: summarize(folder, entries),
        nights: nights(entries, grading.as_ref()),
        checks: checks(entries),
        grading,
    }
}

/// `4h 12m`, `35 min`, `20 s`.
pub fn duration(s: f64) -> String {
    let m = (s / 60.0).round() as i64;
    match m {
        0 => format!("{} s", s.round()),
        1..60 => format!("{m} min"),
        _ => format!("{}h {:02}m", m / 60, m % 60),
    }
}

fn reason_text(r: Reason) -> String {
    serde_json::to_value(r)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.replace('_', " ")))
        .unwrap_or_default()
}

fn mark(s: Status) -> &'static str {
    match s {
        Status::Ok => "✓",
        Status::Warn => "!",
        Status::Bad => "✗",
    }
}

fn file_name(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

pub fn markdown(r: &Report) -> String {
    let s = &r.summary;
    let mut o = String::new();
    let _ = writeln!(o, "# Session report: {}\n", file_name(&s.folder));
    let _ = writeln!(
        o,
        "| Subs | Captured | Usable | Suggested rejects | Median HFR |"
    );
    let _ = writeln!(o, "| ---: | ---: | ---: | ---: | ---: |");
    let g = r.grading.as_ref();
    let _ = writeln!(
        o,
        "| {} | {} | {} | {} | {} |\n",
        g.map_or(s.frames.get("light").copied().unwrap_or(0), |g| g
            .subs
            .len()),
        duration(s.total_light_s),
        g.map_or("—".into(), |g| duration(g.usable_s)),
        g.map_or("—".into(), |g| g.rejected.to_string()),
        g.and_then(|g| g.median_hfr)
            .map_or("—".into(), |h| format!("{h:.2} px")),
    );
    let _ = writeln!(
        o,
        "## Targets\n\n| Target | Filter | Subs | Integration | Nights |\n| --- | --- | ---: | ---: | ---: |"
    );
    for t in &s.targets {
        let _ = writeln!(
            o,
            "| {} | {} | {} | {} | {} |",
            t.object,
            t.filter,
            t.subs,
            duration(t.integration_s),
            t.nights.len()
        );
    }
    let _ = writeln!(
        o,
        "\n## Nights\n\n| Night | Subs | Captured | Usable | Rejects |\n| --- | ---: | ---: | ---: | ---: |"
    );
    for n in &r.nights {
        let _ = writeln!(
            o,
            "| {} | {} | {} | {} | {} |",
            n.night,
            n.subs,
            duration(n.captured_s),
            n.usable_s.map_or("—".into(), duration),
            n.rejects.map_or("—".into(), |x| x.to_string())
        );
    }
    let _ = writeln!(o, "\n## Consistency\n");
    for c in &r.checks {
        let _ = writeln!(o, "- {} {}", mark(c.status), c.text);
    }
    if let Some(g) = g {
        let mut worst: Vec<_> = g.subs.iter().filter(|s| !s.reasons.is_empty()).collect();
        worst.sort_by(|a, b| b.badness.total_cmp(&a.badness));
        if !worst.is_empty() {
            let _ = writeln!(
                o,
                "\n## Worst subs\n\n| File | Stars | HFR | Background | Reason |\n| --- | ---: | ---: | ---: | --- |"
            );
            for w in worst.iter().take(20) {
                let st = w.stats.as_ref();
                let _ = writeln!(
                    o,
                    "| {} | {} | {} | {:.4} | {} |",
                    w.name,
                    st.map_or(0, |x| x.stars),
                    st.and_then(|x| x.hfr)
                        .map_or("—".into(), |h| format!("{h:.2}")),
                    st.map_or(0.0, |x| x.background),
                    w.reasons
                        .iter()
                        .map(|r| reason_text(*r))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
    }
    let _ = writeln!(
        o,
        "\n_Made with Fittle. Metrics are measurements; nothing was changed._"
    );
    o
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// HFR per sub in capture order, as an inline SVG (kept teal, rejects rose,
/// dashed limit of the largest group).
fn hfr_svg(g: &Grading) -> String {
    let pts: Vec<(usize, f32, bool)> = g
        .subs
        .iter()
        .enumerate()
        .filter_map(|(i, s)| Some((i, s.stats.as_ref()?.hfr?, s.reject)))
        .collect();
    if pts.is_empty() {
        return String::new();
    }
    let (w, h, pad) = (900.0, 260.0, 30.0);
    let top = pts
        .iter()
        .map(|p| p.1)
        .fold(1.0f32, f32::max)
        .max(g.median_hfr.unwrap_or(0.0) * 2.0)
        .ceil();
    let x = |i: usize| pad + (w - 2.0 * pad) * i as f32 / (g.subs.len().max(2) - 1) as f32;
    let y = |v: f32| h - pad - (h - 2.0 * pad) * (v / top).min(1.0);
    let mut o = format!("<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"HFR per sub\">");
    for k in 1..=(top as i32) {
        let (x2, yk) = (w - pad, y(k as f32));
        let ty = yk + 4.0;
        let _ = write!(
            o,
            "<line x1=\"{pad}\" x2=\"{x2}\" y1=\"{yk}\" y2=\"{yk}\" class=\"grid\"/><text x=\"4\" y=\"{ty}\">{k}</text>"
        );
    }
    if let Some(limit) = g
        .groups
        .iter()
        .max_by_key(|t| t.subs)
        .and_then(|t| t.hfr_max)
    {
        let (x2, yl) = (w - pad, y(limit));
        let _ = write!(
            o,
            "<line x1=\"{pad}\" x2=\"{x2}\" y1=\"{yl}\" y2=\"{yl}\" class=\"limit\"/>"
        );
    }
    for (i, v, rej) in pts {
        let _ = write!(
            o,
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2\" class=\"{}\"/>",
            x(i),
            y(v),
            if rej { "rej" } else { "kept" }
        );
    }
    o.push_str("</svg>");
    o
}

/// A self-contained HTML page (dark design tokens).
pub fn html(r: &Report) -> String {
    let md_tables = markdown(r);
    let s = &r.summary;
    let g = r.grading.as_ref();
    let tile = |v: String, l: &str, cls: &str| {
        format!(
            "<div class=\"tile {cls}\"><b>{}</b><span>{l}</span></div>",
            esc(&v)
        )
    };
    let mut o = String::new();
    let _ = write!(
        o,
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Session report</title><style>\
:root{{--bg:#0A0E16;--surface:#111827;--line:#1F2A3D;--text:#E8ECF3;--muted:#8391A7;--accent:#F5B83D;--ok:#3DD68C;--rose:#E0567A;--teal:#3FC1C9}}\
body{{margin:0;background:var(--bg);color:var(--text);font:14px/1.5 Inter,system-ui,sans-serif;padding:32px 16px}}main{{max-width:980px;margin:auto}}\
h1{{font:600 26px system-ui}}h2{{font-size:13px;letter-spacing:.08em;text-transform:uppercase;color:var(--muted);margin-top:28px}}\
.tiles{{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:10px}}.tile{{background:var(--surface);border:1px solid var(--line);border-radius:14px;padding:14px}}\
.tile b{{display:block;font-size:26px}}.tile span{{font-size:11px;letter-spacing:.08em;text-transform:uppercase;color:var(--muted)}}.good b{{color:var(--ok)}}.bad b{{color:var(--rose)}}\
svg{{width:100%;background:var(--surface);border:1px solid var(--line);border-radius:14px}}svg text{{fill:var(--muted);font-size:10px}}.grid{{stroke:var(--line)}}.limit{{stroke:var(--rose);stroke-dasharray:6 4}}.kept{{fill:var(--teal)}}.rej{{fill:var(--rose)}}\
table{{width:100%;border-collapse:collapse;font-size:13px}}td,th{{padding:6px 8px;border-bottom:1px solid var(--line);text-align:left}}th{{color:var(--muted);font-weight:500}}\
ul{{list-style:none;padding:0}}li{{padding:3px 0;font-family:ui-monospace,monospace;font-size:12.5px}}.ok{{color:var(--ok)}}.warn{{color:var(--accent)}}.badc{{color:var(--rose)}}\
footer{{color:var(--muted);font-size:12px;margin-top:32px}}</style></head><body><main><h1>Session report · {}</h1><div class=\"tiles\">",
        esc(file_name(&s.folder))
    );
    o.push_str(&tile(
        g.map_or(s.frames.get("light").copied().unwrap_or(0), |g| {
            g.subs.len()
        })
        .to_string(),
        "Subs found",
        "",
    ));
    o.push_str(&tile(duration(s.total_light_s), "Captured", ""));
    if let Some(g) = g {
        o.push_str(&tile(duration(g.usable_s), "Usable", "good"));
        o.push_str(&tile(
            g.rejected.to_string(),
            "Suggested rejects",
            if g.rejected > 0 { "bad" } else { "" },
        ));
        o.push_str(&tile(
            g.median_hfr.map_or("—".into(), |h| format!("{h:.1} px")),
            "Median HFR",
            "",
        ));
    }
    o.push_str("</div>");
    if let Some(g) = g {
        let _ = write!(o, "<h2>HFR per sub, in capture order</h2>{}", hfr_svg(g));
    }
    // Tables come from the Markdown so both stay in step.
    for section in md_tables.split("\n## ").skip(1) {
        let (title, body) = section.split_once('\n').unwrap_or((section, ""));
        let _ = write!(o, "<h2>{}</h2>", esc(title));
        let rows: Vec<&str> = body.lines().filter(|l| l.starts_with('|')).collect();
        if rows.len() >= 2 {
            o.push_str("<table>");
            for (i, row) in rows.iter().enumerate() {
                if i == 1 {
                    continue;
                }
                let tag = if i == 0 { "th" } else { "td" };
                o.push_str("<tr>");
                for cell in row.trim_matches('|').split('|') {
                    let _ = write!(o, "<{tag}>{}</{tag}>", esc(cell.trim()));
                }
                o.push_str("</tr>");
            }
            o.push_str("</table>");
        } else {
            o.push_str("<ul>");
            for c in &r.checks {
                let cls = match c.status {
                    Status::Ok => "ok",
                    Status::Warn => "warn",
                    Status::Bad => "badc",
                };
                let _ = write!(
                    o,
                    "<li><span class=\"{cls}\">{}</span> {}</li>",
                    mark(c.status),
                    esc(&c.text)
                );
            }
            o.push_str("</ul>");
        }
    }
    o.push_str("<footer>Made with Fittle. Metrics are measurements; nothing was changed.</footer></main></body></html>");
    o
}

/// AstroBin acquisition CSV (one row per night × filter × sub length). The
/// `filter` column holds the filter name; AstroBin asks for its filter ID,
/// so replace it before importing.
pub fn astrobin_csv(r: &Report, entries: &[Entry]) -> String {
    let rejected: BTreeSet<&str> = r
        .grading
        .as_ref()
        .map(|g| {
            g.subs
                .iter()
                .filter(|s| s.reject)
                .map(|s| s.path.as_str())
                .collect()
        })
        .unwrap_or_default();
    type Key = (String, String, i64);
    let mut rows: BTreeMap<Key, Vec<&Entry>> = BTreeMap::new();
    for e in lights(entries)
        .into_iter()
        .filter(|e| !rejected.contains(e.path.as_str()))
    {
        let key = (
            e.night.clone().unwrap_or_default(),
            e.filter.clone().unwrap_or_default(),
            e.exposure_s.map_or(0, |x| (x * 1000.0).round() as i64),
        );
        rows.entry(key).or_default().push(e);
    }
    let mut o = String::from("date,filter,number,duration,binning,gain,sensorCooling,meanFwhm\n");
    let fwhm: BTreeMap<&str, f32> = r
        .grading
        .as_ref()
        .map(|g| {
            g.subs
                .iter()
                .filter_map(|s| Some((s.path.as_str(), s.stats.as_ref()?.fwhm?)))
                .collect()
        })
        .unwrap_or_default();
    for ((night, filter, ms), subs) in rows {
        let first = subs[0];
        // FWHM in arcsec when the pixel scale is known.
        let arcsec: Vec<f64> = subs
            .iter()
            .filter_map(|e| Some(*fwhm.get(e.path.as_str())? as f64 * e.pixel_scale?))
            .collect();
        let mean_fwhm = if arcsec.is_empty() {
            String::new()
        } else {
            format!("{:.2}", arcsec.iter().sum::<f64>() / arcsec.len() as f64)
        };
        let _ = writeln!(
            o,
            "{night},{},{},{},{},{},{},{mean_fwhm}",
            filter.replace(',', " "),
            subs.len(),
            fmt_num(ms as f64 / 1000.0),
            first.binning.unwrap_or(1),
            first.gain.map_or(String::new(), fmt_num),
            first
                .set_temp_c
                .map_or(String::new(), |t| fmt_num(t.round())),
        );
    }
    o
}

/// Write the report (`md`, `html`, `json` or `astrobin`) into the scanned
/// folder under a new name (`fittle-session-report.md`, `_2`, …). Never
/// replaces a file.
pub fn save(r: &Report, entries: &[Entry], format: &str) -> std::io::Result<std::path::PathBuf> {
    use std::io::Write;
    let (stem, ext, text) = match format {
        "md" | "markdown" => ("fittle-session-report", "md", markdown(r)),
        "html" => ("fittle-session-report", "html", html(r)),
        "json" => (
            "fittle-session-report",
            "json",
            serde_json::to_string_pretty(r).map_err(std::io::Error::other)?,
        ),
        "astrobin" | "csv" => ("astrobin-acquisition", "csv", astrobin_csv(r, entries)),
        other => {
            return Err(std::io::Error::other(format!(
                "unknown report format '{other}'"
            )));
        }
    };
    let dir = std::path::Path::new(&r.summary.folder);
    for n in 1.. {
        let name = if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}_{n}.{ext}")
        };
        let path = dir.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                f.write_all(text.as_bytes())?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn corpus_report_renders() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic");
        let entries = crate::list_recursive(&root).unwrap();
        let r = report(&root.to_string_lossy(), &entries, Some(&Rules::default()));
        assert_eq!(r.schema, REPORT_SCHEMA);
        assert!(!r.nights.is_empty());
        assert!(
            r.checks
                .iter()
                .any(|c| c.status == Status::Bad && c.text.contains("damaged"))
        );
        let md = markdown(&r);
        assert!(
            md.starts_with("# Session report")
                && md.contains("## Consistency")
                && md.contains("| NGC 6995 |")
        );
        let html = html(&r);
        assert!(html.contains("<svg") && html.contains("<table>") && html.ends_with("</html>"));
        let csv = astrobin_csv(&r, &entries);
        assert!(csv.starts_with("date,filter,number,duration"));
        assert!(csv.lines().count() > 1);
    }

    #[test]
    fn checks_flag_gain_and_focal() {
        let base = Entry {
            path: "a".into(),
            name: "a".into(),
            bytes: 10,
            label: Some("Light sub".into()),
            frame: Some(fittle_core::classify::FrameKind::Light),
            integrated: false,
            exposure_s: Some(20.0),
            stack_count: None,
            filter: Some("LP".into()),
            object: Some("NGC 6995".into()),
            date_obs: Some("2026-09-18T21:00:00".into()),
            night: Some("2026-09-18".into()),
            gain: Some(80.0),
            focal_mm: None,
            sensor_temp_c: Some(24.0),
            set_temp_c: None,
            binning: Some(1),
            pixel_scale: None,
            offset: None,
            camera: Some("ZWO ASI2600MC Pro".into()),
            size: Some([6248, 4176]),
            error: None,
        };
        let mut other = base.clone();
        other.path = "b".into();
        other.gain = Some(120.0);
        other.date_obs = Some("2026-09-18T21:01:00".into());
        other.sensor_temp_c = Some(32.0);
        let c = checks(&[base.clone(), base.clone(), other]);
        let text: Vec<&str> = c.iter().map(|c| c.text.as_str()).collect();
        assert!(text.contains(&"Exposure 20 s on all subs"), "{text:?}");
        assert!(text.contains(&"Filter LP on all subs"));
        assert!(text.contains(&"Gain 120 on 1 subs (2026-09-18)"));
        assert!(text.contains(&"Sensor temp 24 to 32 °C, uncooled"));
        assert!(text.iter().any(|t| t.contains("duplicate")));
        assert!(
            text.iter()
                .any(|t| t.starts_with("FOCALLEN missing or zero on 3"))
        );
    }
}
