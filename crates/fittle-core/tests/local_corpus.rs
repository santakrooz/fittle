//! Opt-in check over a local folder of real FITS files that is too large (and
//! too private) to commit. Run with:
//!
//!     FITTLE_SAMPLES="/path/to/sample fits" cargo test -p fittle-core --release --test local_corpus -- --nocapture

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fittle_core::{Fits, Severity};

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

#[test]
fn every_local_sample_opens() {
    let Some(root) = std::env::var_os("FITTLE_SAMPLES") else {
        eprintln!("FITTLE_SAMPLES not set; skipping");
        return;
    };
    let root = PathBuf::from(root);
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "no FITS files under {}", root.display());

    let mut failures = Vec::new();
    let mut slowest = Duration::ZERO;
    let mut by_folder: BTreeMap<String, (usize, BTreeMap<String, usize>)> = BTreeMap::new();
    let start = Instant::now();
    for f in &files {
        let t = Instant::now();
        let fits = match Fits::open(f) {
            Ok(fits) => fits,
            Err(e) => {
                failures.push(format!("{}: {e}", f.display()));
                continue;
            }
        };
        slowest = slowest.max(t.elapsed());
        let folder = f
            .parent()
            .unwrap()
            .strip_prefix(&root)
            .unwrap()
            .display()
            .to_string();
        let entry = by_folder.entry(folder).or_default();
        entry.0 += 1;
        for i in &fits.issues {
            *entry.1.entry(i.code.clone()).or_default() += 1;
            if i.severity == Severity::Error {
                failures.push(format!("{}: {} {}", f.display(), i.code, i.message));
            }
        }
    }
    let total = start.elapsed();
    eprintln!(
        "{} files in {total:.2?} (slowest {slowest:.2?})",
        files.len()
    );
    for (folder, (n, issues)) in &by_folder {
        eprintln!("  {folder:<40} {n:>5} files  issues: {issues:?}");
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
