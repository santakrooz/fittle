//! Organize and rename: move FITS files into folders built from their
//! headers (`{object}/{filter}/{night}`) and/or rename them from a template.
//! Plan first; applying only renames (never copies, never deletes), never
//! replaces a file, moves companion files (same stem, e.g. Seestar's .jpg)
//! along, and writes an undo manifest.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Entry;

/// Schema id for `fittle organize --json` and `fits_organize`.
pub const ORGANIZE_SCHEMA: &str = "fittle.organize/1";

/// Sidecar files that travel with a FITS file of the same stem.
const COMPANIONS: [&str; 7] = ["jpg", "jpeg", "png", "tif", "tiff", "txt", "json"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Spec {
    /// Folder template relative to the root, e.g. `{object}/{filter}/{night}`.
    /// Empty keeps files in their folder.
    #[serde(default)]
    pub by: String,
    /// File-name template (extension kept), e.g. `{object}_{filter}_{exptime}s_{seq}`.
    #[serde(default)]
    pub rename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Move {
    pub from: String,
    pub to: String,
    /// Moved because a FITS file with the same stem moved.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub companion: bool,
    /// Why the name differs from the template (a clash was avoided).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Set after applying, if this move failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub schema: String,
    pub root: String,
    pub spec: Spec,
    pub moves: Vec<Move>,
    /// Files already where the template puts them.
    pub unchanged: usize,
    /// Folders that will be created.
    pub new_folders: Vec<String>,
    /// Template tokens some files lack (written as `unknown`).
    pub missing_tokens: Vec<String>,
    /// Set after applying: the undo manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<String>,
}

fn stem_ext(p: &Path) -> (String, String) {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // `x.fits.fz` keeps both parts as the extension.
    for multi in [".fits.fz", ".fit.fz", ".fts.fz"] {
        if let Some(s) = name.strip_suffix(multi) {
            return (s.to_string(), multi[1..].to_string());
        }
    }
    match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), e.to_string()),
        None => (name, String::new()),
    }
}

/// Companion files per folder, keyed by lower-case stem, from the actual
/// directory listing (so case-insensitive file systems don't double up).
fn companion_index(dirs: &BTreeSet<PathBuf>) -> BTreeMap<(PathBuf, String), Vec<PathBuf>> {
    let mut out: BTreeMap<(PathBuf, String), Vec<PathBuf>> = BTreeMap::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let (stem, ext) = stem_ext(&p);
            if p.is_file() && COMPANIONS.contains(&ext.to_lowercase().as_str()) {
                out.entry((d.clone(), stem.to_lowercase()))
                    .or_default()
                    .push(p);
            }
        }
    }
    out
}

/// Plan the moves for `entries` (FITS files under `root`).
pub fn plan(root: &Path, entries: &[Entry], spec: &Spec) -> Plan {
    // Header facts per file, in capture order.
    let mut infos: Vec<(PathBuf, Option<fittle_core::Info>)> = entries
        .par_iter()
        .map(|e| (PathBuf::from(&e.path), fittle_core::info(&e.path).ok()))
        .collect();
    infos.sort_by(|a, b| {
        let d = |i: &Option<fittle_core::Info>| {
            i.as_ref()
                .and_then(|i| i.fields.date_obs.as_ref().map(|d| d.value.clone()))
        };
        d(&a.1).cmp(&d(&b.1)).then(a.0.cmp(&b.0))
    });

    let mut missing = BTreeSet::new();
    let mut seq: BTreeMap<PathBuf, usize> = BTreeMap::new();
    let mut taken: BTreeSet<PathBuf> = BTreeSet::new();
    let sources: BTreeSet<PathBuf> = infos.iter().map(|(p, _)| p.clone()).collect();
    let mut moves = Vec::new();
    let mut unchanged = 0;
    let mut folders = BTreeSet::new();
    let dirs: BTreeSet<PathBuf> = infos
        .iter()
        .filter_map(|(p, _)| p.parent().map(Path::to_path_buf))
        .collect();
    let index = companion_index(&dirs);

    for (path, info) in &infos {
        let (stem, ext) = stem_ext(path);
        let render = |t: &str, n: usize, missing: &mut BTreeSet<String>| -> Option<String> {
            let info = info.as_ref()?;
            let (s, miss) = fittle_core::naming::render(t, info, n);
            missing.extend(miss);
            Some(s)
        };
        // Destination folder.
        let dir = if spec.by.trim().is_empty() {
            path.parent().unwrap_or(root).to_path_buf()
        } else {
            let mut d = root.to_path_buf();
            for seg in spec.by.split('/').map(str::trim).filter(|s| !s.is_empty()) {
                d = d.join(render(seg, 1, &mut missing).unwrap_or_else(|| "unreadable".into()));
            }
            d
        };
        // Destination name.
        let n = {
            let c = seq.entry(dir.clone()).or_insert(0);
            *c += 1;
            *c
        };
        let new_stem = match &spec.rename {
            Some(t) if !t.trim().is_empty() => {
                render(t, n, &mut missing).unwrap_or_else(|| stem.clone())
            }
            _ => stem.clone(),
        };
        let file = |s: &str| {
            if ext.is_empty() {
                s.to_string()
            } else {
                format!("{s}.{ext}")
            }
        };
        let mut to = dir.join(file(&new_stem));
        if &to == path {
            unchanged += 1;
            taken.insert(to);
            continue;
        }
        // Never land on an existing file or another planned target.
        let mut note = None;
        let mut k = 2;
        while taken.contains(&to) || (to.exists() && !sources.contains(&to)) {
            to = dir.join(file(&format!("{new_stem}_{k}")));
            note = Some("renamed with a suffix to avoid a clash".to_string());
            k += 1;
        }
        taken.insert(to.clone());
        if !dir.exists() {
            folders.insert(dir.to_string_lossy().into_owned());
        }
        let (to_stem, _) = stem_ext(&to);
        let key = (
            path.parent().unwrap_or(root).to_path_buf(),
            stem.to_lowercase(),
        );
        for c in index.get(&key).into_iter().flatten() {
            let (_, cext) = stem_ext(c);
            moves.push(Move {
                from: c.to_string_lossy().into_owned(),
                to: dir
                    .join(format!("{to_stem}.{cext}"))
                    .to_string_lossy()
                    .into_owned(),
                companion: true,
                note: None,
                error: None,
            });
        }
        moves.push(Move {
            from: path.to_string_lossy().into_owned(),
            to: to.to_string_lossy().into_owned(),
            companion: false,
            note,
            error: None,
        });
    }
    Plan {
        schema: ORGANIZE_SCHEMA.into(),
        root: root.to_string_lossy().into_owned(),
        spec: spec.clone(),
        moves,
        unchanged,
        new_folders: folders.into_iter().collect(),
        missing_tokens: missing.into_iter().collect(),
        manifest: None,
    }
}

/// Carry out a plan: rename each file (creating folders), never replacing
/// anything. Moves whose target appeared meanwhile are skipped with an
/// error. Writes `fittle-organize-<time>.json` in the root to undo.
pub fn apply(mut plan: Plan) -> Plan {
    // Two-phase for rename chains (a→b while b→c): park files first.
    let parked: Vec<Option<PathBuf>> = plan
        .moves
        .iter_mut()
        .map(|m| {
            let from = PathBuf::from(&m.from);
            let park = from.with_file_name(format!(
                ".fittle-moving-{}",
                from.file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            ));
            match std::fs::rename(&from, &park) {
                Ok(()) => Some(park),
                Err(e) => {
                    m.error = Some(e.to_string());
                    None
                }
            }
        })
        .collect();
    for (m, park) in plan.moves.iter_mut().zip(parked) {
        let Some(park) = park else { continue };
        let to = PathBuf::from(&m.to);
        let res = (|| {
            if let Some(d) = to.parent() {
                std::fs::create_dir_all(d)?;
            }
            if to.exists() {
                return Err(std::io::Error::other("target exists; left in place"));
            }
            std::fs::rename(&park, &to)
        })();
        if let Err(e) = res {
            m.error = Some(e.to_string());
            let _ = std::fs::rename(&park, &m.from);
        }
    }
    // Folders this run emptied go (remove_dir only succeeds when empty).
    let root = PathBuf::from(&plan.root);
    for m in plan.moves.iter().filter(|m| m.error.is_none()) {
        let mut d = Path::new(&m.from).parent();
        while let Some(dir) = d {
            if dir == root || !dir.starts_with(&root) || std::fs::remove_dir(dir).is_err() {
                break;
            }
            d = dir.parent();
        }
    }
    // Undo manifest (only the moves that happened), under a new name.
    let done: Vec<&Move> = plan.moves.iter().filter(|m| m.error.is_none()).collect();
    if !done.is_empty() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let manifest = Plan {
            moves: done.into_iter().cloned().collect(),
            manifest: None,
            ..plan.clone()
        };
        if let Ok(text) = serde_json::to_string_pretty(&manifest) {
            for n in 1..100 {
                let name = if n == 1 {
                    format!("fittle-organize-{stamp}.json")
                } else {
                    format!("fittle-organize-{stamp}_{n}.json")
                };
                let path = root.join(name);
                let created = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path);
                if let Ok(mut f) = created {
                    use std::io::Write;
                    if f.write_all(text.as_bytes()).is_ok() {
                        plan.manifest = Some(path.to_string_lossy().into_owned());
                    }
                    break;
                }
            }
        }
    }
    plan
}

/// The plan that reverses an applied manifest.
pub fn undo_plan(manifest: &Path) -> std::io::Result<Plan> {
    let text = std::fs::read_to_string(manifest)?;
    let mut p: Plan = serde_json::from_str(&text).map_err(std::io::Error::other)?;
    p.moves = p
        .moves
        .into_iter()
        .rev()
        .map(|m| Move {
            from: m.to,
            to: m.from,
            companion: m.companion,
            note: None,
            error: None,
        })
        .collect();
    p.new_folders.clear();
    p.manifest = None;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fittle-organize-{}-{}",
            std::process::id(),
            rand_suffix()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic");
        std::fs::copy(
            src.join("seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit"),
            dir.join("a.fit"),
        )
        .unwrap();
        std::fs::write(dir.join("a.jpg"), b"preview").unwrap();
        std::fs::copy(
            src.join("seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit"),
            dir.join("b.fit"),
        )
        .unwrap();
        std::fs::copy(
            src.join(
                "asiair/Light_M 31_300.0s_Bin1_2600MC_gain100_20260901-221500_-10.0C_0001.fit",
            ),
            dir.join("c.fit"),
        )
        .unwrap();
        dir
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    #[test]
    fn organize_apply_and_undo() {
        let dir = setup();
        let entries = crate::list(&dir).unwrap();
        let spec = Spec {
            by: "{object}/{filter}".into(),
            rename: Some("{object}_{exptime}s".into()),
        };
        let p = plan(&dir, &entries, &spec);
        assert_eq!(p.schema, ORGANIZE_SCHEMA);
        // Two identical subs: the second gets a suffix, not a clash.
        let veil: Vec<&Move> = p
            .moves
            .iter()
            .filter(|m| !m.companion && m.to.contains("NGC 6995"))
            .collect();
        assert_eq!(veil.len(), 2);
        assert!(
            veil[0].to.ends_with("NGC 6995/LP/NGC 6995_20s.fit"),
            "{}",
            veil[0].to
        );
        assert!(veil[1].to.ends_with("NGC 6995_20s_2.fit") && veil[1].note.is_some());
        // The .jpg follows its sub.
        assert!(
            p.moves
                .iter()
                .any(|m| m.companion && m.to.ends_with(".jpg"))
        );
        assert!(p.new_folders.iter().any(|f| f.ends_with("M 31/L-eXtreme")));
        // Planning touches nothing.
        assert!(dir.join("a.fit").exists());

        let done = apply(p);
        assert!(
            done.moves.iter().all(|m| m.error.is_none()),
            "{:?}",
            done.moves
        );
        assert!(!dir.join("a.fit").exists() && !dir.join("a.jpg").exists());
        assert!(dir.join("M 31/L-eXtreme/M 31_300s.fit").exists());
        let manifest = PathBuf::from(done.manifest.unwrap());

        let back = apply(undo_plan(&manifest).unwrap());
        assert!(
            back.moves.iter().all(|m| m.error.is_none()),
            "{:?}",
            back.moves
        );
        for f in ["a.fit", "a.jpg", "b.fit", "c.fit"] {
            assert!(dir.join(f).exists(), "{f} restored");
        }
        // Folders the run created are gone again; both manifests remain.
        assert!(!dir.join("NGC 6995").exists() && !dir.join("M 31").exists());
        assert!(manifest.exists() && Path::new(&back.manifest.unwrap()) != manifest);
    }

    #[test]
    fn rename_in_place_never_overwrites() {
        let dir = setup();
        std::fs::write(dir.join("NGC 6995.fit"), b"not ours").unwrap();
        let entries: Vec<Entry> = crate::list(&dir)
            .unwrap()
            .into_iter()
            .filter(|e| e.name == "a.fit")
            .collect();
        let p = plan(
            &dir,
            &entries,
            &Spec {
                by: String::new(),
                rename: Some("{object}".into()),
            },
        );
        let m = p.moves.iter().find(|m| !m.companion).unwrap();
        assert!(m.to.ends_with("NGC 6995_2.fit"), "{}", m.to);
        let done = apply(p);
        assert!(done.moves.iter().all(|m| m.error.is_none()));
        assert_eq!(
            std::fs::read(dir.join("NGC 6995.fit")).unwrap(),
            b"not ours"
        );
    }
}
