//! Header diff with plain-language impact ("blocks calibration", "Δ 3.6 °C").
//! Impact is judged from what each file is (via the classifier), so a light
//! compared with a master dark reads differently from two lights.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::card::{Card, Value};
use crate::classify::FrameKind;
use crate::fits::{Error, Fits};
use crate::info::{Info, info_from};

pub const DIFF_SCHEMA: &str = "fittle.diff/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Same,
    Changed,
    OnlyA,
    OnlyB,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Ok,
    Info,
    Warning,
    Blocker,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Impact {
    pub level: Level,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub keyword: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<String>,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<Impact>,
    /// `wcs` for plate-solution keywords, which views may collapse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<&'static str>,
}

/// Plate-solution keywords (WCS and SIP distortion terms).
pub fn is_wcs(k: &str) -> bool {
    const EXACT: [&str; 12] = [
        "WCSAXES", "LONPOLE", "LATPOLE", "RADESYS", "MJDREF", "A_ORDER", "B_ORDER", "AP_ORDER",
        "BP_ORDER", "IMAGEW", "IMAGEH", "PLTSOLVD",
    ];
    const PREFIX: [&str; 12] = [
        "CTYPE", "CRVAL", "CRPIX", "CDELT", "CROTA", "CUNIT", "CD1_", "CD2_", "PC1_", "PC2_", "PV",
        "LTM",
    ];
    let sip = ["A_", "B_", "AP_", "BP_"].iter().any(|p| {
        k.strip_prefix(p)
            .is_some_and(|r| r.chars().next().is_some_and(|c| c.is_ascii_digit()))
    });
    EXACT.contains(&k) || PREFIX.iter().any(|p| k.starts_with(p)) || sip
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Side {
    pub path: String,
    pub hdu: usize,
    pub verdict: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Diff {
    pub schema: &'static str,
    pub a: Side,
    pub b: Side,
    /// Every value keyword in either header, in A's order then B's extras.
    pub rows: Vec<Row>,
    pub differences: usize,
    pub blockers: usize,
}

impl Row {
    /// Shown in the default view: differences, plus matches that matter.
    pub fn is_notable(&self) -> bool {
        self.status != Status::Same || self.impact.is_some()
    }
}

/// Keys whose mismatch prevents using one file to calibrate the other.
const CALIBRATION: &[(&str, &str)] = &[
    ("GAIN", "gain"),
    ("OFFSET", "offset"),
    ("XBINNING", "binning"),
    ("YBINNING", "binning"),
    ("NAXIS1", "image width"),
    ("NAXIS2", "image height"),
    ("INSTRUME", "camera"),
    ("READOUTM", "readout mode"),
    ("BAYERPAT", "Bayer pattern"),
    ("ROWORDER", "row order"),
];

pub fn diff_files(a: &str, b: &str, hdu: Option<usize>) -> Result<Diff, Error> {
    let fa = Fits::open(a)?;
    let fb = Fits::open(b)?;
    Ok(diff(&fa, a, &fb, b, hdu))
}

pub fn diff(fa: &Fits, a_path: &str, fb: &Fits, b_path: &str, hdu: Option<usize>) -> Diff {
    let ia = info_from(fa, a_path);
    let ib = info_from(fb, b_path);
    let pick = |f: &Fits, info: &Info| {
        hdu.unwrap_or_else(|| info.image.as_ref().map_or(0, |i| i.hdu))
            .min(f.hdus.len() - 1)
    };
    let (ha, hb) = (pick(fa, &ia), pick(fb, &ib));
    let ca = values(&fa.hdus[ha].header().cards);
    let cb = values(&fb.hdus[hb].header().cards);

    let mut order: Vec<&str> = ca.keys.clone();
    order.extend(cb.keys.iter().filter(|k| !ca.map.contains_key(*k)));

    let rows: Vec<Row> = order
        .into_iter()
        .map(|k| {
            let (va, vb) = (ca.map.get(k).copied(), cb.map.get(k).copied());
            let status = match (va, vb) {
                (Some(x), Some(y)) if same(x, y) => Status::Same,
                (Some(_), Some(_)) => Status::Changed,
                (Some(_), None) => Status::OnlyA,
                _ => Status::OnlyB,
            };
            Row {
                keyword: k.to_string(),
                a: va.map(show),
                b: vb.map(show),
                status,
                impact: impact(k, va, vb, status, &ia, &ib),
                group: is_wcs(k).then_some("wcs"),
            }
        })
        .collect();
    let differences = rows.iter().filter(|r| r.status != Status::Same).count();
    let blockers = rows
        .iter()
        .filter(|r| r.impact.as_ref().is_some_and(|i| i.level == Level::Blocker))
        .count();
    Diff {
        schema: DIFF_SCHEMA,
        a: Side {
            path: a_path.into(),
            hdu: ha,
            verdict: ia.verdict.label.clone(),
        },
        b: Side {
            path: b_path.into(),
            hdu: hb,
            verdict: ib.verdict.label.clone(),
        },
        rows,
        differences,
        blockers,
    }
}

struct Values<'a> {
    keys: Vec<&'a str>,
    map: BTreeMap<&'a str, &'a Card>,
}

fn values(cards: &[Card]) -> Values<'_> {
    let mut v = Values {
        keys: Vec::new(),
        map: BTreeMap::new(),
    };
    for c in cards.iter().filter(|c| !c.value.is_commentary()) {
        if v.map.insert(c.keyword.as_str(), c).is_none() {
            v.keys.push(c.keyword.as_str());
        }
    }
    v
}

fn same(a: &Card, b: &Card) -> bool {
    match (&a.value, &b.value) {
        (Value::String(x), Value::String(y)) => x.trim() == y.trim(),
        (x, y) => match (x.as_f64(), y.as_f64()) {
            (Some(p), Some(q)) => p == q,
            _ => x == y,
        },
    }
}

fn show(c: &Card) -> String {
    match &c.value {
        Value::String(s) => format!("'{}'", s.trim()),
        _ => c.value_text.clone(),
    }
}

fn is_cal(f: FrameKind) -> bool {
    matches!(
        f,
        FrameKind::Dark | FrameKind::Flat | FrameKind::Bias | FrameKind::DarkFlat
    )
}

fn impact(
    k: &str,
    a: Option<&Card>,
    b: Option<&Card>,
    status: Status,
    ia: &Info,
    ib: &Info,
) -> Option<Impact> {
    let imp = |level, text: String| Some(Impact { level, text });
    let (fa, fb) = (ia.verdict.frame, ib.verdict.frame);
    let calibrating = is_cal(fa) != is_cal(fb) || (is_cal(fa) && is_cal(fb));
    let num = |c: Option<&Card>| c.and_then(|c| c.value.as_f64());

    if let Some((_, what)) = CALIBRATION.iter().find(|(key, _)| *key == k) {
        return match status {
            Status::Same if calibrating => imp(Level::Ok, "match".into()),
            Status::Same => None,
            Status::Changed => imp(
                Level::Blocker,
                format!("blocks calibration ({what} differs)"),
            ),
            _ => imp(Level::Warning, format!("{what} missing on one side")),
        };
    }
    match k {
        "CCD-TEMP" | "SET-TEMP" => {
            let d = (num(a)? - num(b)?).abs();
            let level = if d > 2.0 { Level::Warning } else { Level::Ok };
            let text = if d < 0.05 {
                "match".to_string()
            } else {
                format!("Δ {d:.1} °C")
            };
            (d > 0.0 || calibrating).then_some(Impact { level, text })
        }
        "EXPTIME" | "EXPOSURE" => {
            let dark = matches!(fa, FrameKind::Dark) || matches!(fb, FrameKind::Dark);
            match status {
                Status::Same if dark => imp(Level::Ok, "match".into()),
                Status::Changed if dark && !(ia.verdict.integrated && ib.verdict.integrated) => {
                    imp(
                        Level::Warning,
                        "dark exposure differs (needs scaling)".into(),
                    )
                }
                _ => None,
            }
        }
        "FILTER" => {
            let flat = matches!(fa, FrameKind::Flat) || matches!(fb, FrameKind::Flat);
            match status {
                Status::Changed if flat => imp(
                    Level::Blocker,
                    "blocks flat calibration (filter differs)".into(),
                ),
                Status::Same if flat => imp(Level::Ok, "match".into()),
                _ => None,
            }
        }
        "IMAGETYP" | "FRAME" if status == Status::Changed && fa != fb => {
            imp(Level::Info, "expected".into())
        }
        "BITPIX"
            if status == Status::Changed && (ia.verdict.integrated || ib.verdict.integrated) =>
        {
            imp(Level::Info, "expected for a stack or master".into())
        }
        "STACKCNT" | "NCOMBINE" if status != Status::Same => {
            let n = num(a).or(num(b))?;
            imp(
                Level::Info,
                format!(
                    "{} of {n}",
                    if is_cal(fa) || is_cal(fb) {
                        "master"
                    } else {
                        "stack"
                    }
                ),
            )
        }
        "DATE-OBS" if status == Status::Changed => {
            let t = |c: Option<&Card>| {
                let s = c?.value.as_str()?;
                fittle_astro::parse_datetime(s).map(|t| fittle_astro::julian_day(&t))
            };
            let days = (t(a)? - t(b)?).abs();
            let text = if days >= 1.0 {
                format!("{} days apart", days.round())
            } else if days * 24.0 >= 1.0 {
                format!("{:.1} h apart", days * 24.0)
            } else {
                format!("{} min apart", (days * 1440.0).round())
            };
            imp(Level::Info, text)
        }
        _ => None,
    }
}
