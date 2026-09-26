//! Sub grader: measure every light sub (stars, HFR, background, shape,
//! trails) and suggest rejects against robust per-group thresholds.
//! Measurement only; moving rejects is a separate, explicit step.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fittle_image::stars::{FrameStats, measure_file};
use rayon::prelude::*;
use serde::Serialize;

use crate::Entry;

/// Schema id for `fittle grade --json` and `fits_grade_subs`.
pub const GRADE_SCHEMA: &str = "fittle.grade/1";

/// Why a sub is flagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// Few stars (and usually softer): cloud or haze.
    Clouds,
    /// Few stars with the target low (below 35°): extinction and airmass.
    LowAltitude,
    /// Bright background, typically twilight or the Moon.
    Dawn,
    /// Stars elongated: tracking or wind.
    Trailing,
    /// Stars soft: focus or seeing.
    Soft,
    /// A satellite or plane streak (flag only by default).
    Satellite,
    /// Could not be read or measured.
    Unreadable,
}

/// Limits a sub is judged against (per group of target/filter/exposure).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Thresholds {
    pub object: String,
    pub filter: String,
    pub exposure_s: Option<f64>,
    pub subs: usize,
    pub median_hfr: Option<f32>,
    pub median_stars: f32,
    pub median_background: f32,
    /// Reject above.
    pub hfr_max: Option<f32>,
    /// Reject below.
    pub stars_min: f32,
    pub background_max: f32,
    pub eccentricity_max: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SubGrade {
    pub path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_obs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub night: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<FrameStats>,
    /// Sun altitude at mid-exposure, degrees (derived), if the site is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sun_altitude: Option<f64>,
    /// Target altitude, degrees (derived), if site and target are known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub altitude: Option<f64>,
    pub reject: bool,
    pub reasons: Vec<Reason>,
    /// Worst ratio to a limit (1 = at the limit); sorts the worst first.
    pub badness: f32,
    /// Index into `Grading::groups`.
    pub group: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Grading {
    pub schema: &'static str,
    pub folder: String,
    pub subs: Vec<SubGrade>,
    pub groups: Vec<Thresholds>,
    pub kept: usize,
    pub rejected: usize,
    pub flagged_trails: usize,
    pub captured_s: f64,
    pub usable_s: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_hfr: Option<f32>,
    pub elapsed_ms: u64,
}

/// User overrides, e.g. `hfr>3.5,stars<50,bg>0.2,ecc>0.6`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rules {
    pub hfr_max: Option<f32>,
    pub stars_min: Option<f32>,
    pub background_max: Option<f32>,
    pub eccentricity_max: Option<f32>,
    /// Reject subs with a satellite trail too.
    pub reject_trails: bool,
    /// Robust-threshold width in sigmas (default 3).
    pub k: Option<f32>,
}

impl Rules {
    pub fn parse(s: &str) -> Result<Rules, String> {
        let mut r = Rules::default();
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            if part == "trails" || part == "satellite" {
                r.reject_trails = true;
                continue;
            }
            let (key, op, val) = ["<", ">"]
                .iter()
                .find_map(|op| part.split_once(op).map(|(k, v)| (k.trim(), *op, v.trim())))
                .ok_or_else(|| format!("'{part}': expected e.g. hfr>3.5 or stars<50"))?;
            let v: f32 = val
                .parse()
                .map_err(|_| format!("'{part}': '{val}' is not a number"))?;
            match (key, op) {
                ("hfr", ">") => r.hfr_max = Some(v),
                ("stars", "<") => r.stars_min = Some(v),
                ("bg" | "background", ">") => r.background_max = Some(v),
                ("ecc" | "eccentricity", ">") => r.eccentricity_max = Some(v),
                _ => return Err(format!("'{part}': use hfr>, stars<, bg> or ecc>")),
            }
        }
        Ok(r)
    }
}

fn median(v: &mut [f32]) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    let m = v.len() / 2;
    Some(*v.select_nth_unstable_by(m, f32::total_cmp).1)
}

/// Median and MAD-sigma.
fn robust(mut v: Vec<f32>) -> Option<(f32, f32)> {
    let m = median(&mut v)?;
    let mut d: Vec<f32> = v.iter().map(|x| (x - m).abs()).collect();
    Some((m, median(&mut d).unwrap_or(0.0) * 1.4826))
}

struct Measured {
    entry: Entry,
    stats: Option<FrameStats>,
    sun: Option<f64>,
    altitude: Option<f64>,
}

/// Grade the light subs among `entries` (other frames are ignored).
pub fn grade(folder: &str, entries: &[Entry], rules: &Rules) -> Grading {
    let t = std::time::Instant::now();
    let lights: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.frame == Some(fittle_core::classify::FrameKind::Light) && !e.integrated)
        .collect();
    let measured: Vec<Measured> = lights
        .par_iter()
        .map(|e| {
            let (stats, sun, altitude) = match measure_file(Path::new(&e.path)) {
                Ok((info, s)) => (
                    Some(s),
                    info.derived.sun_altitude.map(|f| f.value),
                    info.derived.altitude.map(|f| f.value),
                ),
                Err(_) => (None, None, None),
            };
            Measured {
                entry: (*e).clone(),
                stats,
                sun,
                altitude,
            }
        })
        .collect();

    // Groups: target × filter × sub length.
    let key = |e: &Entry| {
        (
            e.object.clone().unwrap_or_else(|| "(no OBJECT)".into()),
            e.filter.clone().unwrap_or_else(|| "(no filter)".into()),
            e.exposure_s.map(|x| (x * 1000.0).round() as i64),
        )
    };
    let mut by_group: BTreeMap<(String, String, Option<i64>), Vec<usize>> = BTreeMap::new();
    for (i, m) in measured.iter().enumerate() {
        by_group.entry(key(&m.entry)).or_default().push(i);
    }
    let k = rules.k.unwrap_or(3.0);
    let mut groups = Vec::new();
    let mut group_of = vec![0usize; measured.len()];
    for ((object, filter, exp), idx) in &by_group {
        let stats: Vec<&FrameStats> = idx
            .iter()
            .filter_map(|&i| measured[i].stats.as_ref())
            .collect();
        let hfr = robust(stats.iter().filter_map(|s| s.hfr).collect());
        let stars = robust(stats.iter().map(|s| s.stars as f32).collect()).unwrap_or((0.0, 0.0));
        let bg = robust(stats.iter().map(|s| s.background).collect()).unwrap_or((0.0, 0.0));
        let ecc = robust(stats.iter().filter_map(|s| s.eccentricity).collect());
        let th = Thresholds {
            object: object.clone(),
            filter: filter.clone(),
            exposure_s: exp.map(|e| e as f64 / 1000.0),
            subs: idx.len(),
            median_hfr: hfr.map(|h| h.0),
            median_stars: stars.0,
            median_background: bg.0,
            hfr_max: rules
                .hfr_max
                .or(hfr.map(|(m, s)| (m + k * s).max(m * 1.25))),
            stars_min: rules
                .stars_min
                .unwrap_or((stars.0 - k * stars.1).max(stars.0 * 0.5)),
            background_max: rules
                .background_max
                .unwrap_or((bg.0 + k * bg.1).max(bg.0 * 1.5)),
            eccentricity_max: rules
                .eccentricity_max
                // Small stars read somewhat elongated anyway; real trailing is a
                // clear step above the session, not its tail.
                .or(ecc.map(|(m, s)| (m + k * s).max(m + 0.2).min(0.95))),
        };
        for &i in idx {
            group_of[i] = groups.len();
        }
        groups.push(th);
    }

    let mut subs: Vec<SubGrade> = measured
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let th = &groups[group_of[i]];
            let mut reasons = Vec::new();
            let badness = match &m.stats {
                None => {
                    reasons.push(Reason::Unreadable);
                    10.0
                }
                Some(s) => {
                    let few = (s.stars as f32) < th.stars_min;
                    let bright = s.background > th.background_max;
                    let soft = th.hfr_max.zip(s.hfr).is_some_and(|(max, h)| h > max);
                    let long = th
                        .eccentricity_max
                        .zip(s.eccentricity)
                        .is_some_and(|(max, e)| e > max);
                    let twilight = m.sun.is_some_and(|a| a > -18.0);
                    if bright && (twilight || !few) {
                        reasons.push(Reason::Dawn);
                    }
                    if few && !(bright && twilight) {
                        let low = m.altitude.is_some_and(|a| a < 35.0);
                        reasons.push(if low {
                            Reason::LowAltitude
                        } else {
                            Reason::Clouds
                        });
                    }
                    if long {
                        reasons.push(Reason::Trailing);
                    }
                    if soft && !few && !long {
                        reasons.push(Reason::Soft);
                    }
                    if s.trail.is_some() {
                        reasons.push(Reason::Satellite);
                    }
                    let ratio = |a: f32, b: f32| if b > 0.0 { a / b } else { 0.0 };
                    [
                        th.hfr_max.zip(s.hfr).map_or(0.0, |(max, h)| ratio(h, max)),
                        ratio(th.stars_min, s.stars as f32 + 0.5),
                        ratio(s.background, th.background_max),
                        th.eccentricity_max
                            .zip(s.eccentricity)
                            .map_or(0.0, |(max, e)| ratio(e, max)),
                    ]
                    .into_iter()
                    .fold(0.0, f32::max)
                }
            };
            let reject = reasons
                .iter()
                .any(|r| *r != Reason::Satellite || rules.reject_trails);
            SubGrade {
                path: m.entry.path.clone(),
                name: m.entry.name.clone(),
                date_obs: m.entry.date_obs.clone(),
                night: m.entry.night.clone(),
                exposure_s: m.entry.exposure_s,
                stats: m.stats,
                sun_altitude: m.sun,
                altitude: m.altitude,
                reject,
                reasons,
                badness,
                group: group_of[i],
            }
        })
        .collect();
    // Capture order.
    subs.sort_by(|a, b| a.date_obs.cmp(&b.date_obs).then(a.path.cmp(&b.path)));

    let rejected = subs.iter().filter(|s| s.reject).count();
    let captured_s = subs.iter().filter_map(|s| s.exposure_s).sum();
    let usable_s = subs
        .iter()
        .filter(|s| !s.reject)
        .filter_map(|s| s.exposure_s)
        .sum();
    let mut hfrs: Vec<f32> = subs.iter().filter_map(|s| s.stats.as_ref()?.hfr).collect();
    Grading {
        schema: GRADE_SCHEMA,
        folder: folder.to_string(),
        kept: subs.len() - rejected,
        rejected,
        flagged_trails: subs
            .iter()
            .filter(|s| s.reasons.contains(&Reason::Satellite))
            .count(),
        captured_s,
        usable_s,
        median_hfr: median(&mut hfrs),
        subs,
        groups,
        elapsed_ms: t.elapsed().as_millis() as u64,
    }
}

/// Where a reject goes: `<its folder>/_rejected/<name>`.
pub fn rejected_path(path: &Path) -> PathBuf {
    let dir = path.parent().unwrap_or(Path::new("."));
    dir.join("_rejected")
        .join(path.file_name().unwrap_or_default())
}

/// One planned or completed move.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Move {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Move `paths` into `_rejected/` beside them (a rename on the same
/// volume; nothing is copied or deleted). Existing targets are never
/// replaced. With `dry_run`, only returns the plan.
pub fn move_rejects(paths: &[&str], dry_run: bool) -> Vec<Move> {
    paths
        .iter()
        .map(|p| {
            let from = Path::new(p);
            let to = rejected_path(from);
            let mut m = Move {
                from: p.to_string(),
                to: to.to_string_lossy().into_owned(),
                error: None,
            };
            if to.exists() {
                m.error = Some("already in _rejected/".into());
            } else if !dry_run {
                let r = std::fs::create_dir_all(to.parent().expect("has parent"))
                    .and_then(|_| std::fs::rename(from, &to));
                if let Err(e) = r {
                    m.error = Some(e.to_string());
                }
            }
            m
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_parse() {
        let r = Rules::parse("hfr>3.5, stars<50,bg>0.2,ecc>0.6,trails").unwrap();
        assert_eq!(r.hfr_max, Some(3.5));
        assert_eq!(r.stars_min, Some(50.0));
        assert_eq!(r.background_max, Some(0.2));
        assert_eq!(r.eccentricity_max, Some(0.6));
        assert!(r.reject_trails);
        assert!(Rules::parse("hfr<3").is_err());
        assert!(Rules::parse("hfr>x").is_err());
    }

    #[test]
    fn grades_and_moves() {
        let dir = std::env::temp_dir().join(format!("fittle-grade-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/synthetic/seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit");
        for i in 0..5 {
            std::fs::copy(&src, dir.join(format!("sub{i}.fit"))).unwrap();
        }
        let entries = crate::list(&dir).unwrap();
        let g = grade(&dir.to_string_lossy(), &entries, &Rules::default());
        assert_eq!(g.subs.len(), 5);
        assert_eq!(g.groups.len(), 1);
        // Identical subs: nothing rejected, everything usable.
        assert_eq!(g.rejected, 0, "{:?}", g.subs[0]);
        assert!((g.usable_s - 100.0).abs() < 1e-9);
        assert!(
            g.subs
                .iter()
                .all(|s| s.stats.as_ref().is_some_and(|st| st.stars > 0))
        );

        // A hard rule rejects them all; moves are planned, then done.
        let strict = grade(
            &dir.to_string_lossy(),
            &entries,
            &Rules {
                stars_min: Some(1e6),
                ..Rules::default()
            },
        );
        assert_eq!(strict.rejected, 5);
        assert!(strict.subs[0].reasons.contains(&Reason::Clouds));
        let paths: Vec<&str> = strict.subs.iter().map(|s| s.path.as_str()).collect();
        let plan = move_rejects(&paths, true);
        assert!(
            plan.iter()
                .all(|m| m.error.is_none() && m.to.contains("_rejected"))
        );
        assert!(dir.join("sub0.fit").exists());
        let done = move_rejects(&paths[..2], false);
        assert!(done.iter().all(|m| m.error.is_none()));
        assert!(dir.join("_rejected/sub0.fit").exists() && !dir.join("sub0.fit").exists());
        // Never replaces an existing reject.
        std::fs::copy(&src, dir.join("sub0.fit")).unwrap();
        assert!(move_rejects(&[paths[0]], false)[0].error.is_some());
    }
}
