//! Golden snapshots of `fittle header --json` for every committed corpus file,
//! plus exit-code conventions.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic")
}

fn fittle(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fittle"))
        .args(args)
        .current_dir(corpus_root())
        .output()
        .unwrap()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x != "py" && x != "md") {
            out.push(p);
        }
    }
}

/// Every file in the corpus, as forward-slash paths relative to the root.
fn corpus_files() -> Vec<String> {
    let root = corpus_root();
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut rel: Vec<String> = files
        .iter()
        .map(|p| {
            p.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    rel.sort();
    rel
}

#[test]
fn header_json_snapshots() {
    let files = corpus_files();
    assert!(files.len() >= 20, "corpus looks incomplete: {files:?}");
    for rel in files {
        let out = fittle(&["header", "--json", &rel]);
        assert!(
            out.status.success(),
            "{rel}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(json["schema"], "fittle.header/1");
        let name = rel.replace(['/', ' '], "__");
        insta::assert_json_snapshot!(name, json);
    }
}

#[test]
fn info_json_snapshots() {
    for rel in corpus_files() {
        let out = fittle(&["info", "--json", &rel]);
        assert!(
            out.status.success(),
            "{rel}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(json["schema"], "fittle.info/1");
        let name = format!("info__{}", rel.replace(['/', ' '], "__"));
        insta::assert_json_snapshot!(name, json);
    }
}

#[test]
fn info_human_view() {
    let out = Command::new(env!("CARGO_BIN_EXE_fittle"))
        .args([
            "info",
            "seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit",
        ])
        .env("NO_COLOR", "1")
        .current_dir(corpus_root())
        .output()
        .unwrap();
    insta::assert_snapshot!("info_human_seestar", String::from_utf8(out.stdout).unwrap());
}

#[test]
fn diff_snapshots() {
    for (name, a, b) in [
        (
            "light_vs_dark",
            "asiair/Light_M 31_300.0s_Bin1_2600MC_gain100_20260901-221500_-10.0C_0001.fit",
            "calibration/DARK_300.00s_0001.fits",
        ),
        (
            "light_vs_master_dark",
            "seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit",
            "calibration/master_dark_300s_g100_-10C.fit",
        ),
    ] {
        let out = fittle(&["diff", "--json", a, b]);
        assert!(out.status.success());
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        insta::assert_json_snapshot!(format!("diff__{name}"), json);
    }
}

#[test]
fn grouped_and_raw_views() {
    let rel = "siril/r_pp_NGC6995_stacked.fit";
    let out = fittle(&["header", rel]);
    assert!(out.status.success());
    insta::assert_snapshot!("grouped_siril", String::from_utf8(out.stdout).unwrap());

    let out = fittle(&["header", "--raw", rel]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.lines().all(|l| l.len() == 80),
        "raw records must be 80 chars"
    );
    assert!(text.lines().last().unwrap().starts_with("END "));
}

#[test]
fn filters() {
    let out = fittle(&[
        "header",
        "--json",
        "--hdu",
        "2",
        "--grep",
        "ttype",
        "edge/multi-hdu.fits",
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let hdus = json["hdus"].as_array().unwrap();
    assert_eq!(hdus.len(), 1);
    let keys: Vec<&str> = hdus[0]["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["keyword"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["TTYPE1", "TTYPE2", "TTYPE3"]);
}

#[test]
fn exit_codes() {
    assert_eq!(
        fittle(&["header", "does-not-exist.fit"]).status.code(),
        Some(1)
    );
    assert_eq!(
        fittle(&["header", "--hdu", "9", "edge/multi-hdu.fits"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        fittle(&["header", "--raw", "--json", "edge/multi-hdu.fits"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        fittle(&["header", "malformed/truncated.fit"]).status.code(),
        Some(0)
    );
}
