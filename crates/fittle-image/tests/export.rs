//! Export pipeline: every format writes, the source is never touched, plate
//! solutions follow the pixels, and outputs match golden images.
//! Refresh goldens with `FITTLE_UPDATE_GOLDEN=1 cargo test -p fittle-image --test export`.

use std::path::{Path, PathBuf};

use fittle_core::Fits;
use fittle_image::export::{Crop, ExportSpec, Format, Stretch, export};

fn corpus(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/synthetic")
        .join(rel)
}

const SEESTAR: &str = "seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit";
const PIXINSIGHT: &str =
    "pixinsight/masterLight_BIN-1_6248x4176_EXPOSURE-300.00s_FILTER-NoFilter_RGB.fits";
const SIRIL: &str = "siril/r_pp_NGC6995_stacked.fit";

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fittle-export-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn hash(p: &Path) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::fs::read(p).unwrap().hash(&mut h);
    h.finish()
}

fn png_pixels(p: &Path) -> (u32, u32, png::ColorType, Vec<u8>) {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).unwrap()));
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    (info.width, info.height, info.color_type, buf)
}

#[test]
fn every_format_writes_and_source_is_untouched() {
    let src = corpus(SEESTAR);
    let before = hash(&src);
    let dir = tmp("formats");
    for f in [
        "png", "png16", "jpeg", "webp", "tiff8", "tiff16", "tiff32", "fits",
    ] {
        let spec = ExportSpec {
            format: Format::parse(f).unwrap(),
            ..Default::default()
        };
        let out = export(&src, &dir, Some("{object}_{filter}"), &spec).unwrap();
        assert!(out.bytes > 0, "{f}");
        // CFA debayered at full size.
        assert_eq!((out.width, out.height, out.channels), (64, 48, 3), "{f}");
    }
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"NGC 6995_LP.png".to_string()));
    // png and png16 share a name: the second gets a suffix, nothing is replaced.
    assert!(names.contains(&"NGC 6995_LP_2.png".to_string()));
    assert!(!names.iter().any(|n| n.contains("fittle-part")));
    assert_eq!(hash(&src), before);

    let (w, h, ct, _) = png_pixels(&dir.join("NGC 6995_LP.png"));
    assert_eq!((w, h, ct), (64, 48, png::ColorType::Rgb));
}

#[test]
fn never_overwrites() {
    let dir = tmp("overwrite");
    let copy = dir.join("src.fit");
    std::fs::copy(corpus(SIRIL), &copy).unwrap();
    let before = hash(&copy);
    let spec = ExportSpec {
        format: Format::Fits,
        stretch: Stretch::None,
        ..Default::default()
    };
    // Explicit output equal to the source.
    assert!(export(&copy, &copy, None, &spec).is_err());
    // Template naming that collides with the source gets a suffix.
    let out = export(
        &copy,
        &dir,
        Some("src"),
        &ExportSpec {
            format: Format::Fits,
            ..spec.clone()
        },
    )
    .unwrap();
    assert!(out.path.ends_with("src.fits"));
    let again = export(&copy, &dir, Some("src"), &spec).unwrap();
    assert!(again.path.ends_with("src_2.fits"));
    // Explicit existing output.
    assert!(export(&copy, Path::new(&out.path), None, &spec).is_err());
    assert_eq!(hash(&copy), before);
}

#[test]
fn linear_fits_keeps_physical_values() {
    let src = corpus(SIRIL);
    let dir = tmp("linear");
    let spec = ExportSpec {
        format: Format::Fits,
        stretch: Stretch::None,
        ..Default::default()
    };
    let out = export(&src, &dir, None, &spec).unwrap();
    let read = |p: &Path| {
        let f = Fits::open(p).unwrap();
        let i = fittle_core::info_from(&f, "");
        fittle_image::decode_hdu(p, &f.hdus[i.image.unwrap().hdu]).unwrap()
    };
    let (a, b) = (read(&src), read(Path::new(&out.path)));
    assert_eq!((a.width, a.height), (b.width, b.height));
    for (x, y) in a.data.iter().zip(&b.data) {
        assert!((x - y).abs() <= 1e-5 * x.abs().max(1.0), "{x} vs {y}");
    }
    let f = Fits::open(Path::new(&out.path)).unwrap();
    let h = f.hdus[0].header();
    assert!(h.history().any(|l| l.contains("physical (linear)")));
    assert_eq!(
        h.string("OBJECT"),
        Fits::open(&src).unwrap().hdus[0].header().string("OBJECT")
    );
}

#[test]
fn plate_solution_follows_crop_rotate_flip_bin() {
    let src = corpus(PIXINSIGHT);
    let dir = tmp("wcs");
    let src_fits = Fits::open(&src).unwrap();
    let src_wcs = fittle_core::derive::wcs_tan(src_fits.hdus[0].header()).unwrap();
    let crop = Crop {
        x: 10,
        y: 6,
        width: 40,
        height: 30,
    };
    let spec = ExportSpec {
        format: Format::Fits,
        stretch: Stretch::None,
        crop: Some(crop),
        rotate: 90,
        flip_horizontal: true,
        ..Default::default()
    };
    let out = export(&src, &dir, None, &spec).unwrap();
    assert_eq!((out.width, out.height), (30, 40));
    let f = Fits::open(Path::new(&out.path)).unwrap();
    let wcs = fittle_core::derive::wcs_tan(f.hdus[0].header()).unwrap();
    // Output (nx, ny) ← flipped (w−1−nx, ny) ← rotated: crop (x = ny, y = h−1−(w−1−nx))
    for (nx, ny) in [(0usize, 0usize), (29, 39), (7, 21)] {
        let (cx, cy) = (ny, crop.height - 1 - (29 - nx));
        let a = src_wcs.pixel_to_sky((cx + crop.x) as f64, (cy + crop.y) as f64);
        let b = wcs.pixel_to_sky(nx as f64, ny as f64);
        assert!(
            (a.ra - b.ra).abs() < 1e-9 && (a.dec - b.dec).abs() < 1e-9,
            "({nx},{ny})"
        );
    }
    // Pixels moved the same way.
    let a = fittle_image::decode_hdu(&src, &src_fits.hdus[0]).unwrap();
    let b = fittle_image::decode_hdu(Path::new(&out.path), &f.hdus[0]).unwrap();
    let (cx, cy) = (21, crop.height - 1 - (29 - 7));
    assert_eq!(b.get(7, 21, 1), a.get(cx + crop.x, cy + crop.y, 1));

    // Bin 2: sky at output pixel centres matches the centre of the 2×2 block.
    let spec = ExportSpec {
        format: Format::Fits,
        stretch: Stretch::None,
        bin: Some(2),
        ..Default::default()
    };
    let out = export(&src, &dir, Some("binned"), &spec).unwrap();
    let f = Fits::open(Path::new(&out.path)).unwrap();
    let wcs = fittle_core::derive::wcs_tan(f.hdus[0].header()).unwrap();
    let a = src_wcs.pixel_to_sky(2.0 * 5.0 + 0.5, 2.0 * 7.0 + 0.5);
    let b = wcs.pixel_to_sky(5.0, 7.0);
    assert!((a.ra - b.ra).abs() < 1e-9 && (a.dec - b.dec).abs() < 1e-9);
}

#[test]
fn raw_cfa_keeps_bayer_phase() {
    let src = corpus(SEESTAR);
    let dir = tmp("cfa");
    let spec = ExportSpec {
        format: Format::Fits,
        stretch: Stretch::None,
        debayer: false,
        crop: Some(Crop {
            x: 3,
            y: 5,
            width: 21,
            height: 15,
        }),
        ..Default::default()
    };
    let out = export(&src, &dir, None, &spec).unwrap();
    // Snapped to even origin and size.
    assert_eq!((out.width, out.height, out.channels), (20, 14, 1));
    let f = Fits::open(Path::new(&out.path)).unwrap();
    assert_eq!(
        f.hdus[0].header().string("BAYERPAT").map(str::trim),
        Some("GRBG")
    );
    let rotate = ExportSpec {
        rotate: 90,
        crop: None,
        ..spec
    };
    assert!(matches!(
        export(&src, &dir, None, &rotate),
        Err(fittle_image::export::ExportError::Invalid(_))
    ));
}

#[test]
fn privacy_scrubs_fits_and_xmp() {
    let src = corpus(SEESTAR);
    let dir = tmp("privacy");
    let spec = ExportSpec {
        format: Format::Fits,
        ..Default::default()
    };
    let out = export(&src, &dir, None, &spec).unwrap();
    let f = Fits::open(Path::new(&out.path)).unwrap();
    let h = f.hdus[0].header();
    assert!(h.get("SITELAT").is_none() && h.get("SITELONG").is_none());
    assert!(
        h.get("BAYERPAT").is_none(),
        "debayered output keeps no Bayer keys"
    );
    assert!(h.history().any(|l| l.contains("display-stretched")));

    let png = export(&src, &dir, None, &ExportSpec::default()).unwrap();
    let bytes = std::fs::read(&png.path).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("<fittle:Object>NGC 6995</fittle:Object>"));
    assert!(!text.contains("SiteLatitude"));
    let open = export(
        &src,
        &dir,
        None,
        &ExportSpec {
            private: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&std::fs::read(&open.path).unwrap()).contains("SiteLatitude"));
    let bare = export(
        &src,
        &dir,
        None,
        &ExportSpec {
            metadata: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!String::from_utf8_lossy(&std::fs::read(&bare.path).unwrap()).contains("xmpmeta"));
}

/// Visual regression: 8-bit PNGs compared with goldens (±1 per sample).
#[test]
fn golden_images() {
    let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/golden/export");
    let update = std::env::var_os("FITTLE_UPDATE_GOLDEN").is_some();
    let dir = tmp("golden");
    let cases: Vec<(&str, &str, ExportSpec)> = vec![
        (
            "seestar-auto",
            SEESTAR,
            ExportSpec {
                format: Format::Png { bits: 8 },
                ..Default::default()
            },
        ),
        (
            "seestar-linked-rot-half",
            SEESTAR,
            ExportSpec {
                format: Format::Png { bits: 8 },
                stretch: Stretch::Auto { linked: true },
                rotate: 270,
                long_edge: Some(32),
                ..Default::default()
            },
        ),
        (
            "pixinsight-asinh",
            PIXINSIGHT,
            ExportSpec {
                format: Format::Png { bits: 8 },
                stretch: Stretch::Asinh,
                ..Default::default()
            },
        ),
        (
            "siril-linear",
            SIRIL,
            ExportSpec {
                format: Format::Png { bits: 8 },
                stretch: Stretch::None,
                ..Default::default()
            },
        ),
    ];
    for (name, src, spec) in cases {
        let out = export(&corpus(src), &dir, Some(name), &spec).unwrap();
        let want = golden.join(format!("{name}.png"));
        if update || !want.exists() {
            std::fs::create_dir_all(&golden).unwrap();
            std::fs::copy(&out.path, &want).unwrap();
            if !update {
                panic!("golden {name}.png was missing and has been written; review and commit it");
            }
            continue;
        }
        let (a, b) = (png_pixels(Path::new(&out.path)), png_pixels(&want));
        assert_eq!(
            (a.0, a.1, a.2),
            (b.0, b.1, b.2),
            "{name}: size or colour type"
        );
        let worst =
            a.3.iter()
                .zip(&b.3)
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap_or(0);
        assert!(worst <= 1, "{name}: differs from golden by {worst}");
    }
}

#[test]
fn plan_matches_export_and_preview_runs() {
    let dir = tmp("plan");
    let specs = [
        ExportSpec::default(),
        ExportSpec {
            crop: Some(Crop {
                x: 5,
                y: 3,
                width: 33,
                height: 21,
            }),
            rotate: 90,
            ..Default::default()
        },
        ExportSpec {
            bin: Some(3),
            long_edge: Some(10),
            format: Format::Fits,
            ..Default::default()
        },
        ExportSpec {
            debayer: false,
            crop: Some(Crop {
                x: 1,
                y: 1,
                width: 31,
                height: 17,
            }),
            format: Format::Fits,
            stretch: Stretch::None,
            ..Default::default()
        },
    ];
    for src in [SEESTAR, PIXINSIGHT] {
        let path = corpus(src);
        let session = fittle_image::ViewSession::open(&path).unwrap();
        for (i, spec) in specs.iter().enumerate() {
            let plan = fittle_image::export::plan(&session.info, spec, "{object}");
            let out = export(&path, &dir, Some(&format!("{i}")), spec).unwrap();
            assert_eq!(
                (plan.width, plan.height, plan.channels),
                (out.width, out.height, out.channels),
                "{src} #{i}"
            );
            assert!(plan.problem.is_none());
            let o = session.image.as_ref().unwrap();
            let p = fittle_image::export::preview(o, &session.info, spec, 24).unwrap();
            assert!(p.image.width.max(p.image.height) <= 24, "{src} #{i}");
            assert_eq!(p.image.planes, out.channels);
        }
    }
    let bad = ExportSpec {
        rotate: 45,
        ..Default::default()
    };
    let info = fittle_core::info(corpus(SEESTAR)).unwrap();
    assert!(
        fittle_image::export::plan(&info, &bad, "x")
            .problem
            .is_some()
    );
}

#[test]
fn share_card_and_avif() {
    let dir = tmp("card");
    let src = corpus(SEESTAR);
    let spec = ExportSpec {
        card: true,
        format: Format::Png { bits: 8 },
        ..Default::default()
    };
    let out = export(&src, &dir, Some("card"), &spec).unwrap();
    let (w, h, _) = fittle_image::card::size(64, 48);
    assert_eq!((out.width, out.height, out.channels), (w, h, 3));
    let info = fittle_core::info(&src).unwrap();
    let plan = fittle_image::export::plan(&info, &spec, "x");
    assert_eq!((plan.width, plan.height, plan.channels), (w, h, 3));
    // Caption comes from info; private hides the site.
    let cap = fittle_image::card::caption(&info, "auto STF", true);
    assert_eq!(cap.title, "NGC 6995");
    assert!(
        cap.rig.starts_with("ZWO Seestar S50") && cap.rig.contains("20 s"),
        "{}",
        cap.rig
    );
    assert!(!cap.details.contains('°'));
    assert!(
        fittle_image::card::caption(&info, "auto STF", false)
            .details
            .contains("33.7°N")
    );
    // Not for FITS.
    let fits = ExportSpec {
        card: true,
        format: Format::Fits,
        ..Default::default()
    };
    assert!(export(&src, &dir, None, &fits).is_err());
    assert!(
        fittle_image::export::plan(&info, &fits, "x")
            .problem
            .is_some()
    );

    let avif = export(
        &src,
        &dir,
        Some("small"),
        &ExportSpec {
            format: Format::parse("avif").unwrap(),
            ..Default::default()
        },
    )
    .unwrap();
    let bytes = std::fs::read(&avif.path).unwrap();
    assert_eq!(&bytes[4..12], b"ftypavif");
    // EXIF summary, no site.
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("NGC 6995") && text.contains("Fittle") && !text.contains("33.69"));
}

/// Visual regression for the share card; text edges may differ slightly
/// across CPUs, so allow a few pixels off by more than 2.
#[test]
fn golden_share_card() {
    let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/golden/export/seestar-card.png");
    let dir = tmp("card-golden");
    let spec = ExportSpec {
        card: true,
        format: Format::Png { bits: 8 },
        ..Default::default()
    };
    let out = export(&corpus(SEESTAR), &dir, Some("card"), &spec).unwrap();
    if std::env::var_os("FITTLE_UPDATE_GOLDEN").is_some() || !golden.exists() {
        std::fs::copy(&out.path, &golden).unwrap();
        assert!(
            std::env::var_os("FITTLE_UPDATE_GOLDEN").is_some(),
            "golden written; review and commit it"
        );
        return;
    }
    let (a, b) = (png_pixels(Path::new(&out.path)), png_pixels(&golden));
    assert_eq!((a.0, a.1), (b.0, b.1));
    let off =
        a.3.iter()
            .zip(&b.3)
            .filter(|(x, y)| x.abs_diff(**y) > 2)
            .count();
    assert!(off * 1000 < a.3.len(), "{off} samples differ");
}
