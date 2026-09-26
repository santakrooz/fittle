//! Star detection and per-frame quality metrics for grading subs: star
//! count, median HFR / FWHM / eccentricity, background and noise, and a
//! satellite-trail flag. Measurement only; pixels are never changed.
//!
//! Works on one luminance plane, normalized 0–1. Colour (CFA) frames are
//! measured on a superpixel luminance, and large frames on a box-downsampled
//! copy; results are reported in source pixels.

use serde::Serialize;

use crate::debayer::{Cfa, superpixel};
use crate::decode::Image;
use crate::stretch::downsample;

/// One measured star, in working-image pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Star {
    pub x: f32,
    pub y: f32,
    /// Background-subtracted flux (normalized units × pixels).
    pub flux: f32,
    pub peak: f32,
    /// Half-flux radius: flux-weighted mean distance from the centroid.
    pub hfr: f32,
    pub fwhm: f32,
    /// 0 = round, → 1 = elongated (from second moments); NaN when the
    /// star is too small to measure its shape.
    pub eccentricity: f32,
}

/// A straight bright streak (satellite, plane, meteor).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Trail {
    /// Angle of the line's normal, degrees, 0–180.
    pub angle_deg: f32,
    /// Length of the supporting pixels, source pixels.
    pub length_px: f32,
}

/// Quality metrics for one frame. Sizes are in source pixels.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameStats {
    pub stars: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hfr: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fwhm: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eccentricity: Option<f32>,
    /// Median background, normalized 0–1.
    pub background: f32,
    /// Background noise (sigma), normalized.
    pub noise: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trail: Option<Trail>,
    /// Source pixels per measured pixel (2 for superpixel colour frames).
    pub scale: f32,
}

/// Detection threshold in background sigmas.
const K_SIGMA: f32 = 5.0;
const TILE: usize = 64;
/// Measurement window radius, working pixels.
const R: i32 = 8;
const MAX_STARS: usize = 2000;
/// Saturated peaks are counted but not measured.
const SATURATED: f32 = 0.98;

/// The plane stars are measured on, and its scale to source pixels.
pub fn luminance(img: &Image, cfa: Option<Cfa>, max_edge: usize) -> (Image, f32) {
    let (mut lum, mut scale) = match (cfa, img.planes) {
        (Some(c), 1) => (mean_planes(&superpixel(img, c)), 2.0),
        (_, 1) => (img.clone(), 1.0),
        _ => (mean_planes(img), 1.0),
    };
    let long = lum.width.max(lum.height);
    if long > max_edge {
        let f = long.div_ceil(max_edge);
        lum = downsample(&lum, max_edge);
        scale *= f as f32;
    }
    (lum, scale)
}

fn mean_planes(img: &Image) -> Image {
    let n = img.width * img.height;
    let data = (0..n)
        .map(|i| (0..img.planes).map(|p| img.data[p * n + i]).sum::<f32>() / img.planes as f32)
        .collect();
    Image {
        width: img.width,
        height: img.height,
        planes: 1,
        data,
    }
}

fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mid = v.len() / 2;
    *v.select_nth_unstable_by(mid, f32::total_cmp).1
}

/// Per-tile background median and sigma (MAD-based), with the global values.
struct Background {
    tiles_x: usize,
    bg: Vec<f32>,
    sigma: Vec<f32>,
    global_bg: f32,
    global_sigma: f32,
}

impl Background {
    fn of(img: &Image) -> Background {
        let (w, h) = (img.width, img.height);
        let tiles_x = w.div_ceil(TILE);
        let tiles_y = h.div_ceil(TILE);
        let mut bg = Vec::with_capacity(tiles_x * tiles_y);
        let mut sigma = Vec::with_capacity(tiles_x * tiles_y);
        let mut buf = Vec::with_capacity(TILE * TILE);
        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                buf.clear();
                for y in (ty * TILE..((ty + 1) * TILE).min(h)).step_by(2) {
                    let row = &img.data[y * w..(y + 1) * w];
                    buf.extend(row[tx * TILE..((tx + 1) * TILE).min(w)].iter().step_by(2));
                }
                let m = median(&mut buf);
                for v in buf.iter_mut() {
                    *v = (*v - m).abs();
                }
                bg.push(m);
                sigma.push(median(&mut buf) * 1.4826);
            }
        }
        let global_bg = median(&mut bg.clone());
        let global_sigma = median(&mut sigma.clone());
        Background {
            tiles_x,
            bg,
            sigma,
            global_bg,
            global_sigma,
        }
    }

    fn at(&self, x: usize, y: usize) -> (f32, f32) {
        let i = (y / TILE) * self.tiles_x + x / TILE;
        // A tile full of nebula has a large sigma; never go below the global noise.
        (
            self.bg[i],
            self.sigma[i].max(self.global_sigma * 0.5).max(1e-6),
        )
    }
}

/// Detect and measure stars, and look for a trail.
pub fn measure(img: &Image, scale: f32) -> FrameStats {
    let (w, h) = (img.width, img.height);
    let bg = Background::of(img);
    let px = |x: i32, y: i32| img.data[y as usize * w + x as usize];

    // Local maxima above threshold, brightest first.
    let mut peaks: Vec<(f32, i32, i32)> = Vec::new();
    for y in 1..h.saturating_sub(1) as i32 {
        for x in 1..w.saturating_sub(1) as i32 {
            let v = px(x, y);
            let (b, s) = bg.at(x as usize, y as usize);
            if v <= b + K_SIGMA * s {
                continue;
            }
            let is_max = (-1..=1).all(|dy| {
                (-1..=1).all(|dx| {
                    let n = px(x + dx, y + dy);
                    (dx, dy) == (0, 0) || n < v || (n == v && (dy > 0 || (dy == 0 && dx > 0)))
                })
            });
            if is_max {
                peaks.push((v, x, y));
            }
        }
    }
    peaks.sort_by(|a, b| b.0.total_cmp(&a.0));

    // Measure, suppressing fainter peaks inside a brighter star's window.
    let cell = R as usize;
    let gw = w.div_ceil(cell);
    let mut taken = vec![false; gw * h.div_ceil(cell)];
    let mut stars = Vec::new();
    let mut saturated = 0;
    for &(peak, x, y) in &peaks {
        if stars.len() + saturated >= MAX_STARS {
            break;
        }
        let g = (y as usize / cell) * gw + x as usize / cell;
        if taken[g] {
            continue;
        }
        let (b, s) = bg.at(x as usize, y as usize);
        // Hot pixels and noise: a star has several neighbours above threshold.
        let lit = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| px(x + dx, y + dy) > b + 2.0 * s)
            .count();
        if lit < 4 {
            continue;
        }
        if x < R || y < R || x >= w as i32 - R || y >= h as i32 - R {
            continue;
        }
        mark(&mut taken, gw, cell, x, y, h);
        if peak >= SATURATED {
            saturated += 1;
            continue;
        }
        if let Some(star) = moments(img, x, y, b, s) {
            stars.push(star);
        }
    }

    let found = find_trail(img, &bg, &stars);
    if let Some((_, line)) = found {
        // Detections along the streak are pieces of it, not stars.
        stars.retain(|st| !line.near(st.x, st.y, 4.0));
    }
    let trail = found.map(|(t, _)| Trail {
        length_px: t.length_px * scale,
        ..t
    });
    FrameStats {
        stars: stars.len() + saturated,
        hfr: med(&stars, |s| s.hfr).map(|v| v * scale),
        fwhm: med(&stars, |s| s.fwhm).map(|v| v * scale),
        eccentricity: med(&stars, |s| s.eccentricity),
        background: bg.global_bg,
        noise: bg.global_sigma,
        trail,
        scale,
    }
}

fn med(stars: &[Star], f: fn(&Star) -> f32) -> Option<f32> {
    let mut v: Vec<f32> = stars.iter().map(f).filter(|v| v.is_finite()).collect();
    (!v.is_empty()).then(|| median(&mut v))
}

/// A line x·cos + y·sin = rho, in working pixels.
#[derive(Clone, Copy)]
struct Line {
    c: f32,
    s: f32,
    rho: f32,
}

impl Line {
    fn near(&self, x: f32, y: f32, d: f32) -> bool {
        (x * self.c + y * self.s - self.rho).abs() <= d
    }
}

fn mark(taken: &mut [bool], gw: usize, cell: usize, x: i32, y: i32, h: usize) {
    let gh = h.div_ceil(cell);
    let (cx, cy) = (x as usize / cell, y as usize / cell);
    for gy in cy.saturating_sub(1)..=(cy + 1).min(gh - 1) {
        for gx in cx.saturating_sub(1)..=(cx + 1).min(gw - 1) {
            taken[gy * gw + gx] = true;
        }
    }
}

/// Centroid, HFR, FWHM and eccentricity inside radius R.
fn moments(img: &Image, x: i32, y: i32, _b: f32, s: f32) -> Option<Star> {
    let w = img.width;
    // Local background and noise from a ring around the star, so nebulosity
    // and gradients under it don't inflate the wings.
    let mut ring: Vec<f32> = Vec::with_capacity(64);
    for dy in -R..=R {
        for dx in -R..=R {
            let d2 = dx * dx + dy * dy;
            if ((R - 2) * (R - 2)..=R * R).contains(&d2) {
                ring.push(img.data[(y + dy) as usize * w + (x + dx) as usize]);
            }
        }
    }
    let b = median(&mut ring);
    for v in ring.iter_mut() {
        *v = (*v - b).abs();
    }
    let s = (median(&mut ring) * 1.4826).max(s * 0.5);
    let floor = 3.0 * s;
    let mut sum = 0.0f64;
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    let mut pts = Vec::with_capacity(((2 * R + 1) * (2 * R + 1)) as usize);
    for dy in -R..=R {
        for dx in -R..=R {
            if dx * dx + dy * dy > R * R {
                continue;
            }
            let v = img.data[(y + dy) as usize * w + (x + dx) as usize] - b;
            // Ignore the noise floor so faint wings don't inflate HFR.
            if v <= floor {
                continue;
            }
            let v = v as f64;
            pts.push((dx as f64, dy as f64, v));
            sum += v;
            sx += v * dx as f64;
            sy += v * dy as f64;
        }
    }
    if sum <= 0.0 || pts.len() < 4 {
        return None;
    }
    let (cx, cy) = (sx / sum, sy / sum);
    let (mut hfr, mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0, 0.0);
    for &(px, py, v) in &pts {
        let (ddx, ddy) = (px - cx, py - cy);
        hfr += v * (ddx * ddx + ddy * ddy).sqrt();
        xx += v * ddx * ddx;
        yy += v * ddy * ddy;
        xy += v * ddx * ddy;
    }
    let (hfr, xx, yy, xy) = (hfr / sum, xx / sum, yy / sum, xy / sum);
    if hfr < 0.5 {
        return None;
    }
    // Eigenvalues of the second-moment matrix.
    let tr = xx + yy;
    let det = (xx * yy - xy * xy).max(0.0);
    let disc = ((tr * tr / 4.0) - det).max(0.0).sqrt();
    let (l1, l2) = (tr / 2.0 + disc, (tr / 2.0 - disc).max(1e-9));
    Some(Star {
        x: (x as f64 + cx) as f32,
        y: (y as f64 + cy) as f32,
        flux: sum as f32,
        peak: img.data[y as usize * w + x as usize],
        hfr: hfr as f32,
        fwhm: (2.3548 * (tr / 2.0).sqrt()) as f32,
        // Shape needs enough pixels; tiny undersampled stars read as elongated.
        eccentricity: if pts.len() >= 12 {
            (1.0 - l2 / l1).max(0.0).sqrt() as f32
        } else {
            f32::NAN
        },
    })
}

/// Hough search for one long straight streak among bright, non-star pixels.
fn find_trail(img: &Image, bg: &Background, stars: &[Star]) -> Option<(Trail, Line)> {
    let (w, h) = (img.width, img.height);
    // Bright pixels outside star cores, on a grid of at most ~512 px.
    let step = w.max(h).div_ceil(512).max(1);
    let (gw, gh) = (w.div_ceil(step), h.div_ceil(step));
    let mut mask = vec![false; gw * gh];
    // Tiles at a data edge (mosaic borders, stack margins) have a sharp
    // step inside them; leave them and their neighbours out.
    let tiles_y = h.div_ceil(TILE);
    let edge = |tx: usize, ty: usize| -> bool {
        let i = ty * bg.tiles_x + tx;
        bg.bg[i] < 0.5 * bg.global_bg || bg.sigma[i] > 4.0 * bg.global_sigma
    };
    let near_edge: Vec<bool> = (0..tiles_y * bg.tiles_x)
        .map(|i| {
            let (tx, ty) = (i % bg.tiles_x, i / bg.tiles_x);
            (ty.saturating_sub(1)..=(ty + 1).min(tiles_y - 1))
                .any(|y| (tx.saturating_sub(1)..=(tx + 1).min(bg.tiles_x - 1)).any(|x| edge(x, y)))
        })
        .collect();
    // Frame borders carry stacking and dither margins; a real trail crosses
    // the interior anyway.
    let margin = TILE.min(w / 8).min(h / 8);
    for y in margin..h - margin {
        for x in margin..w - margin {
            if near_edge[(y / TILE) * bg.tiles_x + x / TILE] {
                continue;
            }
            let (b, s) = bg.at(x, y);
            if img.data[y * w + x] > b + 3.0 * s {
                mask[(y / step) * gw + x / step] = true;
            }
        }
    }
    let clear = |mask: &mut Vec<bool>, cx: f32, cy: f32, r: f32| {
        let (x0, x1) = (
            ((cx - r) / step as f32).max(0.0) as usize,
            (((cx + r) / step as f32) as usize).min(gw - 1),
        );
        let (y0, y1) = (
            ((cy - r) / step as f32).max(0.0) as usize,
            (((cy + r) / step as f32) as usize).min(gh - 1),
        );
        for gy in y0..=y1 {
            for gx in x0..=x1 {
                mask[gy * gw + gx] = false;
            }
        }
    };
    // Round stars are cleared; streak pieces (very elongated) stay.
    for s in stars
        .iter()
        .filter(|s| s.eccentricity.is_nan() || s.eccentricity < 0.9)
    {
        clear(&mut mask, s.x, s.y, (3.0 * s.hfr).max(3.0));
    }
    let pts: Vec<(f32, f32)> = (0..gh)
        .flat_map(|y| (0..gw).map(move |x| (x, y)))
        .filter(|&(x, y)| mask[y * gw + x])
        .map(|(x, y)| (x as f32, y as f32))
        .collect();
    // Clouds or nebulosity light up everything; that's not a trail.
    if pts.len() < 20 || pts.len() > gw * gh / 5 {
        return None;
    }
    const ANGLES: usize = 180;
    let diag = ((gw * gw + gh * gh) as f32).sqrt();
    let nr = (2.0 * diag) as usize + 1;
    let trig: Vec<(f32, f32)> = (0..ANGLES)
        .map(|a| (a as f32).to_radians())
        .map(|t| (t.cos(), t.sin()))
        .collect();
    let mut acc = vec![0u16; ANGLES * nr];
    for &(x, y) in &pts {
        for (a, &(c, s)) in trig.iter().enumerate() {
            let r = (x * c + y * s + diag) as usize;
            acc[a * nr + r] = acc[a * nr + r].saturating_add(1);
        }
    }
    let (best, votes) = acc.iter().enumerate().max_by_key(|(_, v)| **v)?;
    let votes = *votes as f32;
    if votes < 25.0f32.max(0.2 * gw.min(gh) as f32) {
        return None;
    }
    // Extent of the points on that line: a trail crosses much of the frame.
    let (a, r) = (best / nr, best % nr);
    let (c, s) = trig[a];
    let rho = r as f32 - diag;
    let along: Vec<f32> = pts
        .iter()
        .filter(|&&(x, y)| (x * c + y * s - rho).abs() <= 1.0)
        .map(|&(x, y)| -x * s + y * c)
        .collect();
    let lo = along.iter().copied().fold(f32::MAX, f32::min);
    let hi = along.iter().copied().fold(f32::MIN, f32::max);
    let length = hi - lo;
    // Contiguous enough: most of the span is covered.
    if length < 0.25 * gw.min(gh) as f32 || (along.len() as f32) < 0.5 * length {
        return None;
    }
    // Thin: a streak is dense on its line and sparse beside it; a crowded
    // star field or a galaxy is dense everywhere.
    let beside = pts
        .iter()
        .filter(|&&(x, y)| (3.0..=5.0).contains(&(x * c + y * s - rho).abs()))
        .count() as f32;
    // On-line band is 3 px wide, the side bands 2 × 3 px.
    if along.len() as f32 / 3.0 < 4.0 * (beside / 6.0).max(0.5) {
        return None;
    }
    let st = step as f32;
    Some((
        Trail {
            angle_deg: a as f32,
            length_px: length * st,
        },
        Line {
            c,
            s,
            rho: rho * st + (st - 1.0) / 2.0 * (c + s),
        },
    ))
}

/// Working size for grading: long edge at most this many pixels.
pub const GRADE_EDGE: usize = 2048;

/// Measure a file's image HDU: decode, normalize, luminance, stars.
pub fn measure_file(
    path: &std::path::Path,
) -> Result<(fittle_core::Info, FrameStats), crate::DecodeError> {
    let fits = fittle_core::Fits::open(path).map_err(|e| std::io::Error::other(e.to_string()))?;
    let info = fittle_core::info_from(&fits, &path.to_string_lossy());
    let summary = info
        .image
        .as_ref()
        .ok_or_else(|| crate::DecodeError::Unsupported("no image".into()))?;
    let hdu = &fits.hdus[summary.hdu];
    let mut img = crate::decode_hdu(path, hdu)?;
    crate::stretch::normalize(
        &mut img,
        hdu.bitpix.unwrap_or(-32),
        hdu.header().float("BZERO").unwrap_or(0.0),
    );
    if img.planes != 1 && img.planes != 3 {
        img.planes = 1;
        img.data.truncate(img.width * img.height);
    }
    let bottom_up = crate::export::bottom_up(&info);
    let cfa = info
        .fields
        .bayer
        .as_ref()
        .filter(|_| img.planes == 1)
        .and_then(|b| {
            let h = hdu.header();
            Cfa::new(
                &b.value,
                h.int("XBAYROFF").unwrap_or(0),
                h.int("YBAYROFF").unwrap_or(0),
                bottom_up,
                img.height,
            )
        });
    let (lum, scale) = luminance(&img, cfa, GRADE_EDGE);
    Ok((info, measure(&lum, scale)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic noise + Gaussian stars.
    fn field(w: usize, h: usize, stars: &[(f32, f32, f32, f32, f32)], noise: f32) -> Image {
        let mut seed = 7u64;
        let mut rnd = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 40) as f32 / (1u64 << 24) as f32) - 0.5
        };
        let mut data: Vec<f32> = (0..w * h).map(|_| 0.1 + noise * rnd() * 3.46).collect();
        for &(sx, sy, amp, sigx, sigy) in stars {
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = (x as f32 - sx, y as f32 - sy);
                    data[y * w + x] += amp
                        * (-(dx * dx) / (2.0 * sigx * sigx) - (dy * dy) / (2.0 * sigy * sigy))
                            .exp();
                }
            }
        }
        Image {
            width: w,
            height: h,
            planes: 1,
            data,
        }
    }

    fn grid(n: usize, w: usize, h: usize, sig: (f32, f32)) -> Vec<(f32, f32, f32, f32, f32)> {
        let side = (n as f32).sqrt().ceil() as usize;
        (0..n)
            .map(|i| {
                let (gx, gy) = (i % side, i / side);
                let x = 20.0 + gx as f32 * (w as f32 - 40.0) / side as f32 + 0.3;
                let y = 20.0 + gy as f32 * (h as f32 - 40.0) / side as f32 + 0.6;
                (x, y, 0.3 + 0.02 * (i % 7) as f32, sig.0, sig.1)
            })
            .collect()
    }

    #[test]
    fn round_stars_hfr_and_fwhm() {
        let sigma = 1.6;
        let img = field(400, 300, &grid(36, 400, 300, (sigma, sigma)), 0.002);
        let m = measure(&img, 1.0);
        assert_eq!(m.stars, 36, "{m:?}");
        // Gaussian: HFR ≈ 1.2533 σ, FWHM = 2.3548 σ (noise-floor cut trims a little).
        let hfr = m.hfr.unwrap();
        assert!((hfr - 1.2533 * sigma).abs() < 0.25, "hfr {hfr}");
        let fwhm = m.fwhm.unwrap();
        assert!((fwhm - 2.3548 * sigma).abs() < 0.5, "fwhm {fwhm}");
        assert!(m.eccentricity.unwrap() < 0.25);
        assert!(m.trail.is_none());
        assert!((m.background - 0.1).abs() < 0.01);
    }

    #[test]
    fn elongated_stars_are_eccentric_and_blur_raises_hfr() {
        let trailed = measure(
            &field(400, 300, &grid(25, 400, 300, (3.0, 1.5)), 0.002),
            1.0,
        );
        assert!(trailed.eccentricity.unwrap() > 0.7, "{trailed:?}");
        let soft = measure(
            &field(400, 300, &grid(25, 400, 300, (2.6, 2.6)), 0.002),
            1.0,
        );
        let sharp = measure(
            &field(400, 300, &grid(25, 400, 300, (1.3, 1.3)), 0.002),
            1.0,
        );
        assert!(soft.hfr.unwrap() > 1.6 * sharp.hfr.unwrap());
        // Scale reports source pixels.
        let scaled = measure(
            &field(400, 300, &grid(25, 400, 300, (1.3, 1.3)), 0.002),
            2.0,
        );
        assert!((scaled.hfr.unwrap() - 2.0 * sharp.hfr.unwrap()).abs() < 1e-4);
    }

    #[test]
    fn hot_pixels_are_not_stars() {
        let mut img = field(300, 200, &[], 0.002);
        for i in 0..50 {
            img.data[(10 + i * 3) * 300 + 20 + i * 5] = 0.9;
        }
        assert_eq!(measure(&img, 1.0).stars, 0);
    }

    #[test]
    fn finds_a_satellite_trail() {
        let mut img = field(512, 384, &grid(30, 512, 384, (1.5, 1.5)), 0.002);
        // A 2-px-wide streak across the frame.
        for x in 0..512 {
            let y = 40.0 + x as f32 * 0.55;
            for k in 0..2 {
                let yy = y as usize + k;
                if yy < 384 {
                    img.data[yy * 512 + x] += 0.08;
                }
            }
        }
        let m = measure(&img, 1.0);
        let t = m.trail.expect("trail");
        assert!(t.length_px > 300.0, "{t:?}");
        assert!(m.stars >= 25);
    }
}
