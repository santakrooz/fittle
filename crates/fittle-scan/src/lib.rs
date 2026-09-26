//! Folder scan, session report, sub grader, calibration matcher.
//!
//! List a folder's FITS files with a header-only summary (GUI file rail),
//! and summarize a folder: frame counts, nights, integration per target and
//! filter, and consistency warnings. Parallel; thousands of headers per second.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::Serialize;

/// FITS file extensions recognised in folders.
pub const EXTENSIONS: [&str; 4] = ["fit", "fits", "fts", "fz"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    /// Verdict label (`Light sub`, `Master dark`, …); `None` if unreadable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// `light`, `dark`, `flat`, `bias`, `dark_flat`, `unknown`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<fittle_core::classify::FrameKind>,
    pub integrated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_obs: Option<String>,
    /// Evening the session started (local solar date), derived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub night: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain: Option<f64>,
    /// Structural error (e.g. truncated), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn is_fits(p: &Path) -> bool {
    p.extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| EXTENSIONS.contains(&x.to_ascii_lowercase().as_str()))
}

/// FITS files directly inside `folder` (not recursive), sorted by name.
pub fn list(folder: impl AsRef<Path>) -> std::io::Result<Vec<Entry>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_fits(p))
        .collect();
    paths.sort();
    Ok(paths.par_iter().map(|p| entry(p)).collect())
}

/// FITS files in `folder` and every subfolder (hidden folders skipped),
/// sorted by path.
pub fn list_recursive(folder: impl AsRef<Path>) -> std::io::Result<Vec<Entry>> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for e in std::fs::read_dir(dir)?.flatten() {
            let p = e.path();
            let hidden = p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'));
            if hidden {
                continue;
            }
            if p.is_dir() {
                walk(&p, out)?;
            } else if p.is_file() && is_fits(&p) {
                out.push(p);
            }
        }
        Ok(())
    }
    let mut paths = Vec::new();
    walk(folder.as_ref(), &mut paths)?;
    paths.sort();
    Ok(paths.par_iter().map(|p| entry(p)).collect())
}

/// Light subs of one target through one filter.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Group {
    pub object: String,
    pub filter: String,
    pub subs: usize,
    /// Distinct sub lengths, seconds.
    pub exposures_s: Vec<f64>,
    /// Sum of sub exposures, seconds.
    pub integration_s: f64,
    pub nights: Vec<String>,
}

/// Schema id for `fittle scan --json` and the `fits_scan_folder` tool.
pub const SCAN_SCHEMA: &str = "fittle.scan/1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub schema: &'static str,
    pub folder: String,
    pub files: usize,
    pub bytes: u64,
    /// Files that could not be read or have structural errors.
    pub unreadable: usize,
    /// Count per frame kind; stacks (integrated files) counted separately.
    pub frames: BTreeMap<String, usize>,
    pub stacks: usize,
    pub nights: Vec<String>,
    /// Light subs grouped by target and filter.
    pub targets: Vec<Group>,
    pub total_light_s: f64,
    /// Inconsistencies worth a look (mixed gain or exposure, unreadable files).
    pub warnings: Vec<String>,
}

/// Light subs by (object, filter), with the nights they were taken.
type Groups<'a> = BTreeMap<(String, String), (Vec<&'a Entry>, BTreeSet<String>)>;

pub fn summarize(folder: &str, entries: &[Entry]) -> Summary {
    let mut frames: BTreeMap<String, usize> = BTreeMap::new();
    let mut nights = BTreeSet::new();
    let mut groups: Groups = BTreeMap::new();
    let mut stacks = 0;
    for e in entries {
        let Some(kind) = e.frame else { continue };
        if e.integrated {
            stacks += 1;
            continue;
        }
        let name = serde_json_kind(kind);
        *frames.entry(name.clone()).or_default() += 1;
        if let Some(n) = &e.night {
            nights.insert(n.clone());
        }
        if name == "light" {
            let key = (
                e.object.clone().unwrap_or_else(|| "(no OBJECT)".into()),
                e.filter.clone().unwrap_or_else(|| "(no filter)".into()),
            );
            let g = groups.entry(key).or_default();
            g.0.push(e);
            if let Some(n) = &e.night {
                g.1.insert(n.clone());
            }
        }
    }
    let mut warnings = Vec::new();
    let unreadable = entries
        .iter()
        .filter(|e| e.frame.is_none() || e.error.is_some())
        .count();
    if unreadable > 0 {
        warnings.push(format!(
            "{unreadable} file(s) could not be read or are damaged (e.g. truncated)"
        ));
    }
    let targets: Vec<Group> = groups
        .into_iter()
        .map(|((object, filter), (subs, nights))| {
            let mut exposures: Vec<f64> = subs.iter().filter_map(|e| e.exposure_s).collect();
            let integration_s = exposures.iter().sum();
            exposures.sort_by(f64::total_cmp);
            exposures.dedup();
            // Most common gain; report the others.
            let mut gains: BTreeMap<String, usize> = BTreeMap::new();
            for g in subs.iter().filter_map(|e| e.gain) {
                *gains.entry(format!("{g}")).or_default() += 1;
            }
            if gains.len() > 1 {
                let common = gains
                    .iter()
                    .max_by_key(|(_, n)| **n)
                    .map(|(g, _)| g.clone());
                for (g, n) in &gains {
                    if Some(g) != common.as_ref() {
                        warnings.push(format!(
                            "{object} {filter}: GAIN {g} on {n} of {} subs",
                            subs.len()
                        ));
                    }
                }
            }
            if exposures.len() > 1 {
                warnings.push(format!(
                    "{object} {filter}: mixed sub lengths {exposures:?} s"
                ));
            }
            Group {
                object,
                filter,
                subs: subs.len(),
                exposures_s: exposures,
                integration_s,
                nights: nights.into_iter().collect(),
            }
        })
        .collect();
    Summary {
        schema: SCAN_SCHEMA,
        folder: folder.to_string(),
        files: entries.len(),
        bytes: entries.iter().map(|e| e.bytes).sum(),
        unreadable,
        frames,
        stacks,
        nights: nights.into_iter().collect(),
        total_light_s: targets.iter().map(|t| t.integration_s).sum(),
        targets,
        warnings,
    }
}

fn serde_json_kind(k: fittle_core::classify::FrameKind) -> String {
    format!("{k:?}")
        .chars()
        .enumerate()
        .flat_map(|(i, c)| {
            let lower = c.to_ascii_lowercase();
            if c.is_ascii_uppercase() && i > 0 {
                vec!['_', lower]
            } else {
                vec![lower]
            }
        })
        .collect()
}

fn entry(p: &Path) -> Entry {
    let name = p
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().to_string());
    let bytes = p.metadata().map_or(0, |m| m.len());
    let mut e = Entry {
        path: p.to_string_lossy().to_string(),
        name,
        bytes,
        label: None,
        frame: None,
        integrated: false,
        exposure_s: None,
        stack_count: None,
        filter: None,
        object: None,
        date_obs: None,
        night: None,
        gain: None,
        error: None,
    };
    match fittle_core::info(p) {
        Ok(i) => {
            e.label = Some(i.verdict.label.clone());
            e.frame = Some(i.verdict.frame);
            e.integrated = i.verdict.integrated;
            e.exposure_s = i.fields.exposure_s.map(|f| f.value);
            e.stack_count = i.fields.stack_count.map(|f| f.value);
            e.filter = i.fields.filter.map(|f| f.value);
            e.object = i.fields.object.map(|f| f.value);
            e.date_obs = i.fields.date_obs.map(|f| f.value);
            e.night = i.derived.session_night.map(|f| f.value);
            e.gain = i.fields.gain.map(|f| f.value);
            e.error = i
                .health
                .iter()
                .find(|x| x.severity == fittle_core::Severity::Error)
                .map(|x| x.message.clone());
        }
        Err(err) => e.error = Some(err.to_string()),
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_corpus_folder() {
        let dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic/calibration");
        let entries = list(&dir).unwrap();
        assert_eq!(entries.len(), 4);
        assert!(
            entries
                .iter()
                .any(|e| e.label.as_deref() == Some("Master dark"))
        );
        assert!(entries.iter().all(|e| e.error.is_none()));
    }

    #[test]
    fn summarizes_corpus() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic");
        let entries = list_recursive(&root).unwrap();
        assert!(entries.len() > 30);
        let s = summarize(&root.to_string_lossy(), &entries);
        assert_eq!(s.schema, SCAN_SCHEMA);
        assert!(s.frames.get("light").copied().unwrap_or(0) >= 3);
        assert!(s.frames.contains_key("dark") && s.frames.contains_key("flat"));
        assert!(s.stacks >= 3);
        let veil = s.targets.iter().find(|t| t.object == "NGC 6995").unwrap();
        assert!(veil.filter == "LP" && veil.subs >= 1 && veil.integration_s >= 20.0);
        assert!(s.total_light_s > 0.0);
        // The malformed corpus has an unreadable file.
        assert!(s.unreadable >= 1 && s.warnings.iter().any(|w| w.contains("could not be read")));
    }
}
