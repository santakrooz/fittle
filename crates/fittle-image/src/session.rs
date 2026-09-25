//! One file opened for viewing: `info`, pixels, and plate solution together.
//! Shared by the GUI and (later) MCP `fits_preview` / `fits_stats`.

use std::path::Path;

use fittle_astro::wcs::Tan;
use fittle_core::{Fits, Info};
use serde::Serialize;

use crate::decode::DecodeError;
use crate::view::{Opened, PixelValue};

pub struct ViewSession {
    pub path: String,
    pub info: Info,
    pub image: Option<Opened>,
    /// Why the image could not be shown, if it could not.
    pub image_error: Option<String>,
    pub wcs: Option<Tan>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Readout {
    #[serde(flatten)]
    pub pixel: PixelValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ra: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dec: Option<f64>,
}

impl ViewSession {
    pub fn open(path: impl AsRef<Path>) -> Result<ViewSession, fittle_core::Error> {
        let path_ref = path.as_ref();
        let p = path_ref.to_string_lossy().to_string();
        let fits = Fits::open(path_ref)?;
        let info = fittle_core::info_from(&fits, &p);
        let hdu = info.image.as_ref().map(|i| &fits.hdus[i.hdu]);
        let wcs = hdu.and_then(|h| fittle_core::derive::wcs_tan(h.header()));
        let (image, image_error) = match hdu {
            None => (None, Some("no image in this file".to_string())),
            Some(h) => {
                let bottom_up = info
                    .fields
                    .row_order
                    .as_ref()
                    .is_some_and(|r| r.value.eq_ignore_ascii_case("BOTTOM-UP"));
                let hd = h.header();
                let off = |k: &str| hd.int(k).unwrap_or(0);
                let bayer = info.fields.bayer.as_ref().map(|b| {
                    (
                        b.value.as_str(),
                        off("XBAYROFF"),
                        off("YBAYROFF"),
                        bottom_up,
                    )
                });
                match Opened::open(path_ref, h, bayer) {
                    Ok(o) => (Some(o), None),
                    Err(e @ DecodeError::Unsupported(_)) | Err(e @ DecodeError::Truncated) => {
                        (None, Some(e.to_string()))
                    }
                    Err(e) => (None, Some(e.to_string())),
                }
            }
        };
        Ok(ViewSession {
            path: p,
            info,
            image,
            image_error,
            wcs,
        })
    }

    /// Pixel values and sky position at a source pixel (stored order).
    pub fn readout(&self, x: usize, y: usize) -> Option<Readout> {
        let pixel = self.image.as_ref()?.pixel(x, y)?;
        let sky = self.wcs.map(|w| w.pixel_to_sky(x as f64, y as f64));
        Some(Readout {
            pixel,
            ra: sky.map(|s| s.ra),
            dec: sky.map(|s| s.dec),
        })
    }
}
