//! Folder scan, session report, sub grader, calibration matcher.
//!
//! M2 scope: list a folder's FITS files with a header-only summary for the
//! GUI file rail. Parallel; thousands of headers per second.

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
}
