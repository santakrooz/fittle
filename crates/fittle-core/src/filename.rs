//! Facts encoded in capture-app file names, e.g.
//! `Light_NGC 6995_20.0s_LP_20260924-213412.fit` (Seestar) or
//! `Light_M 45_300.0s_Bin1_2600MC_gain100_20251024-212700_-14.9C_0001.fit` (ASIAIR).
//! Used only as secondary evidence; header keywords always win.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FileNameFacts {
    /// Leading frame-type or pipeline prefix, lowercased (`light`, `stacked`, `r_pp`…).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binning: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_c: Option<f64>,
    /// Frame count in stack names (`Stacked_771_…`, `…_460x10s_…`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<i64>,
    /// Filter token following the exposure (`LP`, `IRCUT`, `Ha`…).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

/// Prefixes recognised at the start of a name, longest first.
const PREFIXES: [&str; 18] = [
    "dso_stacked",
    "stacked",
    "masterlight",
    "masterdark",
    "masterflat",
    "masterbias",
    "master",
    "r_pp",
    "pp",
    "bkg",
    "result",
    "light",
    "dark",
    "flat",
    "bias",
    "darkflat",
    "offset",
    "integration",
];

const FILTERS: [&str; 14] = [
    "LP", "IRCUT", "UVIR", "L", "R", "G", "B", "Ha", "OIII", "SII", "HO", "SHO", "Duo", "Dual",
];

pub fn parse(file_name: &str) -> FileNameFacts {
    let stem = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let mut stem = stem;
    // Strip extensions (`.fit`, `.fits.fz`), not the dots inside `20.0s`.
    while let Some((head, ext)) = stem.rsplit_once('.') {
        if ext.len() <= 4 && ext.chars().all(|c| c.is_ascii_alphabetic()) {
            stem = head;
        } else {
            break;
        }
    }
    let lower = stem.to_lowercase();
    let mut f = FileNameFacts {
        prefix: PREFIXES
            .iter()
            .find(|p| lower.starts_with(*p) && lower[p.len()..].starts_with(['_', '-', ' ']))
            .map(|p| p.to_string()),
        ..Default::default()
    };

    let tokens: Vec<&str> = stem.split(['_', ' ']).filter(|t| !t.is_empty()).collect();
    for (i, t) in tokens.iter().enumerate() {
        let tl = t.to_lowercase();
        // Dwarf: `60s60` = 60 s exposure at gain 60.
        if let Some((e, g)) = tl.split_once('s').filter(|(e, g)| {
            num(e).is_some() && !g.is_empty() && g.chars().all(|c| c.is_ascii_digit())
        }) {
            f.exposure_s.get_or_insert(num(e).unwrap());
            f.gain.get_or_insert(g.parse().unwrap());
            if let Some(next) = tokens.get(i + 1) {
                f.filter.get_or_insert(next.to_string());
            }
        } else if let Some(n) = tl.strip_suffix('s').and_then(num) {
            f.exposure_s.get_or_insert(n);
            // A filter token often follows the exposure.
            if let Some(next) = tokens.get(i + 1) {
                if FILTERS.iter().any(|x| x.eq_ignore_ascii_case(next)) {
                    f.filter.get_or_insert(next.to_string());
                }
            }
        } else if let Some(ms) = tl.strip_suffix("ms").and_then(num) {
            f.exposure_s.get_or_insert(ms / 1000.0);
        } else if let Some((count, exp)) = tl.split_once('x') {
            // `460x10s`
            if let (Ok(c), Some(e)) = (count.parse::<i64>(), exp.strip_suffix('s').and_then(num)) {
                f.frames.get_or_insert(c);
                f.exposure_s.get_or_insert(e);
            }
        } else if let Some(g) = tl.strip_prefix("gain").and_then(|g| g.parse().ok()) {
            f.gain = Some(g);
        } else if let Some(b) = tl.strip_prefix("bin").and_then(|b| b.parse().ok()) {
            f.binning = Some(b);
        } else if let Some(c) = tl.strip_suffix('c').and_then(num) {
            if t.contains('.') || t.starts_with('-') {
                f.temp_c = Some(c);
            }
        }
    }
    // Seestar: `Stacked_771_…` / `DSO_Stacked_795_…` — the count follows the prefix.
    if let Some(p @ ("stacked" | "dso_stacked")) = f.prefix.as_deref() {
        let at = p.split('_').count();
        f.frames = f
            .frames
            .or_else(|| tokens.get(at).and_then(|t| t.parse::<i64>().ok()));
    }
    f
}

fn num(s: &str) -> Option<f64> {
    let ok = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-');
    ok.then(|| s.parse().ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seestar_sub() {
        let f = parse("Light_NGC 6995_20.0s_LP_20260924-213412.fit");
        assert_eq!(f.prefix.as_deref(), Some("light"));
        assert_eq!(f.exposure_s, Some(20.0));
        assert_eq!(f.filter.as_deref(), Some("LP"));
    }

    #[test]
    fn seestar_stacks() {
        let f = parse("Stacked_771_NGC 7380_20.0s_LP_20260919-050108.fit");
        assert_eq!(
            (f.prefix.as_deref(), f.frames),
            (Some("stacked"), Some(771))
        );
        let f = parse("DSO_Stacked_795_NGC 7380_20.0s_20260920_112354.fit");
        assert_eq!(
            (f.prefix.as_deref(), f.frames),
            (Some("dso_stacked"), Some(795))
        );
    }

    #[test]
    fn asiair() {
        let f = parse("Light_M 45_300.0s_Bin1_2600MC_gain100_20251024-212700_-14.9C_0001.fit");
        assert_eq!(f.exposure_s, Some(300.0));
        assert_eq!(
            (f.gain, f.binning, f.temp_c),
            (Some(100), Some(1), Some(-14.9))
        );
        let f = parse("Flat_608.3ms_Bin1_2600MC_gain100_20251025-055643_-16.0C_0001.fit");
        assert_eq!(f.prefix.as_deref(), Some("flat"));
        assert!((f.exposure_s.unwrap() - 0.6083).abs() < 1e-9);
    }

    #[test]
    fn dwarf() {
        let f = parse("stacked-16_M 31_60s60_Astro_20250101-203012345_12C.fits");
        assert_eq!(f.prefix.as_deref(), Some("stacked"));
        assert_eq!(
            (f.exposure_s, f.gain, f.filter.as_deref()),
            (Some(60.0), Some(60), Some("Astro"))
        );
        assert_eq!(f.frames, None, "must not take 31 from 'M 31'");
    }

    #[test]
    fn stack_summary_names() {
        let f = parse("M8_460x10s_77min.fits");
        assert_eq!((f.frames, f.exposure_s), (Some(460), Some(10.0)));
        assert_eq!(
            parse("r_pp_NGC6995_stacked.fit").prefix.as_deref(),
            Some("r_pp")
        );
    }
}
