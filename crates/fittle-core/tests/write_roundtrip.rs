//! CLAUDE.md rule 1: header edits never change data-unit bytes. Every write
//! path is exercised on copies of the corpus and each HDU's data bytes are
//! compared with the original's.

use std::fs;
use std::path::{Path, PathBuf};

use fittle_core::edit::{EditError, NewValue, Op, Options};
use fittle_core::write::{Method, apply, verify_checksum};
use fittle_core::{Fits, Value};
use proptest::prelude::*;

fn corpus(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/synthetic")
        .join(rel)
}

fn copy(rel: &str, dir: &Path) -> PathBuf {
    let dst = dir.join(Path::new(rel).file_name().unwrap());
    fs::copy(corpus(rel), &dst).unwrap();
    dst
}

/// Data bytes of every HDU, in order.
fn data_units(path: &Path) -> Vec<Vec<u8>> {
    let bytes = fs::read(path).unwrap();
    Fits::open(path)
        .unwrap()
        .hdus
        .iter()
        .map(|h| bytes[h.data_offset as usize..(h.data_offset + h.data_bytes) as usize].to_vec())
        .collect()
}

fn set(key: &str, v: NewValue) -> Op {
    Op::Set {
        key: key.into(),
        value: v,
        comment: None,
    }
}

const FILES: [&str; 6] = [
    "seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit",
    "siril/r_pp_NGC6995_stacked.fit",
    "edge/multi-hdu.fits",
    "edge/compressed-rice.fits.fz",
    "edge/long-strings-hierarch.fits",
    "edge/checksum.fits",
];

#[test]
fn astropy_checksum_verifies() {
    assert_eq!(
        verify_checksum(corpus("edge/checksum.fits"), 0).unwrap(),
        Some(true)
    );
    assert_eq!(
        verify_checksum(corpus("siril/r_pp_NGC6995_stacked.fit"), 0).unwrap(),
        None
    );
}

#[test]
fn in_place_and_rewrite_keep_data_bytes() {
    let dir = tempfile::tempdir().unwrap();
    for rel in FILES {
        let p = copy(rel, dir.path());
        let before = data_units(&p);

        // Small edit: same block count → in place.
        let r = apply(
            &p,
            &[set("OBJECT", NewValue::String("Test target".into()))],
            &Options::default(),
        )
        .unwrap();
        assert_eq!(r.method, Method::InPlace, "{rel}");
        assert_eq!(data_units(&p), before, "{rel}: in-place write changed data");
        assert!(
            Path::new(&format!("{}.bak", p.display())).exists(),
            "{rel}: no backup"
        );

        // Many new cards: header grows → rewrite via temp file.
        let ops: Vec<Op> = (0..60)
            .map(|i| set(&format!("TEST{i:04}"), NewValue::Integer(i)))
            .collect();
        let r = apply(
            &p,
            &ops,
            &Options {
                backup: false,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(r.method, Method::Rewrite, "{rel}");
        assert_eq!(data_units(&p), before, "{rel}: rewrite changed data");

        // And shrink back: fewer blocks → rewrite again.
        let ops: Vec<Op> = (0..60)
            .map(|i| Op::Unset {
                key: format!("TEST{i:04}"),
            })
            .collect();
        let r = apply(
            &p,
            &ops,
            &Options {
                backup: false,
                history: false,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(r.method, Method::Rewrite, "{rel}");
        assert_eq!(data_units(&p), before, "{rel}: shrink changed data");

        let fits = Fits::open(&p).unwrap();
        assert!(
            fits.issues
                .iter()
                .all(|i| i.severity != fittle_core::Severity::Error),
            "{rel}: {:?}",
            fits.issues
        );
        let h = fits.hdus[fittle_core::edit::default_hdu(&fits)].header();
        assert_eq!(h.string("OBJECT"), Some("Test target"), "{rel}");
        assert!(h.get("TEST0001").is_none());
        // No temp files left behind.
        assert!(
            fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().ends_with(".tmp"))
        );
    }
}

#[test]
fn checksum_is_resealed() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy("edge/checksum.fits", dir.path());
    let r = apply(
        &p,
        &[set("OBJECT", NewValue::String("M 57 edited".into()))],
        &Options::default(),
    )
    .unwrap();
    assert!(r.checksum.is_some());
    assert_eq!(verify_checksum(&p, 0).unwrap(), Some(true));
    // Opting out leaves a stale checksum, which verification reports.
    apply(
        &p,
        &[set("OBJECT", NewValue::String("M 57 again".into()))],
        &Options {
            checksum: false,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(verify_checksum(&p, 0).unwrap(), Some(false));
}

#[test]
fn edits_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy(
        "seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit",
        dir.path(),
    );
    let long = "A long note that needs the CONTINUE convention because it runs well past sixty-eight characters.";
    apply(
        &p,
        &[
            set("FOCALLEN", NewValue::Auto("250".into())),
            set("APTDIA", NewValue::Float(50.0)),
            set("NOTES", NewValue::String(long.into())),
            Op::Unset {
                key: "SITELAT".into(),
            },
            Op::Rename {
                from: "FILTER".into(),
                to: "FILTNAME".into(),
            },
        ],
        &Options::default(),
    )
    .unwrap();
    let fits = Fits::open(&p).unwrap();
    let h = fits.hdus[0].header();
    assert_eq!(h.value("FOCALLEN"), Some(&Value::Integer(250)));
    assert_eq!(h.float("APTDIA"), Some(50.0));
    assert_eq!(h.string("NOTES"), Some(long));
    assert!(h.get("SITELAT").is_none());
    assert_eq!(h.string("FILTNAME"), Some("LP"));
    assert!(
        h.history()
            .any(|l| l.starts_with("Fittle ") && l.contains("FOCALLEN"))
    );
}

#[test]
fn refusals_write_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy(
        "seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit",
        dir.path(),
    );
    let original = fs::read(&p).unwrap();
    for ops in [
        vec![set("NAXIS1", NewValue::Integer(10))],
        vec![set("BZERO", NewValue::Integer(0))],
        vec![Op::Unset {
            key: "BITPIX".into(),
        }],
        vec![set("TOOLONGKEY", NewValue::Integer(1))],
        vec![Op::Rename {
            from: "OBJECT".into(),
            to: "EXPTIME".into(),
        }],
    ] {
        let r = apply(&p, &ops, &Options::default());
        assert!(matches!(r, Err(EditError::Invalid(_))), "{ops:?} → {r:?}");
    }
    assert_eq!(
        fs::read(&p).unwrap(),
        original,
        "a refused edit changed the file"
    );
    assert!(
        !Path::new(&format!("{}.bak", p.display())).exists(),
        "a refused edit made a backup"
    );

    // Structurally broken files are refused outright.
    let t = copy("malformed/truncated.fit", dir.path());
    assert!(matches!(
        apply(
            &t,
            &[set("OBJECT", NewValue::String("x".into()))],
            &Options::default()
        ),
        Err(EditError::Invalid(_))
    ));
}

// ---- property test: random edit sequences --------------------------------

fn op_strategy() -> impl Strategy<Value = Op> {
    let key = prop::sample::select(vec![
        "OBJECT", "FILTER", "GAIN", "NOTES", "OBSERVER", "FOCALLEN", "TESTA", "TESTB",
    ]);
    let value = prop_oneof![
        any::<bool>().prop_map(NewValue::Logical),
        any::<i32>().prop_map(|i| NewValue::Integer(i as i64)),
        (-1e6f64..1e6).prop_map(NewValue::Float),
        "[ -~]{0,120}".prop_map(NewValue::String),
    ];
    prop_oneof![
        4 => (key.clone(), value, proptest::option::of("[ -~]{0,40}")).prop_map(|(k, v, c)| Op::Set { key: k.into(), value: v, comment: c }),
        2 => key.clone().prop_map(|k| Op::Unset { key: k.into() }),
        1 => (key.clone(), key).prop_map(|(a, b)| Op::Rename { from: a.into(), to: b.into() }),
        1 => "[ -~]{1,150}".prop_map(|t| Op::History { text: t }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    #[test]
    fn random_edits_never_touch_data(ops in prop::collection::vec(op_strategy(), 1..12), file in 0usize..FILES.len()) {
        let dir = tempfile::tempdir().unwrap();
        let p = copy(FILES[file], dir.path());
        let before = data_units(&p);
        match apply(&p, &ops, &Options { backup: false, ..Options::default() }) {
            Ok(_) | Err(EditError::Invalid(_)) => {}
            Err(e) => prop_assert!(false, "unexpected error: {e}"),
        }
        prop_assert_eq!(data_units(&p), before);
        let fits = Fits::open(&p).unwrap();
        prop_assert!(fits.issues.iter().all(|i| i.severity != fittle_core::Severity::Error), "{:?}", fits.issues);
        if FILES[file] == "edge/checksum.fits" {
            prop_assert_ne!(verify_checksum(&p, 0).unwrap(), Some(false));
        }
    }
}
