//! Small auto-stretched RGBA thumbnails for file lists.
//!
//! Reads only the rows it samples (about `max_edge` of them), so a 26 MP
//! frame costs a few milliseconds, not a full decode. CFA frames sample 2×2
//! blocks and debayer them; tile-compressed files fall back to a full decode.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use fittle_core::{Fits, HduKind};

use crate::debayer::{Cfa, superpixel};
use crate::decode::Image;
use crate::session::ViewSession;
use crate::stats::channel_stats;
use crate::stretch::{Stf, downsample, normalize};
use crate::view::Mode;

/// RGBA8 thumbnail with long edge ≤ `max_edge`: (width, height, pixels).
pub fn thumbnail(path: impl AsRef<Path>, max_edge: usize) -> Option<(usize, usize, Vec<u8>)> {
    let path = path.as_ref();
    let fits = Fits::open(path).ok()?;
    let info = fittle_core::info_from(&fits, &path.to_string_lossy());
    let hdu = &fits.hdus[info.image.as_ref()?.hdu];
    if hdu.kind == HduKind::CompressedImage {
        return full(path, max_edge);
    }
    let (w, h) = (
        hdu.shape[0] as usize,
        hdu.shape.get(1).copied().unwrap_or(1) as usize,
    );
    let planes = if hdu.shape.len() >= 3 && hdu.shape[2] == 3 {
        3
    } else {
        1
    };
    let bitpix = hdu.bitpix?;
    let bpp = (bitpix.unsigned_abs() / 8) as usize;
    let bottom_up = info
        .fields
        .row_order
        .as_ref()
        .is_some_and(|r| r.value.eq_ignore_ascii_case("BOTTOM-UP"));
    let head = hdu.header();
    let cfa = info
        .fields
        .bayer
        .as_ref()
        .filter(|_| planes == 1)
        .and_then(|b| {
            Cfa::new(
                &b.value,
                head.int("XBAYROFF").unwrap_or(0),
                head.int("YBAYROFF").unwrap_or(0),
                bottom_up,
                h,
            )
        });

    let mut step = w.max(h).div_ceil(max_edge).max(1);
    if cfa.is_some() {
        step = step.max(2).next_multiple_of(2);
    }
    // One stored row per sample row (two for a CFA block).
    let block = if cfa.is_some() { 2 } else { 1 };
    let (ow, oh) = ((w / step).max(1), (h / step).max(1));
    let zero = head.float("BZERO").unwrap_or(0.0);
    let scale = head.float("BSCALE").unwrap_or(1.0);
    let mut f = File::open(path).ok()?;
    let mut row = vec![0u8; w * bpp];
    let sw = ow * block;
    let sh = oh * block;
    // Average a band of up to 8 rows (all columns) per sample row: stars
    // survive and noise drops, at a fraction of a full read.
    let band = step.min(8).max(block) / block * block;
    let mut data = vec![0f32; sw * sh * planes];
    let mut sum = vec![0f64; sw * block];
    let mut cnt = vec![0u32; sw * block];
    for p in 0..planes {
        for oy in 0..oh {
            sum.iter_mut().for_each(|v| *v = 0.0);
            cnt.iter_mut().for_each(|v| *v = 0);
            for j in 0..band {
                let y = (oy * step + j).min(h - 1);
                f.seek(SeekFrom::Start(
                    hdu.data_offset + ((p * h + y) * w * bpp) as u64,
                ))
                .ok()?;
                f.read_exact(&mut row).ok()?;
                let dy = j % block;
                for x in 0..(ow * step).min(w) {
                    let (ox, dx) = (x / step, x % block);
                    let v = sample(&row[x * bpp..(x + 1) * bpp], bitpix)?;
                    let k = dy * sw + ox * block + dx;
                    sum[k] += zero + scale * v;
                    cnt[k] += 1;
                }
            }
            for dy in 0..block {
                for c in 0..sw {
                    let k = dy * sw + c;
                    data[(p * sh + oy * block + dy) * sw + c] = if cnt[k] > 0 {
                        (sum[k] / cnt[k] as f64) as f32
                    } else {
                        0.0
                    };
                }
            }
        }
    }
    let mut small = Image {
        width: sw,
        height: sh,
        planes,
        data,
    };
    normalize(&mut small, bitpix, zero);
    if let Some(c) = cfa {
        small = superpixel(&small, c);
    }
    Some(rgba(&small))
}

fn sample(b: &[u8], bitpix: i64) -> Option<f64> {
    Some(match bitpix {
        8 => b[0] as f64,
        16 => i16::from_be_bytes(b.try_into().ok()?) as f64,
        32 => i32::from_be_bytes(b.try_into().ok()?) as f64,
        64 => i64::from_be_bytes(b.try_into().ok()?) as f64,
        -32 => f32::from_be_bytes(b.try_into().ok()?) as f64,
        -64 => f64::from_be_bytes(b.try_into().ok()?),
        _ => return None,
    })
}

/// Full decode path (tile-compressed files).
fn full(path: &Path, max_edge: usize) -> Option<(usize, usize, Vec<u8>)> {
    let s = ViewSession::open(path).ok()?;
    let o = s.image.as_ref()?;
    let mode = if o.cfa.is_some() {
        Mode::Debayer
    } else {
        Mode::Raw
    };
    Some(rgba(&downsample(o.shown_image(mode), max_edge)))
}

/// Auto-stretch a small normalized image into RGBA8.
fn rgba(img: &Image) -> (usize, usize, Vec<u8>) {
    let stats = channel_stats(img);
    let stf: Vec<Stf> = stats
        .iter()
        .map(|s| crate::view::stf_from(s.median, s.mad))
        .collect();
    let plane = img.width * img.height;
    let mut out = Vec::with_capacity(plane * 4);
    for i in 0..plane {
        let c = |p: usize| {
            let p = p.min(img.planes - 1);
            (stf[p].apply(img.data[p * plane + i]) * 255.0).round() as u8
        };
        out.extend([c(0), c(1), c(2), 255]);
    }
    (img.width, img.height, out)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn corpus_thumbnails() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic");
        for rel in [
            "seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit",
            "pixinsight/masterLight_BIN-1_6248x4176_EXPOSURE-300.00s_FILTER-NoFilter_RGB.fits",
            "edge/compressed-rice.fits.fz",
        ] {
            let (w, h, px) =
                super::thumbnail(root.join(rel), 32).unwrap_or_else(|| panic!("{rel}"));
            assert!(w <= 32 && h <= 32 && w > 0 && h > 0, "{rel}: {w}×{h}");
            assert_eq!(px.len(), w * h * 4);
        }
    }
}
