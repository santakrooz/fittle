//! Opt-in: decode the first image of each file directly under the local sample
//! folder and one per subfolder, reporting timing. Run with:
//!
//!     FITTLE_SAMPLES="/path/to/sample fits" cargo test -p fittle-image --release --test local_decode -- --nocapture

use std::path::PathBuf;
use std::time::Instant;

use fittle_core::Fits;
use fittle_image::decode_hdu;

fn is_fits(p: &std::path::Path) -> bool {
    p.extension().and_then(|x| x.to_str()).is_some_and(|x| {
        matches!(
            x.to_ascii_lowercase().as_str(),
            "fit" | "fits" | "fts" | "fz"
        )
    })
}

#[test]
fn decode_local_samples() {
    let Some(root) = std::env::var_os("FITTLE_SAMPLES") else {
        eprintln!("FITTLE_SAMPLES not set; skipping");
        return;
    };
    let root = PathBuf::from(root);
    let mut picks: Vec<PathBuf> = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        let files: Vec<PathBuf> = entries
            .iter()
            .filter(|p| p.is_file() && is_fits(p))
            .cloned()
            .collect();
        if dir == root {
            picks.extend(files);
        } else if let Some(first) = files.into_iter().next() {
            picks.push(first);
        }
        dirs.extend(entries.into_iter().filter(|p| p.is_dir()));
    }

    for path in picks {
        let fits = Fits::open(&path).unwrap();
        let hdu = fits
            .hdus
            .iter()
            .find(|h| !h.shape.is_empty())
            .expect("image HDU");
        let t = Instant::now();
        let img = decode_hdu(&path, hdu).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let mp = (img.width * img.height * img.planes) as f64 / 1e6;
        eprintln!(
            "{:>8.1} ms  {:>6.1} MP  bitpix {:>3}  {}x{}x{}  {}",
            ms,
            mp,
            hdu.bitpix.unwrap_or(0),
            img.width,
            img.height,
            img.planes,
            path.strip_prefix(&root).unwrap().display()
        );
        assert!(img.data.iter().all(|v| v.is_finite()) || hdu.bitpix.is_some_and(|b| b < 0));
    }
}
