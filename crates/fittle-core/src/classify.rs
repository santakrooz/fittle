//! Frame classification with confidence and evidence.
//!
//! Two questions are scored independently from header facts:
//! 1. What kind of frame? (light / dark / flat / bias / dark-flat)
//! 2. Single exposure or integrated from many?
//!
//! Each piece of evidence has a weight; the verdict's confidence is the
//! weaker of the two answers. HISTORY lines add a processing checklist.

use serde::Serialize;

use crate::canonical::Canonical;
use crate::filename::FileNameFacts;
use crate::header::Header;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    Light,
    Dark,
    Flat,
    Bias,
    DarkFlat,
    Unknown,
}

impl FrameKind {
    fn noun(self) -> &'static str {
        match self {
            FrameKind::Light => "light",
            FrameKind::Dark => "dark",
            FrameKind::Flat => "flat",
            FrameKind::Bias => "bias",
            FrameKind::DarkFlat => "dark-flat",
            FrameKind::Unknown => "frame",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evidence {
    /// What was seen, e.g. `STACKCNT=795` or `no STACKCNT`.
    pub text: String,
    /// Which conclusion it supports: a frame kind, `single` or `integrated`.
    pub supports: String,
    pub weight: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    pub name: &'static str,
    /// The HISTORY line (shortened) that shows it.
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Verdict {
    /// Short label: `Light sub`, `Stacked light`, `Master dark`, …
    pub label: String,
    pub frame: FrameKind,
    pub integrated: bool,
    /// 0–100.
    pub confidence: u8,
    pub evidence: Vec<Evidence>,
    /// Processing steps found in HISTORY, in file order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub processing: Vec<Step>,
    /// HISTORY shows steps beyond calibration/registration/stacking.
    pub processed: bool,
    /// `false` when HISTORY shows a stretch (non-linear data); `None` if unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linear: Option<bool>,
}

pub struct Input<'a> {
    pub header: &'a Header,
    pub fields: &'a Canonical,
    pub file: &'a FileNameFacts,
    pub bitpix: Option<i64>,
    pub planes: u64,
}

/// Map a confidence sum to 0–100 (50 at zero, saturating).
fn confidence(score: i32) -> u8 {
    (50.0 + 50.0 * (score.abs() as f64 / 40.0).tanh()).round() as u8
}

fn frame_from_text(t: &str) -> Option<FrameKind> {
    let t = t.to_lowercase().replace(['-', '_', ' '], "");
    Some(if t.contains("darkflat") || t.contains("flatdark") {
        FrameKind::DarkFlat
    } else if t.contains("bias") || t.contains("offset") || t == "zero" {
        FrameKind::Bias
    } else if t.contains("dark") {
        FrameKind::Dark
    } else if t.contains("flat") {
        FrameKind::Flat
    } else if t.contains("light") || t.contains("object") || t.contains("science") {
        FrameKind::Light
    } else {
        return None;
    })
}

pub fn classify(inp: &Input) -> Verdict {
    let f = inp.fields;
    let mut evidence: Vec<Evidence> = Vec::new();
    let mut push = |text: String, supports: &str, weight: i32| {
        evidence.push(Evidence {
            text,
            supports: supports.to_string(),
            weight,
        })
    };

    // ---- 1. frame kind -------------------------------------------------
    let mut kinds: Vec<(FrameKind, i32)> = Vec::new();
    fn vote(kinds: &mut Vec<(FrameKind, i32)>, k: FrameKind, w: i32) {
        match kinds.iter_mut().find(|(x, _)| *x == k) {
            Some((_, s)) => *s += w,
            None => kinds.push((k, w)),
        }
    }
    let frame_text = f.frame_type.as_ref().filter(|t| t.keyword().is_some());
    let master_in_type = frame_text.is_some_and(|t| t.value.to_lowercase().contains("master"));
    if let Some(t) = frame_text {
        if let Some(k) = frame_from_text(&t.value) {
            vote(&mut kinds, k, 60);
            push(
                format!("{}={}", t.keyword().unwrap(), t.value),
                k.noun(),
                60,
            );
        }
    }
    if let Some(k) = inp.file.prefix.as_deref().and_then(frame_from_text) {
        vote(&mut kinds, k, 25);
        push(
            format!("file name starts '{}'", inp.file.prefix.as_deref().unwrap()),
            k.noun(),
            25,
        );
    }
    if let Some(e) = f.exposure_s.as_ref().filter(|e| e.value < 0.005) {
        vote(&mut kinds, FrameKind::Bias, 30);
        push(
            format!("{}={}", e.keyword().unwrap_or("exposure"), e.value),
            "bias",
            30,
        );
    }
    if kinds.is_empty() && (f.object.is_some() || f.ra.is_some()) {
        vote(&mut kinds, FrameKind::Light, 20);
        push("has a target (OBJECT / RA / DEC)".into(), "light", 20);
    }
    kinds.sort_by_key(|k| std::cmp::Reverse(k.1));

    // ---- 2. single vs integrated --------------------------------------
    let mut integ = 0;
    let mut add = |text: String, w: i32| {
        integ += w;
        evidence.push(Evidence {
            text,
            supports: if w > 0 { "integrated" } else { "single" }.into(),
            weight: w.abs(),
        })
    };
    match &f.stack_count {
        Some(n) if n.value > 1 => {
            let text = if n.keyword().is_some() {
                n.describe(n.value)
            } else {
                n.describe(format!("{} frames", n.value))
            };
            add(text, 50)
        }
        Some(n) => add(n.describe(n.value), -30),
        None => add("no STACKCNT / NCOMBINE".into(), -25),
    }
    if let (Some(t), Some(e)) = (&f.total_integration_s, &f.exposure_s) {
        if let Some(k) = t.keyword() {
            if e.value > 0.0 && t.value > 1.5 * e.value {
                add(format!("{k}={} > exposure {}", t.value, e.value), 25);
            } else if e.value > 0.0 && (t.value - e.value).abs() < 1e-6 {
                add(format!("{k} equals exposure"), -10);
            }
        }
    }
    if master_in_type {
        add("frame type says 'master'".into(), 40);
    }
    let history: Vec<&str> = inp.header.history().collect();
    if let Some(line) = history.iter().find(|l| is_stacking(l)) {
        add(format!("HISTORY '{}'", short(line)), 30);
    }
    match inp.file.prefix.as_deref() {
        Some(p @ ("stacked" | "dso_stacked" | "result" | "integration")) => {
            add(format!("file name starts '{p}'"), 20)
        }
        Some(p) if p.starts_with("master") => add(format!("file name starts '{p}'"), 20),
        _ => {}
    }
    match inp.bitpix {
        Some(b) if b < 0 => add(format!("BITPIX={b} (float)"), 10),
        Some(16) if f.stack_count.is_none() => add("BITPIX=16".into(), -20),
        _ => {}
    }
    if let Some(b) = &f.bayer {
        if inp.planes == 1 && f.stack_count.is_none() {
            add(
                format!(
                    "{}={} on a single-plane (raw CFA) image",
                    b.keyword().unwrap_or("BAYERPAT"),
                    b.value
                ),
                -15,
            );
        }
    }
    let integrated = integ > 0;
    // Stacks without a calibration frame type are stacked lights (stackers
    // label calibration masters; lights often carry no IMAGETYP at all).
    if kinds.is_empty() && integ >= 40 {
        kinds.push((FrameKind::Light, 20));
        evidence.push(Evidence {
            text: "integrated, no calibration frame type".into(),
            supports: "light".into(),
            weight: 20,
        });
    }
    let (frame, kind_score) = match kinds.as_slice() {
        [] => (FrameKind::Unknown, 0),
        [(k, s)] => (*k, *s),
        [(k, s), (_, s2), ..] => (*k, s - s2 / 2),
    };

    // ---- 3. processing -------------------------------------------------
    let processing = processing_steps(&history);
    let processed = processing
        .iter()
        .any(|s| !matches!(s.name, "stacking" | "registration" | "calibration" | "crop"));
    let stretched = processing.iter().any(|s| s.name == "stretch");

    let confidence = if frame == FrameKind::Unknown {
        0
    } else {
        confidence(kind_score).min(confidence(integ))
    };
    let label = label(frame, integrated, processed);
    evidence.sort_by_key(|e| std::cmp::Reverse(e.weight));
    let mut seen = std::collections::HashSet::new();
    evidence.retain(|e| seen.insert(e.text.clone()));
    Verdict {
        label,
        frame,
        integrated,
        confidence,
        evidence,
        processing,
        processed,
        linear: if stretched { Some(false) } else { None },
    }
}

fn label(frame: FrameKind, integrated: bool, processed: bool) -> String {
    let base = match (frame, integrated) {
        (FrameKind::Unknown, _) => "Unknown".to_string(),
        (FrameKind::Light, false) => "Light sub".into(),
        (FrameKind::Light, true) => "Stacked light".into(),
        (k, false) => format!("{} frame", capitalize(k.noun())),
        (k, true) => format!("Master {}", k.noun()),
    };
    if processed {
        format!("{base}, processed")
    } else {
        base
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn short(line: &str) -> String {
    let s: String = line.chars().take(48).collect();
    if line.chars().count() > 48 {
        format!("{s}…")
    } else {
        s
    }
}

fn is_stacking(line: &str) -> bool {
    let l = line.to_lowercase();
    l.contains("stacking")
        || l.contains("siril stack")
        || l.contains("imageintegration")
        || l.contains("integration")
}

/// Processing vocabulary (generic, not vendor-specific).
const STEPS: &[(&str, &[&str])] = &[
    ("calibration", &["calibrat", "preprocess"]),
    (
        "registration",
        &["registration", "register", "star alignment", "align"],
    ),
    (
        "stacking",
        &["stacking", "siril stack", "imageintegration", "integration"],
    ),
    ("crop", &["crop"]),
    (
        "background extraction",
        &[
            "background extraction",
            "bge",
            "graxpert",
            "dynamicbackground",
            "automaticbackground",
            "gradient",
        ],
    ),
    (
        "colour calibration",
        &[
            "spcc",
            "pcc",
            "photometric",
            "color calibration",
            "colour calibration",
        ],
    ),
    ("green removal", &["scnr"]),
    ("deconvolution", &["deconvol", "blurx"]),
    (
        "noise reduction",
        &["denois", "noise reduction", "noisex", "nxt"],
    ),
    ("star removal", &["starnet", "starx", "star removal"]),
    (
        "stretch",
        &[
            "stretch",
            "histogramtransformation",
            "histogram transformation",
            "ghs",
            "asinh",
            "curves",
            "mtf",
        ],
    ),
    ("saturation", &["saturation"]),
];

fn processing_steps(history: &[&str]) -> Vec<Step> {
    let mut out: Vec<Step> = Vec::new();
    for line in history {
        let l = line.to_lowercase();
        for (name, needles) in STEPS {
            if needles.iter().any(|n| l.contains(n)) && !out.iter().any(|s| s.name == *name) {
                out.push(Step {
                    name,
                    evidence: short(line),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_curve() {
        assert_eq!(confidence(0), 50);
        assert!(confidence(70) >= 96);
        assert!(confidence(-70) >= 96);
        assert!(confidence(20) < 80);
    }

    #[test]
    fn frame_words() {
        assert_eq!(frame_from_text("Light Frame"), Some(FrameKind::Light));
        assert_eq!(frame_from_text("Master Dark"), Some(FrameKind::Dark));
        assert_eq!(frame_from_text("DARKFLAT"), Some(FrameKind::DarkFlat));
        assert_eq!(frame_from_text("Bias"), Some(FrameKind::Bias));
        assert_eq!(frame_from_text("whatever"), None);
    }

    #[test]
    fn processing_vocab() {
        let steps = processing_steps(&[
            "mean stacking with winsorized sigma clipping",
            "GraXpert AI BGE: subtraction",
            "SCNR (type=average neutral)",
            "Deconvolution",
        ]);
        let names: Vec<_> = steps.iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            [
                "stacking",
                "background extraction",
                "green removal",
                "deconvolution"
            ]
        );
    }
}
