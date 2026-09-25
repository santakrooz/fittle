//! Display preview: decode → normalize → downsample → auto-STF, laid out for
//! a GPU texture. Used by the GUI and (later) MCP `fits_preview`.

use std::path::Path;

use fittle_core::Hdu;

use crate::decode::{DecodeError, decode_hdu};
use crate::stretch::{Stf, auto_stf, downsample, normalize};

#[derive(Debug, Clone, serde::Serialize)]
pub struct PreviewInfo {
    pub hdu: usize,
    /// Full-resolution size.
    pub source_width: usize,
    pub source_height: usize,
    /// Preview texture size.
    pub width: usize,
    pub height: usize,
    /// 1 (mono / CFA) or 3 (RGB).
    pub channels: usize,
    /// Physical values mapped to 0 and 1.
    pub normalized_from: (f32, f32),
    /// Unlinked auto-STF, one per channel.
    pub stf: Vec<Stf>,
}

pub struct Preview {
    pub info: PreviewInfo,
    /// Normalized values, channel-interleaved (`RGBRGB…` or mono), row 0 first.
    pub pixels: Vec<f32>,
}

/// Build a preview of an image HDU with the long edge capped at `max_edge`.
pub fn preview(path: impl AsRef<Path>, hdu: &Hdu, max_edge: usize) -> Result<Preview, DecodeError> {
    let mut img = decode_hdu(path, hdu)?;
    let (source_width, source_height) = (img.width, img.height);
    let bzero = hdu.header().float("BZERO").unwrap_or(0.0);
    let normalized_from = normalize(&mut img, hdu.bitpix.unwrap_or(-32), bzero);
    let mut small = downsample(&img, max_edge);
    // Only mono and RGB are displayable; other cubes show their first plane.
    if small.planes != 3 {
        small.planes = 1;
        small.data.truncate(small.width * small.height);
    }
    let stf = auto_stf(&small);
    let channels = small.planes;
    let plane = small.width * small.height;
    let pixels = if channels == 1 {
        small.data
    } else {
        let mut out = Vec::with_capacity(plane * 3);
        for i in 0..plane {
            out.extend([
                small.data[i],
                small.data[plane + i],
                small.data[2 * plane + i],
            ]);
        }
        out
    };
    let info = PreviewInfo {
        hdu: hdu.index,
        source_width,
        source_height,
        width: small.width,
        height: small.height,
        channels,
        normalized_from,
        stf,
    };
    Ok(Preview { info, pixels })
}
