//! Opt-in budget check on real data (PLAN M6: 3,785 Seestar subs scanned and
//! graded in < 60 s, i.e. < ~16 ms per sub including listing):
//!
//!     FITTLE_SAMPLES="/path/to/sample fits" cargo test -p fittle-scan --release --test local_grade -- --nocapture

use std::path::PathBuf;
use std::time::Instant;

#[test]
fn grade_budget() {
    let Some(root) = std::env::var_os("FITTLE_SAMPLES") else {
        eprintln!("FITTLE_SAMPLES not set; skipping");
        return;
    };
    let root = PathBuf::from(root);
    let t = Instant::now();
    let entries = fittle_scan::list_recursive(&root).unwrap();
    let g = fittle_scan::grade::grade(&root.to_string_lossy(), &entries, &Default::default());
    let secs = t.elapsed().as_secs_f64();
    let per_sub_ms = secs * 1000.0 / g.subs.len().max(1) as f64;
    eprintln!(
        "{} files, {} subs graded in {secs:.1} s ({per_sub_ms:.1} ms/sub), {} rejects, median HFR {:?}",
        entries.len(),
        g.subs.len(),
        g.rejected,
        g.median_hfr
    );
    assert!(per_sub_ms < 16.0, "{per_sub_ms:.1} ms per sub");
}
