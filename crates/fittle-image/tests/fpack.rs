//! fpack / funpack: lossless round trips over the corpus, interop with
//! astropy-written files, checksums, and never overwriting.

use std::path::{Path, PathBuf};

use fittle_core::{Fits, HduKind};
use fittle_image::fpack::{
    Method, PackError, PackOptions, fpack, funpack, packed_name, unpacked_name,
};

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic")
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fittle-fpack-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// (data unit, non-structural header records) of every image HDU.
fn images(path: &Path) -> Vec<(Vec<u8>, Vec<String>)> {
    let bytes = std::fs::read(path).unwrap();
    let f = Fits::from_bytes(&bytes).unwrap();
    f.hdus
        .iter()
        .filter(|h| matches!(h.kind, HduKind::Primary | HduKind::Image) && h.data_bytes > 0)
        .map(|h| {
            let data =
                bytes[h.data_offset as usize..(h.data_offset + h.data_bytes) as usize].to_vec();
            let keep: Vec<String> = h
                .cards
                .cards
                .iter()
                .filter(|c| {
                    let k = c.keyword.as_str();
                    !(matches!(
                        k,
                        "SIMPLE"
                            | "XTENSION"
                            | "BITPIX"
                            | "NAXIS"
                            | "EXTEND"
                            | "PCOUNT"
                            | "GCOUNT"
                            | "CHECKSUM"
                            | "DATASUM"
                    ) || k.starts_with("NAXIS"))
                })
                .flat_map(|c| c.raw.clone())
                .collect();
            (data, keep)
        })
        .collect()
}

#[test]
fn corpus_round_trips_exactly() {
    let dir = tmp("corpus");
    let mut files = Vec::new();
    walk(&corpus_root(), &mut files);
    let mut n = 0;
    for src in files {
        let name = src.to_string_lossy();
        if name.contains("malformed")
            || name.ends_with(".fz")
            || !(name.ends_with(".fit") || name.ends_with(".fits"))
        {
            continue;
        }
        let before = images(&src);
        if before.is_empty() {
            continue;
        }
        let packed = dir.join(format!("{n}.fits.fz"));
        let r =
            fpack(&src, &packed, &PackOptions::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(r.verified);
        let unpacked = dir.join(format!("{n}.fits"));
        funpack(&packed, &unpacked).unwrap_or_else(|e| panic!("{name}: {e}"));
        let after = images(&unpacked);
        assert_eq!(before.len(), after.len(), "{name}: image count");
        for (i, (b, a)) in before.iter().zip(&after).enumerate() {
            assert!(b.0 == a.0, "{name} image {i}: pixels differ");
            assert_eq!(b.1, a.1, "{name} image {i}: header differs");
        }
        n += 1;
    }
    assert!(n >= 15, "only {n} corpus files round-tripped");
}

#[test]
fn every_method_round_trips() {
    let dir = tmp("methods");
    let src = corpus_root().join("edge/rice/i16-original.fits");
    let want = images(&src);
    for (i, m) in [Method::Rice, Method::Gzip1, Method::Gzip2]
        .into_iter()
        .enumerate()
    {
        for rows in [1, 7, 48] {
            let p = dir.join(format!("{i}-{rows}.fz"));
            fpack(
                &src,
                &p,
                &PackOptions {
                    method: Some(m),
                    tile_rows: Some(rows),
                },
            )
            .unwrap();
            let u = dir.join(format!("{i}-{rows}.fits"));
            funpack(&p, &u).unwrap();
            assert_eq!(images(&u), want, "{m:?} rows {rows}");
        }
    }
    // RICE_1 refuses floats.
    let float = corpus_root().join("edge/rice/gzip1-f32-original.fits");
    let err = fpack(
        &float,
        &dir.join("f.fz"),
        &PackOptions {
            method: Some(Method::Rice),
            tile_rows: None,
        },
    );
    assert!(matches!(err, Err(PackError::Invalid(_))));
}

#[test]
fn unpacks_astropy_files() {
    let dir = tmp("astropy");
    for name in [
        "u8",
        "i16",
        "i32",
        "flat",
        "noise",
        "gzip1-f32",
        "gzip2-f32",
        "gzip1-i16",
        "gzip2-i16",
    ] {
        let src = corpus_root().join(format!("edge/rice/{name}.fits.fz"));
        let out = dir.join(format!("{name}.fits"));
        funpack(&src, &out).unwrap();
        let want = images(&corpus_root().join(format!("edge/rice/{name}-original.fits")));
        assert_eq!(images(&out)[0].0, want[0].0, "{name}");
    }
    // Quantized: unpacks to floats.
    funpack(
        &corpus_root().join("edge/rice/q-dither1.fits.fz"),
        &dir.join("q.fits"),
    )
    .unwrap();
    let f = Fits::open(dir.join("q.fits")).unwrap();
    assert_eq!(f.hdus[0].bitpix, Some(-32));
}

#[test]
fn compresses_a_realistic_sub() {
    // 512×512 16-bit sky: background ~1000 ADU with ~20 ADU of noise.
    let dir = tmp("ratio");
    let src = dir.join("sky.fits");
    let mut seed = 99u64;
    let mut noise = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((seed >> 33) % 41) as i16 - 20
    };
    let data: Vec<u8> = (0..512 * 512)
        .flat_map(|_| (1000 + noise()).to_be_bytes())
        .collect();
    let records: Vec<String> = [
        "SIMPLE  =                    T",
        "BITPIX  =                   16",
        "NAXIS   =                    2",
        "NAXIS1  =                  512",
        "NAXIS2  =                  512",
        "OBJECT  = 'M 42    '",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    fittle_image::encode::fits(&src, &records, &data).unwrap();
    let r = fpack(&src, &dir.join("sky.fits.fz"), &PackOptions::default()).unwrap();
    let ratio = r.bytes_out as f64 / r.bytes_in as f64;
    assert!(ratio < 0.5, "ratio {ratio:.2}");
}

#[test]
fn checksums_are_valid_both_ways() {
    let dir = tmp("checksum");
    let src = corpus_root().join("edge/checksum.fits");
    let p = dir.join("c.fits.fz");
    fpack(&src, &p, &PackOptions::default()).unwrap();
    // The table carries the original data sum; funpack reseals the image.
    let f = Fits::open(&p).unwrap();
    assert!(f.hdus[1].header().string("ZDATASUM").is_some());
    assert!(f.hdus[1].header().get("CHECKSUM").is_none());
    let u = dir.join("c.fits");
    funpack(&p, &u).unwrap();
    assert_eq!(
        fittle_core::write::verify_checksum(&u, 0).unwrap(),
        Some(true)
    );
}

#[test]
fn never_overwrites() {
    let dir = tmp("overwrite");
    let src = dir.join("a.fits");
    std::fs::copy(corpus_root().join("edge/rice/u8-original.fits"), &src).unwrap();
    let before = std::fs::read(&src).unwrap();
    assert!(matches!(
        fpack(&src, &src, &PackOptions::default()),
        Err(PackError::Invalid(_))
    ));
    let p = packed_name(&src);
    assert_eq!(p, dir.join("a.fits.fz"));
    fpack(&src, &p, &PackOptions::default()).unwrap();
    assert!(matches!(
        fpack(&src, &p, &PackOptions::default()),
        Err(PackError::Invalid(_))
    ));
    assert!(matches!(funpack(&p, &src), Err(PackError::Invalid(_))));
    assert!(matches!(
        fpack(&p, &dir.join("b.fz"), &PackOptions::default()),
        Err(PackError::Invalid(_))
    ));
    assert_eq!(std::fs::read(&src).unwrap(), before);
    assert_eq!(unpacked_name(&p), src);
    assert_eq!(
        unpacked_name(Path::new("x/img.fz")),
        Path::new("x/img.fits")
    );
    assert!(!std::fs::read_dir(&dir).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .contains("fittle-part")
    }));
}
