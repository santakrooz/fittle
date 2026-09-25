//! Write `?demo` fixtures for the GUI from a folder of FITS files: the same
//! JSON and binary payloads the Tauri commands return. Local-only (real files
//! can carry site coordinates); the output directory is git-ignored.
//!
//!     cargo run --release -p fittle-image --example ui_fixtures -- <folder>...
//!
//! Takes up to `FITTLE_DEMO_MAX` (default 6) files from each folder.

use std::fs;
use std::path::PathBuf;

use fittle_core::{Fits, HEADER_SCHEMA, HeaderDoc};
use fittle_image::{Mode, ViewSession};

const PREVIEW_EDGE: usize = 2560;

fn packed(w: usize, h: usize, ch: usize, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    for v in [w, h, ch] {
        out.extend((v as u32).to_le_bytes());
    }
    out.extend_from_slice(data);
    out
}

fn main() {
    let folders: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    assert!(!folders.is_empty(), "usage: ui_fixtures <folder>...");
    let max: usize = std::env::var("FITTLE_DEMO_MAX")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(6);
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../app/demo-fixtures");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(out.join("files")).unwrap();

    let mut entries: Vec<_> = folders
        .iter()
        .flat_map(|f| {
            fittle_scan::list(f)
                .expect("readable folder")
                .into_iter()
                .take(max)
        })
        .collect();
    for (i, e) in entries.iter_mut().enumerate() {
        let dir = out.join("files").join(i.to_string());
        fs::create_dir_all(&dir).unwrap();
        let s = ViewSession::open(&e.path).expect("open");
        let opened = serde_json::json!({
            "info": s.info, "image": s.image.as_ref().map(|o| o.info()), "image_error": s.image_error, "wcs": s.wcs.is_some()
        });
        fs::write(dir.join("open.json"), serde_json::to_vec(&opened).unwrap()).unwrap();
        let fits = Fits::open(&e.path).unwrap();
        let doc = HeaderDoc {
            schema: HEADER_SCHEMA,
            path: &e.name,
            fits: &fits,
        };
        fs::write(dir.join("header.json"), serde_json::to_vec(&doc).unwrap()).unwrap();
        if let Some(o) = &s.image {
            let mut modes = vec![Mode::Raw];
            if o.cfa.is_some() {
                modes.push(Mode::Debayer);
            }
            for m in modes {
                let name = if m == Mode::Raw { "raw" } else { "debayer" };
                let d = o.display(m);
                fs::write(
                    dir.join(format!("display-{name}.json")),
                    serde_json::to_vec(d).unwrap(),
                )
                .unwrap();
                let (w, h, px) = o.preview(m, PREVIEW_EDGE);
                fs::write(
                    dir.join(format!("preview-{name}.bin")),
                    packed(w, h, d.channels, &px),
                )
                .unwrap();
            }
        }
        if let Some((w, h, px)) = fittle_image::thumbnail(&e.path, 96) {
            fs::write(dir.join("thumb.bin"), packed(w, h, 4, &px)).unwrap();
        }
        e.path = format!("demo:{i}");
        eprintln!("{i:>3}  {}", e.name);
    }
    let name = folders[0]
        .file_name()
        .map(|n| n.to_string_lossy().to_string());
    let list = serde_json::json!({ "folder": name, "entries": entries });
    fs::write(out.join("list.json"), serde_json::to_vec(&list).unwrap()).unwrap();
    fs::write(
        out.join("dictionary.json"),
        serde_json::to_vec(fittle_core::dict::KEYWORDS).unwrap(),
    )
    .unwrap();
    eprintln!("wrote {}", out.display());
}
