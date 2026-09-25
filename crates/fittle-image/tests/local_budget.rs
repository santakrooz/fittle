//! Opt-in timing of the GUI open path on real files (PLAN budget: 60 MP
//! open + auto-stretch < 1 s). Run in release:
//!
//!     FITTLE_SAMPLES="/path/to/sample fits" cargo test -p fittle-image --release --test local_budget -- --nocapture

use std::path::PathBuf;
use std::time::Instant;

use fittle_core::Fits;
use fittle_image::Opened;

#[test]
fn open_and_stretch_budget() {
    let Some(root) = std::env::var_os("FITTLE_SAMPLES") else {
        eprintln!("FITTLE_SAMPLES not set; skipping");
        return;
    };
    let root = PathBuf::from(root);
    let pleiades = root.join("Pleiades/lights");
    let first = std::fs::read_dir(&pleiades)
        .ok()
        .and_then(|mut d| d.next())
        .and_then(|e| e.ok())
        .map(|e| e.path());
    let files: Vec<PathBuf> = [Some(root.join("M31_302x10s_50min.fits")), first]
        .into_iter()
        .flatten()
        .filter(|p| p.exists())
        .collect();
    for path in files {
        let t = Instant::now();
        let fits = Fits::open(&path).unwrap();
        let hdu = fits.hdus.iter().find(|h| !h.shape.is_empty()).unwrap();
        let info = fittle_core::info_from(&fits, &path.to_string_lossy());
        let bayer = info
            .fields
            .bayer
            .as_ref()
            .map(|b| (b.value.as_str(), 0, 0, false));
        let t_header = t.elapsed();
        let opened = Opened::open(&path, hdu, bayer).unwrap();
        let t_open = t.elapsed();
        let mode = opened.info().default_mode;
        let d = opened.display(mode);
        let (w, h, bytes) = opened.preview(mode, 2560);
        let total = t.elapsed();
        let mp = (opened.width * opened.height * opened.planes) as f64 / 1e6;
        eprintln!(
            "{:>6.1} MP  header {:>5.1} ms  decode+stats {:>6.1} ms  {mode:?}+preview {:>6.1} ms  total {:>6.1} ms  → {w}×{h}×{} ({} MB f16)  {}",
            mp,
            t_header.as_secs_f64() * 1e3,
            (t_open - t_header).as_secs_f64() * 1e3,
            (total - t_open).as_secs_f64() * 1e3,
            total.as_secs_f64() * 1e3,
            d.channels,
            bytes.len() / 1_000_000,
            path.file_name().unwrap().to_string_lossy()
        );
        assert!(
            total.as_secs_f64() < 1.0 * (mp / 60.0).max(1.0),
            "over budget"
        );
    }
}

#[test]
fn thumbnail_budget() {
    let Some(root) = std::env::var_os("FITTLE_SAMPLES") else {
        return;
    };
    let Ok(rd) = std::fs::read_dir(PathBuf::from(root).join("Pleiades/lights")) else {
        return;
    };
    let files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).take(10).collect();
    let t = Instant::now();
    for f in &files {
        fittle_image::thumbnail(f, 96).expect("thumbnail");
    }
    let each = t.elapsed().as_secs_f64() * 1e3 / files.len() as f64;
    eprintln!("thumbnail: {each:.1} ms each (26 MP CFA, 96 px)");
    assert!(each < 50.0);
}
