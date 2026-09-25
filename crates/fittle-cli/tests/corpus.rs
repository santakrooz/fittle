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

// ---- editing ----------------------------------------------------------------

fn fittle_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fittle"))
        .args(args)
        .env("NO_COLOR", "1")
        .current_dir(dir)
        .output()
        .unwrap()
}

#[test]
fn set_dry_run_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        corpus_root().join("seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit"),
        dir.path().join("sub.fit"),
    )
    .unwrap();
    let before = std::fs::read(dir.path().join("sub.fit")).unwrap();
    let out = fittle_in(
        dir.path(),
        &["set", "sub.fit", "FOCALLEN=250", "APTDIA=50.0", "--dry-run"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    insta::assert_snapshot!("set_dry_run", String::from_utf8(out.stdout).unwrap());
    assert_eq!(std::fs::read(dir.path().join("sub.fit")).unwrap(), before);
    assert!(!dir.path().join("sub.fit.bak").exists());
}

#[test]
fn set_json_and_batch_validation() {
    let dir = tempfile::tempdir().unwrap();
    for n in ["a.fit", "b.fit"] {
        std::fs::copy(
            corpus_root().join("seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit"),
            dir.path().join(n),
        )
        .unwrap();
    }
    let out = fittle_in(
        dir.path(),
        &[
            "set",
            "a.fit",
            "b.fit",
            "OBJECT=M 31",
            "--json",
            "--no-backup",
        ],
    );
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["schema"], "fittle.edit/1");
    assert_eq!(json["files"].as_array().unwrap().len(), 2);
    assert_eq!(json["files"][0]["method"], "in_place");

    // One locked key fails the whole batch before anything is written.
    let before = std::fs::read(dir.path().join("a.fit")).unwrap();
    let out = fittle_in(
        dir.path(),
        &["set", "a.fit", "b.fit", "OBJECT=M 33", "BITPIX=8"],
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(std::fs::read(dir.path().join("a.fit")).unwrap(), before);
}

#[test]
fn template_and_scrub() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        corpus_root().join("seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit"),
        dir.path().join("sub.fit"),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("rig.json"),
        r#"{"FOCALLEN": 250.0, "APTDIA": 50.0, "TELESCOP": "Seestar S50"}"#,
    )
    .unwrap();
    assert!(
        fittle_in(
            dir.path(),
            &["set", "sub.fit", "--from", "rig.json", "--no-backup"]
        )
        .status
        .success()
    );
    let out = fittle_in(
        dir.path(),
        &["scrub", "sub.fit", "--privacy", "--no-backup"],
    );
    assert!(out.status.success());
    let info = fittle_in(dir.path(), &["info", "--json", "sub.fit"]);
    let json: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(json["fields"]["focal_mm"]["value"], 250.0);
    assert!(json["fields"]["site_lat"].is_null());
    assert_eq!(
        fittle_in(dir.path(), &["unset", "sub.fit", "NAXIS"])
            .status
            .code(),
        Some(2)
    );
}

// ---- export -----------------------------------------------------------------

#[test]
fn export_commands() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        corpus_root().join("seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit"),
        dir.path().join("sub.fit"),
    )
    .unwrap();
    let before = std::fs::read(dir.path().join("sub.fit")).unwrap();
    let json = |out: &Output| -> serde_json::Value {
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    };

    let v = json(&fittle_in(
        dir.path(),
        &[
            "export",
            "sub.fit",
            "-o",
            "out",
            "-t",
            "{object}_{filter}",
            "--json",
        ],
    ));
    assert_eq!(v["schema"], "fittle.export/1");
    let row = &v["files"][0];
    assert!(row["path"].as_str().unwrap().ends_with("NGC 6995_LP.png"));
    assert_eq!(
        (row["width"].as_u64(), row["channels"].as_u64()),
        (Some(64), Some(3))
    );

    // Format from the output extension; linear FITS for geometry commands.
    let v = json(&fittle_in(
        dir.path(),
        &[
            "export",
            "sub.fit",
            "-o",
            "small.jpg",
            "--long-edge",
            "32",
            "--json",
        ],
    ));
    assert_eq!(v["files"][0]["format"]["kind"], "jpeg");
    assert_eq!(v["files"][0]["width"], 32);
    let v = json(&fittle_in(
        dir.path(),
        &["crop", "sub.fit", "--rect", "0,0,32,16", "--json"],
    ));
    assert!(
        v["files"][0]["path"]
            .as_str()
            .unwrap()
            .ends_with("sub_crop.fits")
    );
    let v = json(&fittle_in(
        dir.path(),
        &["crop", "sub.fit", "--rect", "0,0,32,16", "--json"],
    ));
    assert!(
        v["files"][0]["path"]
            .as_str()
            .unwrap()
            .ends_with("sub_crop_2.fits")
    );

    // Validation errors exit 2 and write nothing.
    let out = fittle_in(
        dir.path(),
        &["export", "sub.fit", "--rotate", "45", "-o", "bad"],
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        std::fs::read_dir(dir.path().join("bad")).unwrap().count(),
        0
    );
    assert_eq!(
        fittle_in(dir.path(), &["export", "sub.fit", "-f", "bmp"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        fittle_in(dir.path(), &["export", "sub.fit", "-o", "sub.fit"])
            .status
            .code(),
        Some(2)
    );

    assert_eq!(std::fs::read(dir.path().join("sub.fit")).unwrap(), before);
    assert!(
        fittle_in(dir.path(), &["export", "--tokens"])
            .status
            .success()
    );
}
