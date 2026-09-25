//! Display stretch. Nothing here changes file data: it produces normalized
//! preview pixels and screen-transfer parameters that the viewer applies.

use crate::decode::Image;

/// Auto-STF defaults (PixInsight / Siril convention).
pub const SHADOWS_CLIP: f32 = -2.8;
pub const TARGET_BACKGROUND: f32 = 0.25;
/// Scales MAD to a Gaussian-consistent sigma.
const MAD_TO_SIGMA: f32 = 1.4826;
/// Cap on samples used for median/MAD; enough for a stable estimate.
const MAX_SAMPLES: usize = 1 << 20;

/// Screen-transfer parameters for one channel, in normalized [0, 1] units.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stf {
    pub shadows: f32,
    pub midtones: f32,
    pub highlights: f32,
}

impl Stf {
    pub const IDENTITY: Stf = Stf {
        shadows: 0.0,
        midtones: 0.5,
        highlights: 1.0,
    };

    /// Apply to one normalized value (same formula as the viewer's shader).
    pub fn apply(&self, v: f32) -> f32 {
        let x = ((v - self.shadows) / (self.highlights - self.shadows)).clamp(0.0, 1.0);
        mtf(self.midtones, x)
    }
}

/// Midtones transfer function.
pub fn mtf(m: f32, x: f32) -> f32 {
    if x <= 0.0 {
        0.0
    } else if x >= 1.0 {
        1.0
    } else if m == 0.5 {
        x
    } else {
        ((m - 1.0) * x) / ((2.0 * m - 1.0) * x - m)
    }
}

/// Map physical values to [0, 1] using the pixel type's natural range:
/// integers by their full range, floats as-is when already within [0, 1],
/// otherwise by their min/max. Returns the (low, high) mapping used.
pub fn normalize(img: &mut Image, bitpix: i64, bzero: f64) -> (f32, f32) {
    let (lo, hi) = match (bitpix, bzero) {
        (8, _) => (0.0, 255.0),
        (16, 32768.0) => (0.0, 65535.0),
        (16, _) => (-32768.0, 32767.0),
        (32, 2147483648.0) => (0.0, 4294967295.0),
        (32, _) => (-2147483648.0, 2147483647.0),
        _ => {
            let (mn, mx) = finite_min_max(&img.data);
            if mn >= 0.0 && mx <= 1.0 {
                (0.0, 1.0)
            } else {
                (mn, mx)
            }
        }
    };
    let span = if hi > lo { hi - lo } else { 1.0 };
    for v in &mut img.data {
        *v = if v.is_finite() {
            ((*v - lo) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    (lo, hi)
}

fn finite_min_max(data: &[f32]) -> (f32, f32) {
    data.iter()
        .filter(|v| v.is_finite())
        .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)))
}

/// Unlinked auto-STF: one set of parameters per plane of a normalized image.
pub fn auto_stf(img: &Image) -> Vec<Stf> {
    let plane = img.width * img.height;
    (0..img.planes)
        .map(|p| auto_stf_plane(&img.data[p * plane..(p + 1) * plane]))
        .collect()
}

fn auto_stf_plane(data: &[f32]) -> Stf {
    let step = data.len().div_ceil(MAX_SAMPLES).max(1);
    let mut samples: Vec<f32> = data.iter().step_by(step).copied().collect();
    if samples.is_empty() {
        return Stf::IDENTITY;
    }
    let median = select_median(&mut samples);
    for s in &mut samples {
        *s = (*s - median).abs();
    }
    let mad = select_median(&mut samples) * MAD_TO_SIGMA;

    let shadows = if mad > 0.0 {
        (median + SHADOWS_CLIP * mad).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Choose m so the background (median) lands at the target level.
    let midtones = mtf(TARGET_BACKGROUND, (median - shadows) / (1.0 - shadows));
    Stf {
        shadows,
        midtones,
        highlights: 1.0,
    }
}

fn select_median(v: &mut [f32]) -> f32 {
    let mid = v.len() / 2;
    *v.select_nth_unstable_by(mid, f32::total_cmp).1
}

/// Box-downsample so the long edge is at most `max_edge` pixels.
pub fn downsample(img: &Image, max_edge: usize) -> Image {
    let factor = img.width.max(img.height).div_ceil(max_edge).max(1);
    if factor == 1 {
        return img.clone();
    }
    let (w, h) = (img.width / factor, img.height / factor);
    let mut data = Vec::with_capacity(w * h * img.planes);
    let inv = 1.0 / (factor * factor) as f32;
    for p in 0..img.planes {
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0;
                for dy in 0..factor {
                    let row = (p * img.height + y * factor + dy) * img.width + x * factor;
                    sum += img.data[row..row + factor].iter().sum::<f32>();
                }
                data.push(sum * inv);
            }
        }
    }
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

    #[test]
    fn mtf_properties() {
        assert_eq!(mtf(0.5, 0.3), 0.3);
        assert!((mtf(0.2, 0.2) - 0.5).abs() < 1e-6, "m maps to 0.5");
        assert_eq!(mtf(0.1, 0.0), 0.0);
        assert_eq!(mtf(0.1, 1.0), 1.0);
    }

    #[test]
    fn auto_stf_puts_background_at_target() {
        // Linear sub: background near 0.02 with small noise.
        let data: Vec<f32> = (0..10_000)
            .map(|i| 0.02 + ((i * 7919) % 101) as f32 * 1e-5)
            .collect();
        let img = Image {
            width: 100,
            height: 100,
            planes: 1,
            data,
        };
        let stf = auto_stf(&img)[0];
        let median = 0.02 + 50.0 * 1e-5;
        assert!(
            (stf.apply(median) - TARGET_BACKGROUND).abs() < 1e-3,
            "{stf:?}"
        );
    }

    #[test]
    fn normalize_u16_and_float() {
        let mut img = Image {
            width: 2,
            height: 1,
            planes: 1,
            data: vec![0.0, 65535.0],
        };
        normalize(&mut img, 16, 32768.0);
        assert_eq!(img.data, [0.0, 1.0]);
        let mut img = Image {
            width: 2,
            height: 1,
            planes: 1,
            data: vec![10.0, 30.0],
        };
        assert_eq!(normalize(&mut img, -32, 0.0), (10.0, 30.0));
        assert_eq!(img.data, [0.0, 1.0]);
    }

    #[test]
    fn downsample_averages() {
        let img = Image {
            width: 4,
            height: 2,
            planes: 1,
            data: vec![1., 3., 5., 7., 1., 3., 5., 7.],
        };
        let d = downsample(&img, 2);
        assert_eq!((d.width, d.height), (2, 1));
        assert_eq!(d.data, [2.0, 6.0]);
    }
}
