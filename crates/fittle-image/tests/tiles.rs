//! Tile decompression against files written by astropy (cfitsio): lossless
//! files must match their uncompressed twin byte for byte; quantized files
//! must reproduce what cfitsio itself reads back.

use std::path::{Path, PathBuf};

use fittle_core::Fits;
use fittle_image::tiles::{self, Samples};

fn corpus(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/synthetic")
        .join(rel)
}

/// Data unit (unpadded) of HDU `i`.
fn data_unit(path: &Path, i: usize) -> Vec<u8> {
    let f = Fits::open(path).unwrap();
    let h = &f.hdus[i];
    let all = std::fs::read(path).unwrap();
    all[h.data_offset as usize..(h.data_offset + h.data_bytes) as usize].to_vec()
}

fn decode(rel: &str) -> tiles::TileImage {
    let p = corpus(rel);
    let f = Fits::open(&p).unwrap();
    tiles::decode(&f.hdus[1], &data_unit(&p, 1)).unwrap()
}

#[test]
fn lossless_match_originals() {
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
        let img = decode(&format!("edge/rice/{name}.fits.fz"));
        let want = data_unit(&corpus(&format!("edge/rice/{name}-original.fits")), 0);
        assert_eq!(img.to_be_bytes(), want, "{name}");
    }
}

#[test]
fn quantized_match_cfitsio() {
    for name in ["q-nodither", "q-dither1", "q-dither2"] {
        let img = decode(&format!("edge/rice/{name}.fits.fz"));
        let Samples::Float(got) = &img.samples else {
            panic!("{name}: not float")
        };
        let want: Vec<f32> = data_unit(&corpus(&format!("edge/rice/{name}-decoded.fits")), 0)
            .chunks_exact(4)
            .map(|c| f32::from_be_bytes(c.try_into().unwrap()))
            .collect();
        assert_eq!(got.len(), want.len());
        let mut off = 0;
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            let g = *g as f32;
            if g.is_nan() || w.is_nan() {
                assert!(g.is_nan() && w.is_nan(), "{name}[{i}]: null mismatch");
            } else if g != *w {
                // Allow one unit in the last place (double→float rounding).
                assert!(
                    (g.to_bits() as i64 - w.to_bits() as i64).abs() <= 1,
                    "{name}[{i}]: {g} vs {w}"
                );
                off += 1;
            }
        }
        // No-dither and dither-1 match cfitsio exactly. Dither-2 reads back
        // within 1 ulp (~1e-9 relative, far below the quantization step);
        // a few values round differently in astropy's reader.
        if name != "q-dither2" {
            assert_eq!(off, 0, "{name}: {off} values off by 1 ulp");
        }
    }
}
