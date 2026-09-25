//! Opt-in regression check of `info` over the local real-file folder:
//! every file explains without panicking, and known folders get the
//! expected verdicts. Run with:
//!
//!     FITTLE_SAMPLES="/path/to/sample fits" cargo test -p fittle-core --release --test local_info -- --nocapture

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| {
            matches!(
                x.to_ascii_lowercase().as_str(),
                "fit" | "fits" | "fts" | "fz"
            )
        }) {
            out.push(p);
        }
    }
}

/// (folder, expected verdict label, minimum confidence). Folders are those in
/// the maintainer's sample set; missing folders are skipped.
const EXPECT: &[(&str, &str, u8)] = &[
    ("Mikes 281", "Light sub", 95),
    ("ELEPHAN1", "Light sub", 90),
    ("NGC 7380_sub rcastro", "Light sub", 95),
    ("NGC_7380_DSOStack", "Stacked light", 90),
    ("NGC_7380_LiveStack", "Stacked light", 90),
    ("Pleiades/lights", "Light sub", 90),
    ("Pleiades/darks", "Dark frame", 90),
    ("Pleiades/flats", "Flat frame", 90),
    ("Pleiades/biases", "Bias frame", 90),
];

#[test]
fn local_samples_explain() {
    let Some(root) = std::env::var_os("FITTLE_SAMPLES") else {
        eprintln!("FITTLE_SAMPLES not set; skipping");
        return;
    };
    let root = PathBuf::from(root);
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut tally: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut wrong = Vec::new();
    for f in &files {
        let info = fittle_core::info(f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let folder = f
            .parent()
            .unwrap()
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        *tally
            .entry((folder.clone(), info.verdict.label.clone()))
            .or_default() += 1;
        if let Some((_, label, min)) = EXPECT.iter().find(|(d, _, _)| *d == folder) {
            if info.verdict.label != *label || info.verdict.confidence < *min {
                wrong.push(format!(
                    "{}: {} ({})",
                    f.display(),
                    info.verdict.label,
                    info.verdict.confidence
                ));
            }
        }
    }
    for ((folder, label), n) in &tally {
        eprintln!("{n:>6}  {folder:<40} {label}");
    }
    assert!(
        wrong.is_empty(),
        "{} unexpected verdicts:\n{}",
        wrong.len(),
        wrong
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
