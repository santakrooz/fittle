//! Per-channel statistics and histograms of normalized [0, 1] data.
//!
//! One parallel pass builds a 65,536-bin histogram per channel; median and
//! MAD come from it (exact to 1/65,536, i.e. exact for 16-bit data), so
//! nothing is sorted and a 60 MP frame costs one read of memory.

use rayon::prelude::*;
use serde::Serialize;

use crate::decode::Image;

const FINE: usize = 1 << 16;
/// Bins in the display histogram sent to the UI.
pub const DISPLAY_BINS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChannelStats {
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    pub median: f32,
    /// Median absolute deviation (not scaled to sigma).
    pub mad: f32,
    /// Fraction of pixels at 0 (clipped low).
    pub clipped_low: f32,
    /// Fraction of pixels at or above 0.9999 (saturated).
    pub saturated: f32,
    /// 99.99th percentile: the bright end of the data (brighter stars),
    /// ignoring the few saturated cores; the white point of the viewer's Linear.
    pub p9999: f32,
    /// `DISPLAY_BINS` counts over [0, 1].
    pub histogram: Vec<u32>,
}

/// Statistics for each plane of a normalized image.
pub fn channel_stats(img: &Image) -> Vec<ChannelStats> {
    let plane = img.width * img.height;
    (0..img.planes)
        .map(|p| stats_of(&img.data[p * plane..(p + 1) * plane]))
        .collect()
}

fn bin(v: f32) -> usize {
    ((v.clamp(0.0, 1.0) * (FINE - 1) as f32).round() as usize).min(FINE - 1)
}

struct Acc {
    hist: Vec<u32>,
    min: f32,
    max: f32,
    sum: f64,
}

fn stats_of(data: &[f32]) -> ChannelStats {
    let acc = data
        .par_chunks(1 << 16)
        .fold(
            || Acc {
                hist: vec![0; FINE],
                min: f32::MAX,
                max: f32::MIN,
                sum: 0.0,
            },
            |mut a, chunk| {
                let mut s = 0.0f64;
                for &v in chunk {
                    a.hist[bin(v)] += 1;
                    a.min = a.min.min(v);
                    a.max = a.max.max(v);
                    s += v as f64;
                }
                a.sum += s;
                a
            },
        )
        .reduce(
            || Acc {
                hist: vec![0; FINE],
                min: f32::MAX,
                max: f32::MIN,
                sum: 0.0,
            },
            |mut a, b| {
                a.hist.iter_mut().zip(&b.hist).for_each(|(x, y)| *x += y);
                a.min = a.min.min(b.min);
                a.max = a.max.max(b.max);
                a.sum += b.sum;
                a
            },
        );
    let n = data.len().max(1) as f64;
    let median_bin = quantile_bin(&acc.hist, 0.5);
    let median = median_bin as f32 / (FINE - 1) as f32;

    // MAD from the same histogram: distances |bin - median| folded around it.
    let mut dev = vec![0u32; FINE];
    for (b, &c) in acc.hist.iter().enumerate() {
        dev[b.abs_diff(median_bin)] += c;
    }
    let mad = quantile_bin(&dev, 0.5) as f32 / (FINE - 1) as f32;

    let sat_from = bin(0.9999);
    let saturated = acc.hist[sat_from..].iter().map(|&c| c as f64).sum::<f64>() / n;
    let mut histogram = vec![0u32; DISPLAY_BINS];
    for (b, &c) in acc.hist.iter().enumerate() {
        histogram[b * DISPLAY_BINS / FINE] += c;
    }
    ChannelStats {
        min: if data.is_empty() { 0.0 } else { acc.min },
        max: if data.is_empty() { 0.0 } else { acc.max },
        mean: (acc.sum / n) as f32,
        median,
        mad,
        clipped_low: (acc.hist[0] as f64 / n) as f32,
        p9999: quantile_bin(&acc.hist, 0.9999) as f32 / (FINE - 1) as f32,
        saturated: saturated as f32,
        histogram,
    }
}

fn quantile_bin(hist: &[u32], q: f64) -> usize {
    let total: u64 = hist.iter().map(|&c| c as u64).sum();
    let target = (total as f64 * q).ceil().max(1.0) as u64;
    let mut run = 0u64;
    for (i, &c) in hist.iter().enumerate() {
        run += c as u64;
        if run >= target {
            return i;
        }
    }
    hist.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_and_mad() {
        // 0.1, 0.2, …, 0.9 → median 0.5, MAD 0.2
        let data: Vec<f32> = (1..=9).map(|i| i as f32 / 10.0).collect();
        let img = Image {
            width: 9,
            height: 1,
            planes: 1,
            data,
        };
        let s = &channel_stats(&img)[0];
        assert!((s.median - 0.5).abs() < 1e-4, "{s:?}");
        assert!((s.mad - 0.2).abs() < 1e-4, "{s:?}");
        assert!((s.mean - 0.5).abs() < 1e-6);
        assert_eq!((s.min, s.max), (0.1, 0.9));
        assert_eq!(s.histogram.iter().sum::<u32>(), 9);
    }

    #[test]
    fn clipping_fractions() {
        let img = Image {
            width: 4,
            height: 1,
            planes: 1,
            data: vec![0.0, 0.5, 1.0, 1.0],
        };
        let s = &channel_stats(&img)[0];
        assert_eq!(s.clipped_low, 0.25);
        assert_eq!(s.saturated, 0.5);
    }
}
