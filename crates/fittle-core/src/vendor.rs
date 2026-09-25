//! Who made this file: smart-scope profile (hardware) and software
//! fingerprints, driven entirely by data files in `data/` (no vendor names in
//! code). Match rules follow AstroSideKick's scope-profiles convention.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::header::Header;

// ---- data model ---------------------------------------------------------

type MatchRules = BTreeMap<String, Vec<String>>;

/// One scope in `scope-profiles.json` (only the fields Fittle uses).
#[derive(Debug, Clone, Deserialize)]
pub struct ScopeProfile {
    pub id: String,
    pub display: String,
    pub vendor: String,
    #[serde(rename = "match")]
    pub rules: MatchRules,
    pub sensor: Option<String>,
    pub pixel_size_um: Option<f64>,
    pub focal_length_mm: Option<f64>,
    pub aperture_mm: Option<f64>,
    pub native_frame: Option<[u64; 2]>,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub stack_files: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct Registry {
    profiles: Vec<ScopeProfile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Capture,
    Processing,
    Observatory,
}

#[derive(Debug, Clone, Deserialize)]
pub struct App {
    pub id: String,
    pub name: String,
    pub role: Role,
    #[serde(default, rename = "match")]
    pub rules: MatchRules,
    /// HISTORY substrings (case-insensitive) that indicate this software.
    #[serde(default)]
    pub history: Vec<String>,
    /// Keywords whose mere presence indicates this software.
    #[serde(default)]
    pub present: Vec<String>,
    /// COMMENT substrings (case-insensitive, `*` ignored) that indicate it.
    #[serde(default)]
    pub comment: Vec<String>,
    /// Where the fingerprint comes from (documentation only).
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Reason {
    pub key: String,
    pub reason: String,
}

/// Multiply a keyword's value to reach canonical units.
#[derive(Debug, Clone, Deserialize)]
pub struct Scale {
    pub key: String,
    pub factor: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MountIn {
    pub key: String,
    pub patterns: Vec<String>,
    pub reason: String,
}

/// How to read a vendor's header. `applies_to` holds scope-profile id
/// patterns (`zwo-seestar-*`) or `app:<id>`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Quirk {
    pub applies_to: Vec<String>,
    /// Extra keywords (tried first) for canonical fields.
    #[serde(default)]
    pub keys: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub ignore: Vec<Reason>,
    #[serde(default)]
    pub serial: Vec<Reason>,
    #[serde(default)]
    pub mount_in: Vec<MountIn>,
    pub row_order: Option<String>,
    #[serde(default)]
    pub scale: Vec<Scale>,
    /// Per-keyword value translations, e.g. IMAGETYP `EXT` → `Light`.
    #[serde(default)]
    pub aliases: BTreeMap<String, BTreeMap<String, String>>,
    /// `always`, or `stacked_file` (only when the file name marks a stack):
    /// EXPTIME holds the total integration, not the sub length.
    pub exptime_is_total: Option<String>,
    /// HISTORY prefixes followed by the number of combined frames.
    #[serde(default)]
    pub history_count: Vec<String>,
    /// Plain-language facts shown with the file.
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Deserialize)]
struct AppsFile {
    apps: Vec<App>,
    quirks: Vec<Quirk>,
}

struct Data {
    profiles: Vec<ScopeProfile>,
    apps: Vec<App>,
    quirks: Vec<Quirk>,
}

fn data() -> &'static Data {
    static DATA: OnceLock<Data> = OnceLock::new();
    DATA.get_or_init(|| {
        let reg: Registry = serde_json::from_str(include_str!("../data/scope-profiles.json"))
            .expect("valid scope-profiles.json");
        let apps: AppsFile =
            serde_json::from_str(include_str!("../data/apps.json")).expect("valid apps.json");
        Data {
            profiles: reg.profiles,
            apps: apps.apps,
            quirks: apps.quirks,
        }
    })
}

pub fn profiles() -> &'static [ScopeProfile] {
    &data().profiles
}

pub fn apps() -> &'static [App] {
    &data().apps
}

// ---- matching -----------------------------------------------------------

/// `*` is the only wildcard; comparison is trimmed and case-insensitive.
pub fn pattern_matches(pattern: &str, value: &str) -> bool {
    let p = pattern.trim().to_lowercase();
    let v = value.trim().to_lowercase();
    if !p.contains('*') {
        return p == v;
    }
    let parts: Vec<&str> = p.split('*').collect();
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(idx) = v[pos..].find(part) else {
            return false;
        };
        if i == 0 && idx != 0 {
            return false;
        }
        pos += idx + part.len();
    }
    parts.last().is_some_and(|l| l.is_empty() || v.ends_with(l))
}

/// Text of a keyword for matching: strings as-is, numbers as written.
fn text_of(h: &Header, key: &str) -> Option<String> {
    let card = h.get(key)?;
    Some(match &card.value {
        crate::Value::String(s) => s.clone(),
        _ => card.value_text.clone(),
    })
}

/// Evidence that a rule matched: `KEY='value'`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Matched {
    pub id: String,
    pub name: String,
    /// Keywords (or HISTORY lines) that matched, as `KEY=value` strings.
    pub evidence: Vec<String>,
    pub verified: bool,
}

fn matched_rules(rules: &MatchRules, h: &Header) -> Vec<String> {
    rules
        .iter()
        .filter_map(|(key, patterns)| {
            let v = text_of(h, key)?;
            patterns
                .iter()
                .any(|p| pattern_matches(p, &v))
                .then(|| format!("{key}={}", v.trim()))
        })
        .collect()
}

/// Best-matching smart-scope profile; more matching keys wins.
pub fn match_scope(h: &Header) -> Option<(&'static ScopeProfile, Matched)> {
    match_scopes(h).into_iter().next()
}

/// Every scope profile the header matches, best first (most matching keys).
/// More than one means the header disagrees with itself (e.g. INSTRUME says
/// one model, CREATOR another).
pub fn match_scopes(h: &Header) -> Vec<(&'static ScopeProfile, Matched)> {
    let mut all: Vec<(&ScopeProfile, Vec<String>)> = profiles()
        .iter()
        .map(|p| (p, matched_rules(&p.rules, h)))
        .filter(|(_, hits)| !hits.is_empty())
        .collect();
    // Weigh keys by how much they say about the hardware: INSTRUME is the
    // FITS instrument keyword, CREATOR names the capture device's software,
    // TELESCOP is often a user-renamable device name (Seestar). The stable
    // sort keeps registry order only for true ties.
    let weight = |hits: &[String]| -> u32 {
        hits.iter()
            .map(|e| match e.split('=').next().unwrap_or("") {
                "INSTRUME" => 4,
                "CREATOR" => 3,
                "TELESCOP" => 1,
                _ => 2,
            })
            .sum()
    };
    all.sort_by_key(|(_, hits)| std::cmp::Reverse(weight(hits)));
    all.into_iter()
        .map(|(p, evidence)| {
            let m = Matched {
                id: p.id.clone(),
                name: p.display.clone(),
                evidence,
                verified: p.verified,
            };
            (p, m)
        })
        .collect()
}

/// Software that wrote or processed the file, in data-file order.
pub fn match_apps(h: &Header) -> Vec<(&'static App, Matched)> {
    let history: Vec<String> = h.history().map(str::to_lowercase).collect();
    apps()
        .iter()
        .filter_map(|app| {
            let mut evidence = matched_rules(&app.rules, h);
            evidence.extend(
                app.present
                    .iter()
                    .filter(|k| h.get(k).is_some())
                    .map(|k| format!("{k} present")),
            );
            let comments: Vec<String> = h.commentary("COMMENT").map(str::to_lowercase).collect();
            if let Some(c) = app.comment.iter().find_map(|needle| {
                let n = needle.replace('*', "").to_lowercase();
                comments.iter().find(|l| l.contains(n.trim()))
            }) {
                let short: String = c.chars().take(48).collect();
                evidence.push(format!("COMMENT '{short}'"));
            }
            if let Some(line) = app
                .history
                .iter()
                .find_map(|needle| history.iter().find(|l| l.contains(&needle.to_lowercase())))
            {
                let short: String = line.chars().take(48).collect();
                evidence.push(format!("HISTORY '{short}…'"));
            }
            (!evidence.is_empty()).then(|| {
                (
                    app,
                    Matched {
                        id: app.id.clone(),
                        name: app.name.clone(),
                        evidence,
                        verified: app.verified,
                    },
                )
            })
        })
        .collect()
}

/// Quirks that apply given the matched scope profile and apps.
pub fn quirks_for(scope: Option<&str>, apps: &[&str]) -> Vec<&'static Quirk> {
    data()
        .quirks
        .iter()
        .filter(|q| {
            q.applies_to.iter().any(|t| match t.strip_prefix("app:") {
                Some(app) => apps.contains(&app),
                None => scope.is_some_and(|s| pattern_matches(t, s)),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::HeaderBuilder;

    fn header(lines: &[&str]) -> Header {
        let mut b = HeaderBuilder::default();
        for l in lines {
            b.push(format!("{l:<80}").as_bytes());
        }
        b.finish().0
    }

    #[test]
    fn patterns() {
        assert!(pattern_matches("S50_*", "S50_d2421385"));
        assert!(pattern_matches("seestar s50", " Seestar S50 "));
        assert!(!pattern_matches("S50", "S50 Pro"));
        assert!(pattern_matches("ZWO *AIR", "ZWO 2600AIR"));
        assert!(pattern_matches("*Mount*", "EQMod Mount"));
        assert!(!pattern_matches("S30*", "xS30"));
    }

    #[test]
    fn data_files_load() {
        assert!(profiles().iter().any(|p| p.id == "zwo-seestar-s50"));
        assert!(apps().iter().any(|a| a.id == "siril"));
    }

    #[test]
    fn seestar_sub() {
        let h = header(&[
            "CREATOR = 'ZWO Seestar S50'",
            "PRODUCER= 'ZWO     '",
            "INSTRUME= 'Seestar S50'",
            "TELESCOP= 'S50_d2421385'",
        ]);
        let (p, m) = match_scope(&h).unwrap();
        assert_eq!(p.id, "zwo-seestar-s50");
        assert_eq!(m.evidence.len(), 3);
        let apps: Vec<_> = match_apps(&h)
            .into_iter()
            .map(|(a, _)| a.id.as_str())
            .collect();
        assert_eq!(apps, ["seestar-app"]);
        let q = quirks_for(Some(&p.id), &apps);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].ignore[0].key, "APERTURE");
    }

    #[test]
    fn siril_history() {
        let h = header(&[
            "PROGRAM = 'Siril 1.4.4'",
            "HISTORY mean stacking with winsorized sigma clipping",
        ]);
        let m = match_apps(&h);
        assert_eq!(m[0].0.id, "siril");
        assert_eq!(m[0].1.evidence.len(), 2);
    }
}
