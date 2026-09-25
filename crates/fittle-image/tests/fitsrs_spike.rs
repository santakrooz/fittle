//! M0 spike: cross-check our decoder against `fitsrs`, and confirm `fitsrs`
//! decodes Rice tile-compressed `.fz` losslessly. See docs/decisions/0001.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use fitsrs::{HDU, Pixels};
use fittle_core::Fits;
use fittle_image::decode_hdu;

fn corpus(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/synthetic")
        .join(rel)
}

/// Raw stored values of the first image HDU, via fitsrs.
fn fitsrs_raw(path: &PathBuf) -> Vec<f64> {
    let mut list = fitsrs::Fits::from_reader(BufReader::new(File::open(path).unwrap()));
    while let Some(Ok(hdu)) = list.next() {
        let data = match &hdu {
            HDU::Primary(h) | HDU::XImage(h) => {
                if h.get_header().get_xtension().get_naxis().is_empty() {
                    continue;
                }
                list.get_data(h).pixels()
            }
            HDU::XBinaryTable(h) => match list.get_data(h) {
                fitsrs::hdu::data::bintable::data::BinaryTableData::TileCompressed(p) => {
                    use fitsrs::hdu::data::bintable::tile_compressed::pixels::Pixels as Z;
                    return match p {
                        Z::U8(it) => it.map(|v| v as f64).collect(),
                        Z::I16(it) => it.map(|v| v as f64).collect(),
                        Z::I32(it) => it.map(|v| v as f64).collect(),
                        Z::F32(it) => it.map(|v| v as f64).collect(),
                        Z::F64(it) => it.map(|v| v as f64).collect(),
                    };
                }
                _ => continue,
            },
            _ => continue,
        };
        return match data {
            Pixels::U8(it) => it.map(|v| v as f64).collect(),
            Pixels::I16(it) => it.map(|v| v as f64).collect(),
            Pixels::I32(it) => it.map(|v| v as f64).collect(),
            Pixels::I64(it) => it.map(|v| v as f64).collect(),
            Pixels::F32(it) => it.map(|v| v as f64).collect(),
            Pixels::F64(it) => it.collect(),
        };
    }
    panic!("no image in {}", path.display())
}

#[test]
fn decoder_matches_fitsrs() {
    for rel in [
        "seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit",
        "siril/r_pp_NGC6995_stacked.fit",
        "pixinsight/masterLight_BIN-1_6248x4176_EXPOSURE-300.00s_FILTER-NoFilter_RGB.fits",
        "calibration/master_dark_300s_g100_-10C.fit",
        "edge/multi-hdu.fits",
    ] {
        let path = corpus(rel);
        let fits = Fits::open(&path).unwrap();
        let hdu = fits.hdus.iter().find(|h| !h.shape.is_empty()).unwrap();
        let ours = decode_hdu(&path, hdu).unwrap();
        let zero = hdu.header().float("BZERO").unwrap_or(0.0);
        let scale = hdu.header().float("BSCALE").unwrap_or(1.0);
        let theirs: Vec<f32> = fitsrs_raw(&path)
            .into_iter()
            .map(|v| (zero + scale * v) as f32)
            .collect();
        assert_eq!(ours.data.len(), theirs.len(), "{rel}");
        assert_eq!(ours.data, theirs, "{rel}");
    }
}

/// Our RICE_1 decoder must reproduce each uncompressed twin exactly.
#[test]
fn rice_fz_decodes_losslessly() {
    for (fz, original) in [
        (
            "edge/compressed-rice.fits.fz",
            "edge/compressed-rice-original.fits",
        ),
        ("edge/rice/u8.fits.fz", "edge/rice/u8-original.fits"),
        ("edge/rice/i16.fits.fz", "edge/rice/i16-original.fits"),
        ("edge/rice/i32.fits.fz", "edge/rice/i32-original.fits"),
        ("edge/rice/flat.fits.fz", "edge/rice/flat-original.fits"),
        ("edge/rice/noise.fits.fz", "edge/rice/noise-original.fits"),
    ] {
        let original = corpus(original);
        let fits = Fits::open(&original).unwrap();
        let expected = decode_hdu(&original, &fits.hdus[0]).unwrap();

        let fz_path = corpus(fz);
        let zfits = Fits::open(&fz_path).unwrap();
        let zhdu = &zfits.hdus[1];
        assert_eq!(zhdu.kind, fittle_core::HduKind::CompressedImage, "{fz}");
        let got = decode_hdu(&fz_path, zhdu).unwrap_or_else(|e| panic!("{fz}: {e}"));
        assert_eq!(
            (got.width, got.height, got.planes),
            (expected.width, expected.height, expected.planes),
            "{fz}"
        );
        assert!(got.data == expected.data, "{fz}: pixels differ");
    }
}

/// Records the spike finding behind docs/decisions/0001: fitsrs 0.4.1
/// mis-decodes 16-bit RICE_1 tiles written by astropy/fpack (and overflows in
/// debug builds). Ignored so CI stays green; run with `--ignored` to recheck
/// when upgrading fitsrs.
#[test]
#[ignore = "fitsrs 0.4.1 RICE_1 bug; see docs/decisions/0001-fits-io.md"]
fn fitsrs_decodes_rice_fz_losslessly() {
    let original = corpus("edge/compressed-rice-original.fits");
    let fits = Fits::open(&original).unwrap();
    let expected = decode_hdu(&original, &fits.hdus[0]).unwrap();
    let fz = corpus("edge/compressed-rice.fits.fz");
    let zero = Fits::open(&fz).unwrap().hdus[1]
        .header()
        .float("BZERO")
        .unwrap_or(0.0);
    let got: Vec<f32> = fitsrs_raw(&fz)
        .into_iter()
        .map(|v| (v + zero) as f32)
        .collect();
    assert_eq!(got, expected.data);
}
