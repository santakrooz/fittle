//! Calibration matcher: which darks, flats and bias (or dark-flats) in a
//! library fit each group of lights, and why the others don't.
//! Header-only; nothing is read beyond the header, nothing is written.

use std::collections::BTreeMap;

use fittle_core::classify::FrameKind;
use serde::Serialize;

use crate::Entry;
use crate::report::Status;

/// Schema id for `fittle match-cal --json` and `fits_match_calibration`.
pub const CALMATCH_SCHEMA: &str = "fittle.calmatch/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalKind {
    Dark,
    Flat,
    Bias,
    DarkFlat,
}

/// A master file, or individual frames with identical settings.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalSet {
    pub kind: CalKind,
    /// Master file name, or a description of the set.
    pub name: String,
    pub master: bool,
    pub frames: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_c: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binning: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<[u64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub night: Option<String>,
    pub paths: Vec<String>,
}

/// The best candidate of one kind for a light group.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindMatch {
    pub kind: CalKind,
    /// ok, warn (usable with caveats), bad (mismatch), or `none` found.
    pub status: MatchStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set: Option<CalSet>,
    /// Plain-language differences, e.g. `gain 80 (lights 120)`.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    Ok,
    Warn,
    Bad,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupStatus {
    Ready,
    Partial,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LightGroup {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binning: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<[u64; 2]>,
    /// Median sensor temperature of the subs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_c: Option<f64>,
    pub cooled: bool,
    pub subs: usize,
    pub integration_s: f64,
    pub nights: Vec<String>,
    pub darks: KindMatch,
    pub flats: KindMatch,
    /// Bias, or dark-flats for the matched flats.
    pub bias: KindMatch,
    pub status: GroupStatus,
    /// Why this group is not ready, and what to do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Matching {
    pub schema: &'static str,
    pub lights: String,
    pub library: String,
    pub library_frames: usize,
    pub sets: usize,
    pub groups: Vec<LightGroup>,
    pub ready: usize,
    pub partial: usize,
    pub missing: usize,
}

fn num(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn close(a: Option<f64>, b: Option<f64>, tol: f64) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() <= tol.max(1e-6 * a.abs()),
        _ => true, // unknown on one side: not a mismatch we can prove
    }
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[v.len() / 2])
}

fn cal_kind(e: &Entry) -> Option<CalKind> {
    match e.frame? {
        FrameKind::Dark => Some(CalKind::Dark),
        FrameKind::Flat => Some(CalKind::Flat),
        FrameKind::Bias => Some(CalKind::Bias),
        FrameKind::DarkFlat => Some(CalKind::DarkFlat),
        _ => None,
    }
}

/// Group library frames into masters and sets of like frames.
pub fn sets(library: &[Entry]) -> Vec<CalSet> {
    type Key = (
        CalKind,
        String,
        i64,
        i64,
        i64,
        i64,
        i64,
        String,
        String,
        String,
    );
    let mut groups: BTreeMap<Key, Vec<&Entry>> = BTreeMap::new();
    let mut out = Vec::new();
    let r = |v: Option<f64>, k: f64| v.map_or(i64::MIN, |x| (x * k).round() as i64);
    for e in library {
        let Some(kind) = cal_kind(e) else { continue };
        if e.integrated {
            out.push(set_of(kind, &[e], true));
            continue;
        }
        let key: Key = (
            kind,
            e.camera.clone().unwrap_or_default(),
            r(e.gain, 1.0),
            r(e.offset, 1.0),
            r(e.exposure_s, 1000.0),
            // 1 °C buckets; flats and bias don't care.
            if kind == CalKind::Dark {
                r(e.sensor_temp_c, 1.0)
            } else {
                0
            },
            e.binning.unwrap_or(1),
            e.filter.clone().unwrap_or_default(),
            e.size
                .map_or(String::new(), |s| format!("{}x{}", s[0], s[1])),
            if kind == CalKind::Flat {
                e.night.clone().unwrap_or_default()
            } else {
                String::new()
            },
        );
        groups.entry(key).or_default().push(e);
    }
    out.extend(groups.into_values().map(|es| {
        let kind = cal_kind(es[0]).expect("calibration frame");
        set_of(kind, &es, false)
    }));
    out
}

fn set_of(kind: CalKind, es: &[&Entry], master: bool) -> CalSet {
    let e = es[0];
    let temp = median(es.iter().filter_map(|e| e.sensor_temp_c).collect());
    let noun = match kind {
        CalKind::Dark => "darks",
        CalKind::Flat => "flats",
        CalKind::Bias => "bias",
        CalKind::DarkFlat => "dark-flats",
    };
    let name = if master {
        e.name.clone()
    } else {
        let mut parts = vec![format!("{} {noun}", es.len())];
        if matches!(kind, CalKind::Dark | CalKind::DarkFlat | CalKind::Flat) {
            if let Some(x) = e.exposure_s {
                parts.push(format!("{} s", num(x)));
            }
        }
        if let Some(f) = e.filter.as_ref().filter(|_| kind == CalKind::Flat) {
            parts.push(f.clone());
        }
        if let Some(g) = e.gain {
            parts.push(format!("g{}", num(g)));
        }
        if let Some(t) = temp.filter(|_| kind == CalKind::Dark) {
            parts.push(format!("{} °C", t.round()));
        }
        parts.join(" · ")
    };
    CalSet {
        kind,
        name,
        master,
        frames: if master {
            e.stack_count.unwrap_or(1).max(1) as usize
        } else {
            es.len()
        },
        camera: e.camera.clone(),
        gain: e.gain,
        offset: e.offset,
        exposure_s: e.exposure_s,
        temp_c: temp,
        binning: e.binning,
        filter: e.filter.clone(),
        size: e.size,
        night: e.night.clone(),
        paths: es.iter().map(|e| e.path.clone()).collect(),
    }
}

/// Differences between a set and what it must match, worst first.
struct Fit {
    status: MatchStatus,
    notes: Vec<String>,
    score: f64,
}

fn fit(
    set: &CalSet,
    g: &LightGroup,
    want_exposure: Option<f64>,
    want_gain: Option<f64>,
    want_offset: Option<f64>,
) -> Fit {
    let mut bad = Vec::new();
    let mut warn = Vec::new();
    let mut score = 0.0;
    let norm = |s: &Option<String>| s.as_deref().map(|x| x.trim().to_lowercase());
    if norm(&set.camera)
        .zip(norm(&g.camera))
        .is_some_and(|(a, b)| a != b)
    {
        bad.push(format!(
            "different camera ({})",
            set.camera.as_deref().unwrap_or("?")
        ));
    }
    if set.size.zip(g.size).is_some_and(|(a, b)| a != b) {
        let s = set.size.unwrap();
        bad.push(format!("size {}×{}", s[0], s[1]));
    }
    if set.binning.unwrap_or(1) != g.binning.unwrap_or(1) {
        bad.push(format!("bin {}", set.binning.unwrap_or(1)));
    }
    // Gain/offset: must match for darks and bias; flats only warn.
    let flat = set.kind == CalKind::Flat;
    let mut settings = Vec::new();
    if !close(set.gain, want_gain, 0.5) {
        let (a, b) = (set.gain.unwrap_or(0.0), want_gain.unwrap_or(0.0));
        settings.push(format!(
            "gain {} (needs {}, Δ {})",
            num(a),
            num(b),
            num((a - b).abs())
        ));
    }
    if !close(set.offset, want_offset, 0.5) {
        settings.push(format!(
            "offset {} (needs {})",
            num(set.offset.unwrap_or(0.0)),
            num(want_offset.unwrap_or(0.0))
        ));
    }
    if flat {
        warn.extend(settings)
    } else {
        bad.extend(settings)
    }
    if let Some(want) = want_exposure.filter(|_| !close(set.exposure_s, want_exposure, 0.01)) {
        bad.push(format!(
            "{} s (needs {} s)",
            num(set.exposure_s.unwrap_or(0.0)),
            num(want)
        ));
    }
    if set.kind == CalKind::Dark {
        if let (Some(a), Some(b)) = (set.temp_c, g.temp_c) {
            let d = (a - b).abs();
            let (ok, meh) = if g.cooled { (2.0, 5.0) } else { (3.0, 6.0) };
            score += d;
            if d > meh {
                bad.push(format!("{} °C (lights {} °C)", a.round(), b.round()));
            } else if d > ok {
                warn.push(format!("{} °C (lights {} °C)", a.round(), b.round()));
            }
        }
    }
    if flat {
        let (lf, sf) = (norm(&g.filter), norm(&set.filter));
        if lf != sf && !(lf.is_none() || sf.is_none()) {
            bad.push(format!(
                "filter {} (lights {})",
                set.filter.as_deref().unwrap_or("none"),
                g.filter.as_deref().unwrap_or("none")
            ));
        }
        if let (Some(fn_), false) = (&set.night, g.nights.is_empty()) {
            if !g.nights.contains(fn_) {
                warn.push(format!(
                    "taken {fn_}, not on a light night; dust or rotation may differ"
                ));
            }
        }
    }
    score +=
        bad.len() as f64 * 1000.0 + warn.len() as f64 * 10.0 + if set.master { 0.0 } else { 0.5 };
    let status = if !bad.is_empty() {
        MatchStatus::Bad
    } else if !warn.is_empty() {
        MatchStatus::Warn
    } else {
        MatchStatus::Ok
    };
    bad.extend(warn);
    Fit {
        status,
        notes: bad,
        score,
    }
}

fn best(
    sets: &[CalSet],
    kinds: &[CalKind],
    g: &LightGroup,
    want: impl Fn(&CalSet) -> (Option<f64>, Option<f64>, Option<f64>),
) -> KindMatch {
    let kind = kinds[0];
    let chosen = sets
        .iter()
        .filter(|s| kinds.contains(&s.kind))
        .map(|s| {
            let (e, ga, o) = want(s);
            (s, fit(s, g, e, ga, o))
        })
        .min_by(|a, b| a.1.score.total_cmp(&b.1.score));
    match chosen {
        Some((s, f)) => KindMatch {
            kind: s.kind,
            status: f.status,
            set: Some(s.clone()),
            notes: f.notes,
        },
        None => KindMatch {
            kind,
            status: MatchStatus::None,
            set: None,
            notes: vec![],
        },
    }
}

fn lights_groups(lights: &[Entry]) -> Vec<(LightGroup, Vec<&Entry>)> {
    type Key = (String, i64, i64, i64, i64, String, String);
    let mut by: BTreeMap<Key, Vec<&Entry>> = BTreeMap::new();
    let r = |v: Option<f64>, k: f64| v.map_or(i64::MIN, |x| (x * k).round() as i64);
    for e in lights
        .iter()
        .filter(|e| e.frame == Some(FrameKind::Light) && !e.integrated)
    {
        by.entry((
            e.camera.clone().unwrap_or_default(),
            r(e.gain, 1.0),
            r(e.offset, 1.0),
            r(e.exposure_s, 1000.0),
            e.binning.unwrap_or(1),
            e.filter.clone().unwrap_or_default(),
            e.size
                .map_or(String::new(), |s| format!("{}x{}", s[0], s[1])),
        ))
        .or_default()
        .push(e);
    }
    by.into_values()
        .map(|es| {
            let e = es[0];
            let mut nights: Vec<String> = es.iter().filter_map(|e| e.night.clone()).collect();
            nights.sort();
            nights.dedup();
            let none = |kind| KindMatch {
                kind,
                status: MatchStatus::None,
                set: None,
                notes: vec![],
            };
            (
                LightGroup {
                    camera: e.camera.clone(),
                    gain: e.gain,
                    offset: e.offset,
                    exposure_s: e.exposure_s,
                    binning: e.binning,
                    filter: e.filter.clone(),
                    size: e.size,
                    temp_c: median(es.iter().filter_map(|e| e.sensor_temp_c).collect()),
                    cooled: es.iter().any(|e| e.set_temp_c.is_some()),
                    subs: es.len(),
                    integration_s: es.iter().filter_map(|e| e.exposure_s).sum(),
                    nights,
                    darks: none(CalKind::Dark),
                    flats: none(CalKind::Flat),
                    bias: none(CalKind::Bias),
                    status: GroupStatus::Missing,
                    why: None,
                },
                es,
            )
        })
        .collect()
}

fn describe(g: &LightGroup) -> String {
    let mut p = Vec::new();
    if let Some(x) = g.gain {
        p.push(format!("Gain {}", num(x)));
    }
    if let Some(x) = g.exposure_s {
        p.push(format!("{} s", num(x)));
    }
    if let Some(x) = &g.filter {
        p.push(x.clone());
    }
    p.join(" · ")
}

fn explain(g: &LightGroup, total_s: f64) -> Option<String> {
    if g.status == GroupStatus::Ready {
        return None;
    }
    let mut why: Vec<String> = Vec::new();
    let mut todo: Vec<String> = Vec::new();
    let gain = g
        .gain
        .map_or("the lights' gain".into(), |x| format!("gain {}", num(x)));
    let temp = g
        .temp_c
        .map_or(String::new(), |t| format!(", ~{} °C", t.round()));
    let exp = g
        .exposure_s
        .map_or(String::new(), |x| format!(", {} s", num(x)));
    let with_filter = g
        .filter
        .as_ref()
        .map_or(String::new(), |f| format!(" with the {f} filter"));
    // Frames from another camera or at another image size can never be used.
    let other_camera = |k: &KindMatch| {
        k.notes
            .iter()
            .any(|n| n.starts_with("different camera") || n.starts_with("size "))
    };

    // Nothing from this camera: say so once instead of listing every difference.
    if g.darks.set.is_some()
        && [&g.darks, &g.flats]
            .iter()
            .all(|k| k.set.is_none() || other_camera(k))
    {
        why.push(format!(
            "the library has no calibration frames from this camera{}",
            g.camera
                .as_deref()
                .map_or(" and image size".to_string(), |c| format!(" ({c})"))
        ));
        todo.push(format!("shoot 20–30 darks at {gain}{exp}{temp}"));
        todo.push(format!("flats{with_filter}"));
        todo.push(format!("bias at {gain}"));
        return Some(finish(why, todo, g, total_s));
    }
    let subs = format!(
        "{}{}",
        if g.subs == 1 {
            "the sub".to_string()
        } else {
            format!("the {} subs", g.subs)
        },
        match g.nights.len() {
            0 => String::new(),
            1 => format!(" from {}", g.nights[0]),
            n => format!(" from {n} nights"),
        }
    );
    match g.darks.status {
        MatchStatus::None => {
            why.push("there are no darks in the library".into());
            todo.push(format!("shoot 20–30 darks at {gain}{exp}{temp}"));
        }
        MatchStatus::Bad => {
            why.push(format!(
                "{subs} were shot at {}; the closest darks ({}) differ: {}",
                describe(g),
                g.darks.set.as_ref().map_or("?", |s| s.name.as_str()),
                g.darks.notes.join(", ")
            ));
            why.push("mismatched darks leave amp glow and hot pixels".into());
            todo.push(format!("shoot 20–30 darks at {gain}{exp}{temp}"));
        }
        _ => {}
    }
    match g.flats.status {
        MatchStatus::None => {
            why.push("there are no flats".into());
            todo.push(format!("shoot flats{with_filter}"));
        }
        MatchStatus::Bad => {
            why.push(format!(
                "the closest flats differ: {}",
                g.flats.notes.join(", ")
            ));
            todo.push(format!(
                "shoot flats{with_filter} without changing focus or rotation"
            ));
        }
        _ => {}
    }
    let flats_ok = matches!(g.flats.status, MatchStatus::Ok | MatchStatus::Warn);
    if flats_ok && matches!(g.bias.status, MatchStatus::None | MatchStatus::Bad) {
        why.push(if g.bias.status == MatchStatus::None {
            "there is no bias or dark-flat for the flats".into()
        } else {
            format!("the bias differs: {}", g.bias.notes.join(", "))
        });
        let fg = g
            .flats
            .set
            .as_ref()
            .and_then(|s| s.gain)
            .map_or(gain.clone(), |x| format!("gain {}", num(x)));
        todo.push(format!(
            "shoot bias (or dark-flats at the flat exposure) at {fg}"
        ));
    }
    let s = finish(why, todo, g, total_s);
    (!s.is_empty()).then_some(s)
}

/// Sentence-case the reasons and the to-do list; suggest excluding the subs
/// only when they are a small share of the integration.
fn finish(why: Vec<String>, todo: Vec<String>, g: &LightGroup, total_s: f64) -> String {
    let cap = |t: String| match t.get(..1) {
        Some(c) => c.to_uppercase() + &t[1..],
        None => t,
    };
    let pct = if total_s > 0.0 {
        100.0 * g.integration_s / total_s
    } else {
        0.0
    };
    let mut s = String::new();
    if !why.is_empty() {
        s.push_str(&cap(why.join("; ")));
        s.push_str(". ");
    }
    if !todo.is_empty() {
        s.push_str(&cap(todo.join(", ")));
        if pct < 25.0 {
            s.push_str(&format!(
                ", or exclude these subs ({pct:.1}% of integration)"
            ));
        }
        s.push('.');
    }
    s.trim_end().to_string()
}

/// Match every light group in `lights` against the calibration `library`.
pub fn match_calibration(
    lights_label: &str,
    lights: &[Entry],
    library_label: &str,
    library: &[Entry],
) -> Matching {
    let sets = sets(library);
    let mut groups = Vec::new();
    let total_s: f64 = lights
        .iter()
        .filter(|e| e.frame == Some(FrameKind::Light) && !e.integrated)
        .filter_map(|e| e.exposure_s)
        .sum();
    for (mut g, _) in lights_groups(lights) {
        g.darks = best(&sets, &[CalKind::Dark], &g, |_| {
            (g.exposure_s, g.gain, g.offset)
        });
        g.flats = best(&sets, &[CalKind::Flat], &g, |_| (None, g.gain, g.offset));
        // Bias or dark-flats for the chosen flats (else for the lights).
        let (fg, fo, fe) = g.flats.set.as_ref().map_or((g.gain, g.offset, None), |f| {
            (f.gain, f.offset, f.exposure_s)
        });
        g.bias = best(&sets, &[CalKind::Bias, CalKind::DarkFlat], &g, |s| {
            (
                if s.kind == CalKind::DarkFlat {
                    fe
                } else {
                    None
                },
                fg,
                fo,
            )
        });
        let hard = |m: &KindMatch| matches!(m.status, MatchStatus::Bad | MatchStatus::None);
        // Bias only matters when flats need calibrating; darks can stand in.
        g.status = match (
            hard(&g.darks),
            hard(&g.flats),
            hard(&g.bias) && g.flats.set.is_some(),
        ) {
            (false, false, false) => GroupStatus::Ready,
            (true, true, _) => GroupStatus::Missing,
            _ => GroupStatus::Partial,
        };
        g.why = explain(&g, total_s);
        groups.push(g);
    }
    let count = |s: GroupStatus| groups.iter().filter(|g| g.status == s).count();
    Matching {
        schema: CALMATCH_SCHEMA,
        lights: lights_label.to_string(),
        library: library_label.to_string(),
        library_frames: library.iter().filter(|e| cal_kind(e).is_some()).count(),
        sets: sets.len(),
        ready: count(GroupStatus::Ready),
        partial: count(GroupStatus::Partial),
        missing: count(GroupStatus::Missing),
        groups,
    }
}

/// Overall severity of a group, for display.
pub fn severity(s: GroupStatus) -> Status {
    match s {
        GroupStatus::Ready => Status::Ok,
        GroupStatus::Partial => Status::Warn,
        GroupStatus::Missing => Status::Bad,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn corpus(dir: &str) -> Vec<Entry> {
        crate::list(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../testdata/synthetic")
                .join(dir),
        )
        .unwrap()
    }

    #[test]
    fn asiair_lights_match_the_library() {
        let lights: Vec<Entry> = corpus("asiair")
            .into_iter()
            .filter(|e| e.name.starts_with("Light"))
            .collect();
        let mut library = corpus("calibration");
        library.extend(
            corpus("asiair")
                .into_iter()
                .filter(|e| e.name.starts_with("Master")),
        );
        let m = match_calibration("asiair", &lights, "calibration", &library);
        assert_eq!(m.schema, CALMATCH_SCHEMA);
        assert_eq!(m.groups.len(), 1);
        let g = &m.groups[0];
        // The master dark (300 s, g100, −10 °C) wins over the single dark frame.
        assert_eq!(g.darks.status, MatchStatus::Ok, "{:?}", g.darks);
        assert!(g.darks.set.as_ref().unwrap().master);
        assert_eq!(g.flats.status, MatchStatus::Ok, "{:?}", g.flats);
        assert_eq!(
            g.flats.set.as_ref().unwrap().filter.as_deref(),
            Some("L-eXtreme")
        );
        // The bias frame from the same camera beats the Duo master bias.
        assert_eq!(g.bias.status, MatchStatus::Ok, "{:?}", g.bias);
        assert!(!g.bias.set.as_ref().unwrap().master);
        assert_eq!(g.status, GroupStatus::Ready);
        assert!(g.why.is_none());
        assert_eq!(m.ready, 1);
    }

    #[test]
    fn explains_mismatches() {
        let mut lights: Vec<Entry> = corpus("asiair")
            .into_iter()
            .filter(|e| e.name.starts_with("Light"))
            .collect();
        lights[0].gain = Some(120.0);
        let only_duo_bias: Vec<Entry> = corpus("asiair")
            .into_iter()
            .filter(|e| e.name.starts_with("Master"))
            .collect();
        let mut library: Vec<Entry> = corpus("calibration")
            .into_iter()
            .filter(|e| !e.name.starts_with("BIAS"))
            .collect();
        library.extend(only_duo_bias);
        let m = match_calibration("l", &lights, "lib", &library);
        let g = &m.groups[0];
        assert_eq!(g.darks.status, MatchStatus::Bad);
        assert!(
            g.darks
                .notes
                .iter()
                .any(|n| n.contains("gain 100 (needs 120, Δ 20)")),
            "{:?}",
            g.darks.notes
        );
        // Flats at another gain are usable with a caveat.
        assert_eq!(g.flats.status, MatchStatus::Warn);
        // The only bias is from a different camera.
        assert_eq!(g.bias.status, MatchStatus::Bad);
        assert!(g.bias.notes.iter().any(|n| n.contains("different camera")));
        assert_eq!(g.status, GroupStatus::Partial);
        let why = g.why.as_deref().unwrap();
        assert!(
            why.contains("shoot 20–30 darks at gain 120, 300 s, ~-10 °C")
                || why.contains("darks at gain 120"),
            "{why}"
        );
        // All of the integration: no "exclude them" hint.
        assert!(!why.contains("% of integration"), "{why}");
    }

    #[test]
    fn empty_library() {
        let lights: Vec<Entry> = corpus("asiair")
            .into_iter()
            .filter(|e| e.name.starts_with("Light"))
            .collect();
        let m = match_calibration("l", &lights, "lib", &[]);
        assert_eq!(m.groups[0].status, GroupStatus::Missing);
        assert!(m.groups[0].why.as_deref().unwrap().contains("no darks"));
    }
}
