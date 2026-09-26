//! An opened image held for viewing: full-resolution normalized pixels,
//! stats and auto-STF per display mode, plus previews, full-resolution
//! regions and per-pixel readout. Nothing here writes to the file.

use std::path::Path;
use std::sync::OnceLock;

use fittle_core::Hdu;
use half::f16;
use rayon::prelude::*;
use serde::Serialize;

use crate::debayer::{Cfa, superpixel};
use crate::decode::{DecodeError, Image, decode_hdu};
use crate::stats::{ChannelStats, channel_stats};
use crate::stretch::{Stf, downsample, normalize};

/// How the pixels are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Stored planes as-is (mono, raw CFA, or RGB).
    Raw,
    /// CFA frames debayered to RGB (superpixel, half size).
    Debayer,
}

/// What a display mode looks like: size, channels, stats, auto-STF.
#[derive(Debug, Clone, Serialize)]
pub struct Display {
    pub mode: Mode,
    pub width: usize,
    pub height: usize,
    pub channels: usize,
    /// Display pixel → source pixel scale (2 for superpixel debayer).
    pub source_step: usize,
    pub stats: Vec<ChannelStats>,
    /// Unlinked auto-STF per channel.
    pub stf: Vec<Stf>,
    /// Auto-STF from the luminance-weighted mean of channels (linked).
    pub stf_linked: Stf,
    /// Viewer "Linear": a straight line from the auto black point to a white
    /// point that puts the background at 15 % grey (at most the 99.99th
    /// percentile), per channel. Relative brightness stays true; the
    /// frame is visible instead of black.
    pub stf_linear: Vec<Stf>,
}

struct Shown {
    img: Image,
    display: Display,
}

pub struct Opened {
    pub hdu: usize,
    pub width: usize,
    pub height: usize,
    pub planes: usize,
    /// Physical values mapped to 0 and 1.
    pub normalized_from: (f32, f32),
    /// Present when the frame is single-plane with a usable Bayer pattern.
    pub cfa: Option<Cfa>,
    raw: Shown,
    debayered: OnceLock<Option<Shown>>,
}

/// Summary of an opened image for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct OpenedInfo {
    pub hdu: usize,
    pub width: usize,
    pub height: usize,
    pub planes: usize,
    pub normalized_from: (f32, f32),
    pub can_debayer: bool,
    /// Default mode: debayer for CFA frames.
    pub default_mode: Mode,
}

/// Raw and normalized values at one source pixel.
#[derive(Debug, Clone, Serialize)]
pub struct PixelValue {
    pub x: usize,
    pub y: usize,
    /// Physical value per plane (e.g. ADU).
    pub raw: Vec<f32>,
    /// Normalized 0–1 value per plane.
    pub norm: Vec<f32>,
}

fn shown(img: Image, mode: Mode, source_step: usize) -> Shown {
    let stats = channel_stats(&img);
    let stf: Vec<Stf> = stats.iter().map(|s| stf_from(s.median, s.mad)).collect();
    let stf_linked = if stats.len() == 3 {
        let w = [0.2126, 0.7152, 0.0722];
        let median = stats.iter().zip(w).map(|(s, k)| s.median * k).sum();
        let mad = stats.iter().zip(w).map(|(s, k)| s.mad * k).sum();
        stf_from(median, mad)
    } else {
        stf[0]
    };
    let stf_linear = stf
        .iter()
        .zip(&stats)
        .map(|(s, c)| Stf {
            shadows: s.shadows,
            midtones: 0.5,
            // White point where the background lands at 15 % grey, but never
            // past the bright end of the data: still a straight line.
            highlights: (s.shadows + (c.median - s.shadows).max(1e-5) / 0.15)
                .min(c.p9999)
                .max(s.shadows + 1e-4)
                .min(1.0),
        })
        .collect();
    let display = Display {
        mode,
        width: img.width,
        height: img.height,
        channels: img.planes,
        source_step,
        stats,
        stf,
        stf_linked,
        stf_linear,
    };
    Shown { img, display }
}

pub(crate) fn stf_from(median: f32, mad: f32) -> Stf {
    use crate::stretch::{SHADOWS_CLIP, TARGET_BACKGROUND, mtf};
    let sigma = mad * 1.4826;
    let shadows = if sigma > 0.0 {
        (median + SHADOWS_CLIP * sigma).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let midtones = mtf(
        TARGET_BACKGROUND,
        ((median - shadows) / (1.0 - shadows)).clamp(0.0, 1.0),
    );
    Stf {
        shadows,
        midtones,
        highlights: 1.0,
    }
}

impl Opened {
    /// Decode and normalize an image HDU. `bayer` is (pattern, x_off, y_off,
    /// bottom_up) from the canonical header fields.
    pub fn open(
        path: impl AsRef<Path>,
        hdu: &Hdu,
        bayer: Option<(&str, i64, i64, bool)>,
    ) -> Result<Opened, DecodeError> {
        let mut img = decode_hdu(path, hdu)?;
        let bzero = hdu.header().float("BZERO").unwrap_or(0.0);
        let normalized_from = normalize(&mut img, hdu.bitpix.unwrap_or(-32), bzero);
        // Cubes other than RGB show their first plane.
        if img.planes != 1 && img.planes != 3 {
            img.planes = 1;
            img.data.truncate(img.width * img.height);
        }
        let cfa = bayer
            .filter(|_| img.planes == 1 && img.width >= 2 && img.height >= 2)
            .and_then(|(p, x, y, bu)| Cfa::new(p, x, y, bu, img.height));
        let (width, height, planes) = (img.width, img.height, img.planes);
        Ok(Opened {
            hdu: hdu.index,
            width,
            height,
            planes,
            normalized_from,
            cfa,
            raw: shown(img, Mode::Raw, 1),
            debayered: OnceLock::new(),
        })
    }

    pub fn info(&self) -> OpenedInfo {
        OpenedInfo {
            hdu: self.hdu,
            width: self.width,
            height: self.height,
            planes: self.planes,
            normalized_from: self.normalized_from,
            can_debayer: self.cfa.is_some(),
            default_mode: if self.cfa.is_some() {
                Mode::Debayer
            } else {
                Mode::Raw
            },
        }
    }

    fn shown(&self, mode: Mode) -> &Shown {
        match mode {
            Mode::Raw => &self.raw,
            Mode::Debayer => self
                .debayered
                .get_or_init(|| {
                    self.cfa
                        .map(|c| shown(superpixel(&self.raw.img, c), Mode::Debayer, 2))
                })
                .as_ref()
                .unwrap_or(&self.raw),
        }
    }

    /// The normalized image a mode displays (debayered or raw).
    pub fn shown_image(&self, mode: Mode) -> &Image {
        &self.shown(mode).img
    }

    pub fn display(&self, mode: Mode) -> &Display {
        &self.shown(mode).display
    }

    /// Whole image, long edge ≤ `max_edge`, as interleaved f16 (little-endian).
    pub fn preview(&self, mode: Mode, max_edge: usize) -> (usize, usize, Vec<u8>) {
        let small = downsample(&self.shown(mode).img, max_edge);
        (small.width, small.height, interleave_f16(&small))
    }

    /// Region `[x, x+w) × [y, y+h)` of the display image, box-averaged by
    /// `step`, as interleaved f16. Clamped to the image.
    pub fn region(
        &self,
        mode: Mode,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        step: usize,
    ) -> (usize, usize, Vec<u8>) {
        let img = &self.shown(mode).img;
        let step = step.max(1);
        let x = x.min(img.width);
        let y = y.min(img.height);
        let (w, h) = (w.min(img.width - x) / step, h.min(img.height - y) / step);
        let plane = img.width * img.height;
        let inv = 1.0 / (step * step) as f32;
        let mut out = vec![f16::ZERO; w * h * img.planes];
        out.par_chunks_mut(w * img.planes)
            .enumerate()
            .for_each(|(oy, row)| {
                for ox in 0..w {
                    for p in 0..img.planes {
                        let mut s = 0.0;
                        for dy in 0..step {
                            let base = p * plane + (y + oy * step + dy) * img.width + x + ox * step;
                            s += img.data[base..base + step].iter().sum::<f32>();
                        }
                        row[ox * img.planes + p] = f16::from_f32(s * inv);
                    }
                }
            });
        (w, h, bytes(&out))
    }

    /// Values at a source pixel (0-based, stored row order).
    pub fn pixel(&self, x: usize, y: usize) -> Option<PixelValue> {
        (x < self.width && y < self.height).then(|| {
            let (lo, hi) = self.normalized_from;
            let plane = self.width * self.height;
            let norm: Vec<f32> = (0..self.planes)
                .map(|p| self.raw.img.data[p * plane + y * self.width + x])
                .collect();
            let raw = norm.iter().map(|v| lo + v * (hi - lo)).collect();
            PixelValue { x, y, raw, norm }
        })
    }
}

fn interleave_f16(img: &Image) -> Vec<u8> {
    let plane = img.width * img.height;
    let mut out = vec![f16::ZERO; plane * img.planes];
    out.par_chunks_mut(img.planes)
        .enumerate()
        .for_each(|(i, px)| {
            for (p, v) in px.iter_mut().enumerate() {
                *v = f16::from_f32(img.data[p * plane + i]);
            }
        });
    bytes(&out)
}

fn bytes(v: &[f16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
