//! Batch editing across many files: how each keyword's values are spread
//! over a selection, and a dry run of queued edits over all of them.
//! Header-only reads; writing goes through `fittle_core::write::apply`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use fittle_core::Fits;
use fittle_core::edit::{Op, Options, Plan};
use rayon::prelude::*;
use serde::Serialize;

/// Schema id for `fittle batch --json` and the GUI.
pub const BATCH_SCHEMA: &str = "fittle.batch/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Spread {
    /// One value on every file.
    Same,
    /// A few distinct values.
    Mixed,
    /// Many numeric values (min … max).
    Range,
    /// Different on (nearly) every file, e.g. DATE-OBS.
    Unique,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValueCount {
    /// Value as written in the header.
    pub text: String,
    pub count: usize,
    /// Files with this value (only for mixed keys with few values).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeyDist {
    pub keyword: String,
    /// Files that have the keyword.
    pub present: usize,
    pub spread: Spread,
    /// Most common first (at most 8).
    pub values: Vec<ValueCount>,
    pub distinct: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Distribution {
    pub schema: &'static str,
    pub files: usize,
    pub unreadable: Vec<String>,
    pub keys: Vec<KeyDist>,
}

fn skip(k: &str) -> bool {
    matches!(
        k,
        "SIMPLE"
            | "XTENSION"
            | "BITPIX"
            | "NAXIS"
            | "EXTEND"
            | "PCOUNT"
            | "GCOUNT"
            | "BZERO"
            | "BSCALE"
            | "CHECKSUM"
            | "DATASUM"
            | "END"
            | "COMMENT"
            | "HISTORY"
            | ""
            | "ZIMAGE"
            | "ZBITPIX"
            | "ZCMPTYPE"
            | "TFIELDS"
    ) || [
        "NAXIS", "ZNAXIS", "ZTILE", "ZNAME", "ZVAL", "TTYPE", "TFORM",
    ]
    .iter()
    .any(|p| {
        k.starts_with(p) && k[p.len()..].bytes().all(|b| b.is_ascii_digit()) && k.len() > p.len()
    })
}

/// (keyword, value as written, numeric value) for one card.
type CardFact = (String, String, Option<f64>);
/// Per keyword, per value text: (count, paths, numeric value).
type Tally = BTreeMap<String, BTreeMap<String, (usize, Vec<String>, Option<f64>)>>;

/// How each keyword's values are spread over `paths` (image HDU of each).
pub fn distribution(paths: &[PathBuf]) -> Distribution {
    let heads: Vec<(PathBuf, Option<Vec<CardFact>>)> = paths
        .par_iter()
        .map(|p| {
            let cards = Fits::open(p).ok().map(|f| {
                let hdu = fittle_core::edit::default_hdu(&f);
                f.hdus[hdu]
                    .header()
                    .cards
                    .iter()
                    .filter(|c| !skip(&c.keyword) && !c.value.is_commentary())
                    .map(|c| {
                        let text = match &c.value {
                            fittle_core::Value::String(s) => format!("'{}'", s.trim_end()),
                            _ => c.value_text.trim().to_string(),
                        };
                        (c.keyword.clone(), text, c.value.as_f64())
                    })
                    .collect()
            });
            (p.clone(), cards)
        })
        .collect();
    let mut order: Vec<String> = Vec::new();
    let mut by: Tally = BTreeMap::new();
    let mut unreadable = Vec::new();
    for (path, cards) in &heads {
        let Some(cards) = cards else {
            unreadable.push(path.to_string_lossy().into_owned());
            continue;
        };
        let mut seen = std::collections::BTreeSet::new();
        for (k, text, num) in cards {
            if !seen.insert(k.clone()) {
                continue; // duplicate keyword: first wins, as readers do
            }
            if !by.contains_key(k) {
                order.push(k.clone());
            }
            let e = by
                .entry(k.clone())
                .or_default()
                .entry(text.clone())
                .or_insert((0, Vec::new(), *num));
            e.0 += 1;
            e.1.push(path.to_string_lossy().into_owned());
        }
    }
    let files = heads.len() - unreadable.len();
    let keys = order
        .into_iter()
        .map(|k| {
            let vals = &by[&k];
            let present: usize = vals.values().map(|v| v.0).sum();
            let nums: Vec<f64> = vals.values().filter_map(|v| v.2).collect();
            let numeric = nums.len() == vals.len();
            let distinct = vals.len();
            let spread = if distinct == 1 {
                Spread::Same
            } else if distinct * 10 >= present * 9 && present > 3 {
                Spread::Unique
            } else if numeric && distinct > 6 {
                Spread::Range
            } else {
                Spread::Mixed
            };
            let mut values: Vec<ValueCount> = vals
                .iter()
                .map(|(t, (c, paths, _))| ValueCount {
                    text: t.clone(),
                    count: *c,
                    // Paths let the GUI select "the 83 files at GAIN 120".
                    paths: if spread == Spread::Mixed && distinct <= 8 {
                        paths.clone()
                    } else {
                        Vec::new()
                    },
                })
                .collect();
            values.sort_by(|a, b| b.count.cmp(&a.count).then(a.text.cmp(&b.text)));
            values.truncate(8);
            KeyDist {
                keyword: k,
                present,
                spread,
                values,
                distinct,
                min: numeric.then(|| nums.iter().copied().fold(f64::MAX, f64::min)),
                max: numeric.then(|| nums.iter().copied().fold(f64::MIN, f64::max)),
            }
        })
        .collect();
    Distribution {
        schema: BATCH_SCHEMA,
        files,
        unreadable,
        keys,
    }
}

/// A dry run of `ops` over many files.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatchPlan {
    pub files: usize,
    /// Files whose header would change.
    pub changed: usize,
    /// Of those, rewritten in place (same header size).
    pub in_place: usize,
    /// Rewritten to a new file and renamed (header grows a block).
    pub rewrite: usize,
    /// Bytes of `.bak` copies that would be made (files without one yet).
    pub backup_bytes: u64,
    pub est_seconds: f64,
    /// Validation problems, per file; nothing is written while any exist.
    pub errors: Vec<(String, String)>,
    /// Distinct consequences and warnings across files.
    pub notes: Vec<String>,
    /// A few per-file plans.
    pub sample: Vec<Plan>,
    /// The same edit as a CLI command.
    pub cli: String,
}

fn quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-~=".contains(c))
    {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('"', "\\\""))
    }
}

/// `fittle set …` / `unset` / `rename-key` for these files and ops.
pub fn cli(paths: &[PathBuf], ops: &[Op], opts: &Options) -> String {
    // One folder and one extension: a glob keeps the command short.
    let dirs: std::collections::BTreeSet<_> = paths.iter().filter_map(|p| p.parent()).collect();
    let exts: std::collections::BTreeSet<_> = paths.iter().filter_map(|p| p.extension()).collect();
    let files = if paths.len() > 3 && dirs.len() == 1 && exts.len() == 1 {
        let d = dirs.iter().next().unwrap().to_string_lossy();
        format!(
            "{}/*.{}",
            quote(&d),
            exts.iter().next().unwrap().to_string_lossy()
        )
    } else {
        paths
            .iter()
            .map(|p| quote(&p.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let flags = format!(
        "{}{}",
        if opts.backup { "" } else { " --no-backup" },
        if opts.history { "" } else { " --no-history" }
    );
    let mut sets = Vec::new();
    let mut cmds = Vec::new();
    for op in ops {
        match op {
            Op::Set { key, value, .. } => {
                let v = match value {
                    fittle_core::edit::NewValue::String(s) => {
                        format!("'{}'", s.replace('\'', "''"))
                    }
                    fittle_core::edit::NewValue::Logical(b) => (if *b { "T" } else { "F" }).into(),
                    fittle_core::edit::NewValue::Integer(i) => i.to_string(),
                    fittle_core::edit::NewValue::Float(f) => {
                        let t = format!("{f}");
                        if t.contains('.') || t.contains('e') {
                            t
                        } else {
                            format!("{t}.0")
                        }
                    }
                    fittle_core::edit::NewValue::Auto(s) => s.clone(),
                };
                sets.push(quote(&format!("{key}={v}")));
            }
            Op::Unset { key } => cmds.push(format!("fittle unset {files} {key}{flags}")),
            Op::Rename { from, to } => {
                cmds.push(format!("fittle rename-key {files} {from} {to}{flags}"))
            }
            Op::History { .. } => {}
        }
    }
    if !sets.is_empty() {
        cmds.insert(0, format!("fittle set {files} {}{flags}", sets.join(" ")));
    }
    cmds.join("\n")
}

/// Plan `ops` on every file; nothing is written.
pub fn plan_batch(paths: &[PathBuf], ops: &[Op], opts: &Options) -> BatchPlan {
    let t = std::time::Instant::now();
    let plans: Vec<(PathBuf, Result<Plan, String>)> = paths
        .par_iter()
        .map(|p| {
            (
                p.clone(),
                fittle_core::edit::plan(p, ops, opts).map_err(|e| e.to_string()),
            )
        })
        .collect();
    let mut errors = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let (mut changed, mut in_place, mut rewrite, mut backup_bytes) = (0, 0, 0, 0u64);
    let mut sample = Vec::new();
    for (p, r) in plans {
        match r {
            Err(e) => errors.push((p.to_string_lossy().into_owned(), e)),
            Ok(plan) => {
                if plan.changes.is_empty() {
                    continue;
                }
                changed += 1;
                if plan.in_place {
                    in_place += 1;
                } else {
                    rewrite += 1;
                }
                let bak = PathBuf::from(format!("{}.bak", p.display()));
                if opts.backup && !bak.exists() {
                    backup_bytes += p.metadata().map_or(0, |m| m.len());
                }
                for n in plan.consequences.iter().chain(&plan.warnings) {
                    if !notes.contains(n) {
                        notes.push(n.clone());
                    }
                }
                if sample.len() < 3 {
                    sample.push(plan);
                }
            }
        }
    }
    // Planning reads each header once; writing adds a block write per file
    // (plus a full copy for rewrites and backups).
    let per_plan = t.elapsed().as_secs_f64() / paths.len().max(1) as f64;
    let copy_s = (backup_bytes as f64 + rewrite as f64 * 4e6) / 400e6;
    BatchPlan {
        files: paths.len(),
        changed,
        in_place,
        rewrite,
        backup_bytes,
        est_seconds: per_plan * changed as f64 * 2.0 + changed as f64 * 0.001 + copy_s,
        errors,
        notes,
        sample,
        cli: cli(paths, ops, opts),
    }
}

/// Apply `ops` to every file (after a clean plan). Returns per-file errors.
pub fn apply_batch(paths: &[PathBuf], ops: &[Op], opts: &Options) -> Vec<(String, String)> {
    let plan = plan_batch(paths, ops, opts);
    if !plan.errors.is_empty() {
        return plan.errors;
    }
    paths
        .par_iter()
        .filter_map(|p| {
            fittle_core::write::apply(p, ops, opts)
                .err()
                .map(|e| (p.to_string_lossy().into_owned(), e.to_string()))
        })
        .collect()
}

/// Paths from a folder (all FITS inside, recursively) or a list.
pub fn resolve(inputs: &[PathBuf]) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for i in inputs {
        if i.is_dir() {
            out.extend(
                crate::list_recursive(i)?
                    .into_iter()
                    .map(|e| PathBuf::from(e.path)),
            );
        } else {
            out.push(i.clone());
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fittle_core::edit::NewValue;

    fn copies(n: usize) -> (PathBuf, Vec<PathBuf>) {
        let dir = std::env::temp_dir().join(format!("fittle-batch-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/synthetic/seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit");
        let paths: Vec<PathBuf> = (0..n)
            .map(|i| {
                let p = dir.join(format!("s{i}.fit"));
                std::fs::copy(&src, &p).unwrap();
                p
            })
            .collect();
        (dir, paths)
    }

    #[test]
    fn distribution_and_batch() {
        let (_dir, paths) = copies(4);
        // Make one file differ.
        fittle_core::write::apply(
            &paths[3],
            &[Op::Set {
                key: "GAIN".into(),
                value: NewValue::Integer(120),
                comment: None,
            }],
            &Options {
                backup: false,
                history: false,
                checksum: false,
                hdu: None,
            },
        )
        .unwrap();
        let d = distribution(&paths);
        assert_eq!(d.files, 4);
        let gain = d.keys.iter().find(|k| k.keyword == "GAIN").unwrap();
        assert_eq!(gain.spread, Spread::Mixed);
        assert_eq!(gain.values[0].count, 3);
        assert_eq!(
            gain.values[1].paths,
            vec![paths[3].to_string_lossy().to_string()]
        );
        let obj = d.keys.iter().find(|k| k.keyword == "OBJECT").unwrap();
        assert_eq!(obj.spread, Spread::Same);
        assert!(
            !d.keys
                .iter()
                .any(|k| k.keyword == "NAXIS1" || k.keyword == "BITPIX")
        );

        let ops = vec![
            Op::Set {
                key: "FOCALLEN".into(),
                value: NewValue::Float(250.0),
                comment: None,
            },
            Op::Set {
                key: "OBSERVER".into(),
                value: NewValue::String("Mike".into()),
                comment: None,
            },
        ];
        let opts = Options {
            backup: true,
            history: true,
            checksum: true,
            hdu: None,
        };
        let before: Vec<Vec<u8>> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
        let plan = plan_batch(&paths, &ops, &opts);
        assert_eq!((plan.files, plan.changed), (4, 4));
        assert!(plan.errors.is_empty());
        assert!(plan.backup_bytes > 0);
        assert!(
            plan.cli.starts_with("fittle set ")
                && plan
                    .cli
                    .contains("/*.fit FOCALLEN=250.0 \"OBSERVER='Mike'\""),
            "{}",
            plan.cli
        );
        // Planning wrote nothing.
        assert!(
            paths
                .iter()
                .zip(&before)
                .all(|(p, b)| std::fs::read(p).unwrap() == *b)
        );
        assert!(apply_batch(&paths, &ops, &opts).is_empty());
        let d = distribution(&paths);
        assert_eq!(
            d.keys
                .iter()
                .find(|k| k.keyword == "OBSERVER")
                .unwrap()
                .values[0]
                .text,
            "'Mike'"
        );
    }
}
