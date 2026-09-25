//! Geometric operations on planar images: crop, bin, flip, rotate, resize.
//! Each has a matching `fittle_astro::wcs::Tan` transform so exports keep a
//! valid plate solution. Row 0 is the first stored row.

use rayon::prelude::*;

use crate::decode::Image;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BinMode {
    Average,
    Sum,
}

/// Crop to `[x, x+w) × [y, y+h)`, clamped to the image.
pub fn crop(img: &Image, x: usize, y: usize, w: usize, h: usize) -> Image {
    let x = x.min(img.width);
    let y = y.min(img.height);
    let (w, h) = (w.min(img.width - x), h.min(img.height - y));
    let mut data = Vec::with_capacity(w * h * img.planes);
    for p in 0..img.planes {
        for r in y..y + h {
            let at = (p * img.height + r) * img.width + x;
            data.extend_from_slice(&img.data[at..at + w]);
        }
    }
    Image {
        width: w,
        height: h,
        planes: img.planes,
        data,
    }
}

/// Software binning by `n` (partial edge blocks are dropped).
pub fn bin(img: &Image, n: usize, mode: BinMode) -> Image {
    let n = n.max(1);
    let (w, h) = (img.width / n, img.height / n);
    let scale = if mode == BinMode::Average {
        1.0 / (n * n) as f32
    } else {
        1.0
    };
    let mut data = vec![0f32; w * h * img.planes];
    data.par_chunks_mut(w).enumerate().for_each(|(row, out)| {
        let (p, y) = (row / h.max(1), row % h.max(1));
        for (x, o) in out.iter_mut().enumerate() {
            let mut s = 0.0;
            for dy in 0..n {
                let at = (p * img.height + y * n + dy) * img.width + x * n;
                s += img.data[at..at + n].iter().sum::<f32>();
            }
            *o = s * scale;
        }
    });
    Image {
        width: w,
        height: h,
        planes: img.planes,
        data,
    }
}

/// Mirror columns (`horizontal`) or rows.
pub fn flip(img: &Image, horizontal: bool) -> Image {
    let (w, h) = (img.width, img.height);
    let mut data = vec![0f32; img.data.len()];
    data.par_chunks_mut(w).enumerate().for_each(|(row, out)| {
        let (p, y) = (row / h, row % h);
        let src_y = if horizontal { y } else { h - 1 - y };
        let src = &img.data[(p * h + src_y) * w..(p * h + src_y + 1) * w];
        if horizontal {
            out.iter_mut()
                .zip(src.iter().rev())
                .for_each(|(o, v)| *o = *v);
        } else {
            out.copy_from_slice(src);
        }
    });
    Image {
        width: w,
        height: h,
        planes: img.planes,
        data,
    }
}

/// Rotate 90° clockwise (row 0 at the top): pixel (x, y) moves to (h − 1 − y, x).
pub fn rotate_cw(img: &Image) -> Image {
    let (w, h) = (img.width, img.height);
    let mut data = vec![0f32; img.data.len()];
    // New image is h wide, w tall.
    data.par_chunks_mut(h).enumerate().for_each(|(row, out)| {
        let (p, ny) = (row / w, row % w);
        for (nx, o) in out.iter_mut().enumerate() {
            // new (nx, ny) ← old (x = ny, y = h − 1 − nx)
            *o = img.data[(p * h + (h - 1 - nx)) * w + ny];
        }
    });
    Image {
        width: h,
        height: w,
        planes: img.planes,
        data,
    }
}

fn lanczos3(x: f32) -> f32 {
    if x == 0.0 {
        return 1.0;
    }
    if x.abs() >= 3.0 {
        return 0.0;
    }
    let px = std::f32::consts::PI * x;
    3.0 * px.sin() * (px / 3.0).sin() / (px * px)
}

/// Per-output-sample (first source index, weights) for resampling `src` → `dst`.
fn kernel(src: usize, dst: usize) -> Vec<(usize, Vec<f32>)> {
    let ratio = src as f32 / dst as f32;
    // Widen the kernel when shrinking so it averages (anti-aliasing).
    let support = ratio.max(1.0);
    (0..dst)
        .map(|i| {
            let centre = (i as f32 + 0.5) * ratio - 0.5;
            let lo = (centre - 3.0 * support).floor().max(0.0) as usize;
            let hi = ((centre + 3.0 * support).ceil() as usize).min(src - 1);
            let mut w: Vec<f32> = (lo..=hi)
                .map(|j| lanczos3((j as f32 - centre) / support))
                .collect();
            let sum: f32 = w.iter().sum();
            if sum != 0.0 {
                w.iter_mut().for_each(|v| *v /= sum);
            }
            (lo, w)
        })
        .collect()
}

/// Lanczos3 resample to `w` × `h` (separable; anti-aliased when shrinking).
pub fn resize(img: &Image, w: usize, h: usize) -> Image {
    let (w, h) = (w.max(1), h.max(1));
    let kx = kernel(img.width, w);
    let ky = kernel(img.height, h);
    // Horizontal pass: planes × img.height rows of w.
    let mut tmp = vec![0f32; w * img.height * img.planes];
    tmp.par_chunks_mut(w).enumerate().for_each(|(row, out)| {
        let src = &img.data[row * img.width..(row + 1) * img.width];
        for (o, (lo, wt)) in out.iter_mut().zip(&kx) {
            *o = wt.iter().enumerate().map(|(k, c)| c * src[lo + k]).sum();
        }
    });
    // Vertical pass.
    let mut data = vec![0f32; w * h * img.planes];
    data.par_chunks_mut(w).enumerate().for_each(|(row, out)| {
        let (p, y) = (row / h, row % h);
        let (lo, wt) = &ky[y];
        for (k, c) in wt.iter().enumerate() {
            let src = &tmp[(p * img.height + lo + k) * w..(p * img.height + lo + k + 1) * w];
            out.iter_mut().zip(src).for_each(|(o, v)| *o += c * v);
        }
    });
    Image {
        width: w,
        height: h,
        planes: img.planes,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(w: usize, h: usize, planes: usize) -> Image {
        Image {
            width: w,
            height: h,
            planes,
            data: (0..w * h * planes).map(|i| i as f32).collect(),
        }
    }

    #[test]
    fn crop_and_bin() {
        let img = ramp(4, 4, 1);
        assert_eq!(crop(&img, 1, 1, 2, 2).data, [5.0, 6.0, 9.0, 10.0]);
        assert_eq!(bin(&img, 2, BinMode::Sum).data, [10.0, 18.0, 42.0, 50.0]);
        assert_eq!(bin(&img, 2, BinMode::Average).data, [2.5, 4.5, 10.5, 12.5]);
    }

    #[test]
    fn flips_and_rotations_are_involutions() {
        let img = ramp(5, 3, 3);
        assert_eq!(flip(&flip(&img, true), true), img);
        assert_eq!(flip(&flip(&img, false), false), img);
        let r4 = rotate_cw(&rotate_cw(&rotate_cw(&rotate_cw(&img))));
        assert_eq!(r4, img);
        let r = rotate_cw(&img);
        assert_eq!((r.width, r.height), (3, 5));
        // Top-left of the rotated image is the old bottom-left.
        assert_eq!(r.data[0], img.data[2 * 5]);
    }

    #[test]
    fn resize_keeps_flat_fields_and_mean() {
        let flat = Image {
            width: 40,
            height: 30,
            planes: 1,
            data: vec![0.25; 1200],
        };
        let s = resize(&flat, 17, 11);
        assert!(s.data.iter().all(|v| (v - 0.25).abs() < 1e-5));
        let img = ramp(64, 48, 1);
        let half = resize(&img, 32, 24);
        let mean = |d: &[f32]| d.iter().sum::<f32>() / d.len() as f32;
        assert!((mean(&half.data) - mean(&img.data)).abs() / mean(&img.data) < 0.01);
    }
}
