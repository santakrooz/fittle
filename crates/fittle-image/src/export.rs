//! Export: decode → debayer → orient → crop/rotate/flip/bin/resize →
//! stretch → quantize → encode. The source is never written; outputs go to
//! new, non-existing paths. FITS outputs keep the header (plate solution
//! and Bayer keywords updated to match the pixels).

use std::path::{Path, PathBuf};

use fittle_astro::wcs::Tan;
use fittle_core::edit::{NewValue, Op, format_card};
use fittle_core::{Fits, Info, Value};
use serde::{Deserialize, Serialize};

use crate::debayer::bilinear;
use crate::decode::Image;
use crate::encode::{self, Raster, Samples};
use crate::geom::{self, BinMode};
use crate::stats::channel_stats;
use crate::stretch::Stf;
use crate::view::{Opened, stf_from};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Format {
    /// 8 or 16 bits per sample.
    Png {
        bits: u8,
    },
    Jpeg {
        quality: u8,
    },
    /// Lossless only (lossy needs libwebp; see decision 0005).
    Webp,
    /// 8, 16, or 32 (float).
    Tiff {
        bits: u8,
    },
    /// 32-bit float, header kept.
    Fits,
    /// AV1 still image (8-bit), lossy.
    Avif {
        quality: u8,
    },
}

impl Format {
    pub fn ext(&self) -> &'static str {
        match self {
            Format::Png { .. } => "png",
            Format::Jpeg { .. } => "jpg",
            Format::Webp => "webp",
            Format::Tiff { .. } => "tif",
            Format::Fits => "fits",
            Format::Avif { .. } => "avif",
        }
    }

    /// Parse `png`, `png16`, `jpeg`, `jpg`, `webp`, `avif`, `tiff`, `tiff16`, `tiff32`, `fits`.
    pub fn parse(s: &str) -> Option<Format> {
        Some(match s.to_ascii_lowercase().as_str() {
            "png" | "png8" => Format::Png { bits: 8 },
            "png16" => Format::Png { bits: 16 },
            "jpg" | "jpeg" => Format::Jpeg { quality: 92 },
            "webp" => Format::Webp,
            "tif" | "tiff" | "tiff16" | "tif16" => Format::Tiff { bits: 16 },
            "tiff8" | "tif8" => Format::Tiff { bits: 8 },
            "tiff32" | "tif32" => Format::Tiff { bits: 32 },
            "fit" | "fits" | "fts" => Format::Fits,
            "avif" => Format::Avif { quality: 80 },
            _ => return None,
        })
    }
}

/// How pixel values are mapped for the output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stretch {
    /// Linear data: normalized 0–1 (FITS keeps physical values).
    None,
    /// Auto screen-transfer function, per channel or linked.
    Auto { linked: bool },
    /// Arcsinh from the auto black point, strength set so the background
    /// lands at 0.25 (same as the viewer).
    Asinh,
    /// Exact viewer parameters ("current view" / "export with this stretch").
    /// `asinh` > 0 uses the arcsinh curve with that strength.
    Custom { stf: Vec<Stf>, asinh: f32 },
}

impl Stretch {
    pub fn label(&self) -> &'static str {
        match self {
            Stretch::None => "none (linear)",
            Stretch::Auto { linked: true } => "auto STF, linked",
            Stretch::Auto { linked: false } => "auto STF",
            Stretch::Asinh => "arcsinh",
            Stretch::Custom { .. } => "current view",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crop {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// Everything an export does. Geometry is in displayed orientation (as the
/// viewer shows it, bottom-up files turned upright) and applied in field
/// order: crop, rotate, flip, bin, resize.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportSpec {
    pub format: Format,
    pub stretch: Stretch,
    /// Debayer one-shot-colour frames to RGB (bilinear). Ignored otherwise.
    pub debayer: bool,
    pub crop: Option<Crop>,
    /// Clockwise quarter turns: 0, 90, 180, 270.
    pub rotate: u16,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub bin: Option<u8>,
    pub bin_mode: BinMode,
    /// Resample so the long edge is this many pixels.
    pub long_edge: Option<usize>,
    /// Embed the acquisition summary (XMP; FITS keeps its header).
    pub metadata: bool,
    /// Leave out site location, observer and serial numbers.
    pub private: bool,
    /// Add the share-card caption strip (target, rig, integration, date).
    pub card: bool,
}

impl Default for ExportSpec {
    fn default() -> Self {
        ExportSpec {
            format: Format::Png { bits: 16 },
            stretch: Stretch::Auto { linked: false },
            debayer: true,
            crop: None,
            rotate: 0,
            flip_horizontal: false,
            flip_vertical: false,
            bin: None,
            bin_mode: BinMode::Average,
            long_edge: None,
            metadata: true,
            private: true,
            card: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// The spec cannot be applied to this file (exit code 2).
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Fits(#[from] fittle_core::Error),
    #[error("{0}")]
    Decode(#[from] crate::DecodeError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// What an export produced.
#[derive(Debug, Clone, Serialize)]
pub struct Exported {
    pub path: String,
    pub width: usize,
    pub height: usize,
    pub channels: usize,
    pub format: Format,
    pub bytes: u64,
    /// Steps applied, in order (also written to XMP / FITS HISTORY).
    pub steps: Vec<String>,
    /// True when a plate solution was carried into the output.
    pub wcs: bool,
}

/// Pixels ready to encode, plus what was done to them.
pub struct Rendered {
    pub image: Image,
    pub steps: Vec<String>,
    /// Plate solution in output pixel coordinates (stored row order).
    pub wcs: Option<Tan>,
    /// Geometry changed in a way that invalidates SIP distortion terms.
    pub resampled: bool,
    pub debayered: bool,
    /// Physical (low, high) that 0 and 1 map to.
    pub normalized_from: (f32, f32),
    /// Output rows are stored bottom-up (FITS of a bottom-up source).
    pub bottom_up: bool,
}

fn bayer_of(info: &Info, fits: &Fits, hdu: usize) -> (Option<(String, i64, i64)>, bool) {
    let bottom_up = info
        .fields
        .row_order
        .as_ref()
        .is_some_and(|r| r.value.eq_ignore_ascii_case("BOTTOM-UP"));
    let h = fits.hdus[hdu].header();
    let bayer = info.fields.bayer.as_ref().map(|b| {
        (
            b.value.clone(),
            h.int("XBAYROFF").unwrap_or(0),
            h.int("YBAYROFF").unwrap_or(0),
        )
    });
    (bayer, bottom_up)
}

/// Build the output pixels (no encoding). Validation errors describe what to
/// change in the spec.
pub fn render(
    path: &Path,
    fits: &Fits,
    info: &Info,
    spec: &ExportSpec,
) -> Result<Rendered, ExportError> {
    let summary = info
        .image
        .as_ref()
        .ok_or_else(|| ExportError::Invalid("no image in this file".into()))?;
    let hdu = &fits.hdus[summary.hdu];
    let (bayer, bottom_up) = bayer_of(info, fits, summary.hdu);
    let opened = Opened::open(
        path,
        hdu,
        bayer
            .as_ref()
            .map(|(p, x, y)| (p.as_str(), *x, *y, bottom_up)),
    )?;
    render_opened(&opened, info, Some(hdu.header()), bottom_up, spec, None)
}

/// Whether stored rows run bottom-up for this file.
pub fn bottom_up(info: &Info) -> bool {
    info.fields
        .row_order
        .as_ref()
        .is_some_and(|r| r.value.eq_ignore_ascii_case("BOTTOM-UP"))
}

/// A quick look at what `spec` produces, long edge ≤ `max_edge`, from an
/// image already open for viewing (superpixel debayer, box-downsampled
/// first). Bin and resize are left out; `plan` reports the real size.
pub fn preview(
    opened: &Opened,
    info: &Info,
    spec: &ExportSpec,
    max_edge: usize,
) -> Result<Rendered, ExportError> {
    render_opened(
        opened,
        info,
        None,
        bottom_up(info),
        spec,
        Some(max_edge.max(16)),
    )
}

fn render_opened(
    opened: &Opened,
    info: &Info,
    header: Option<&fittle_core::Header>,
    bottom_up: bool,
    spec: &ExportSpec,
    preview: Option<usize>,
) -> Result<Rendered, ExportError> {
    if ![0, 90, 180, 270].contains(&spec.rotate) {
        return Err(ExportError::Invalid(
            "rotate must be 0, 90, 180 or 270".into(),
        ));
    }
    let mut steps = Vec::new();
    let mut wcs = header.and_then(fittle_core::derive::wcs_tan);
    let raw_cfa = opened.cfa.is_some() && !spec.debayer;
    // Source pixels per working pixel (preview only).
    let mut step = 1usize;
    let mut img = match (opened.cfa, preview) {
        (Some(_), Some(_)) if spec.debayer => {
            step = 2;
            opened.shown_image(crate::Mode::Debayer).clone()
        }
        (Some(cfa), None) if spec.debayer => {
            steps.push("debayer (bilinear)".into());
            bilinear(opened.shown_image(crate::Mode::Raw), cfa)
        }
        _ => opened.shown_image(crate::Mode::Raw).clone(),
    };
    if let Some(edge) = preview {
        let f = img.width.max(img.height).div_ceil(edge).max(1);
        if f > 1 {
            img = crate::stretch::downsample(&img, edge);
            step *= f;
        }
        wcs = None;
    }
    if raw_cfa
        && (spec.rotate != 0
            || spec.flip_horizontal
            || spec.flip_vertical
            || spec.bin.is_some_and(|b| b > 1)
            || spec.long_edge.is_some())
    {
        return Err(ExportError::Invalid(
            "a raw colour (CFA) frame can only be cropped; debayer it to rotate, flip, bin or resize".into(),
        ));
    }

    // Auto parameters come from the whole (debayered) frame, so a crop looks
    // the same as it did in the viewer.
    let auto: Vec<Stf> = match &spec.stretch {
        Stretch::Auto { .. } | Stretch::Asinh => {
            let stats = channel_stats(&img);
            let per: Vec<Stf> = stats.iter().map(|s| stf_from(s.median, s.mad)).collect();
            let linked = if stats.len() == 3 {
                let w = [0.2126, 0.7152, 0.0722];
                stf_from(
                    stats.iter().zip(w).map(|(s, k)| s.median * k).sum(),
                    stats.iter().zip(w).map(|(s, k)| s.mad * k).sum(),
                )
            } else {
                per[0]
            };
            match &spec.stretch {
                Stretch::Auto { linked: false } => per,
                Stretch::Asinh => {
                    let black = linked.shadows;
                    let median = stats.iter().map(|s| s.median).sum::<f32>() / stats.len() as f32;
                    let k = asinh_strength((median - black) / (1.0 - black));
                    vec![Stf {
                        shadows: black,
                        midtones: k,
                        highlights: 1.0,
                    }]
                }
                _ => vec![linked],
            }
        }
        _ => Vec::new(),
    };

    // Displayed orientation.
    if bottom_up {
        img = geom::flip(&img, false);
        wcs = wcs.map(|t| t.flip(false, img.width as f64, img.height as f64));
    }
    let mut resampled = false;
    if let Some(c) = spec.crop {
        let (mut x, mut y, mut w, mut h) = (
            c.x / step,
            c.y / step,
            (c.width / step).max(1),
            (c.height / step).max(1),
        );
        if x >= img.width || y >= img.height || w == 0 || h == 0 {
            return Err(ExportError::Invalid(format!(
                "crop {w}×{h}+{x}+{y} is outside the {}×{} image",
                img.width, img.height
            )));
        }
        w = w.min(img.width - x);
        h = h.min(img.height - y);
        if raw_cfa && preview.is_none() {
            // Keep the Bayer phase: even origin (in stored rows) and even size.
            x &= !1;
            w &= !1;
            h &= !1;
            let stored_y = if bottom_up { img.height - y - h } else { y };
            if stored_y % 2 == 1 {
                y = if bottom_up { y + 1 } else { y - 1 };
            }
            if w == 0 || h == 0 {
                return Err(ExportError::Invalid(
                    "crop too small for a colour (CFA) frame".into(),
                ));
            }
        }
        img = geom::crop(&img, x, y, w, h);
        wcs = wcs.map(|t| t.crop(x as f64, y as f64));
        steps.push(format!("crop {w}×{h} at ({x}, {y})"));
    }
    for _ in 0..spec.rotate / 90 {
        wcs = wcs.map(|t| t.rotate_cw(img.height as f64));
        img = geom::rotate_cw(&img);
        resampled = true;
    }
    if spec.rotate != 0 {
        steps.push(format!("rotate {}° clockwise", spec.rotate));
    }
    for (on, horizontal, name) in [
        (spec.flip_horizontal, true, "flip horizontal"),
        (spec.flip_vertical, false, "flip vertical"),
    ] {
        if on {
            wcs = wcs.map(|t| t.flip(horizontal, img.width as f64, img.height as f64));
            img = geom::flip(&img, horizontal);
            resampled = true;
            steps.push(name.into());
        }
    }
    if let Some(n) = spec.bin.filter(|&n| n > 1 && preview.is_none()) {
        let n = n as usize;
        if img.width < n || img.height < n {
            return Err(ExportError::Invalid(format!(
                "bin {n} is larger than the image"
            )));
        }
        img = geom::bin(&img, n, spec.bin_mode);
        wcs = wcs.map(|t| t.scale(1.0 / n as f64));
        resampled = true;
        steps.push(format!(
            "bin {n}×{n} ({})",
            if spec.bin_mode == BinMode::Sum {
                "sum"
            } else {
                "average"
            }
        ));
    }
    if let Some(edge) = spec.long_edge.filter(|_| preview.is_none()) {
        let long = img.width.max(img.height);
        if edge == 0 {
            return Err(ExportError::Invalid(
                "long edge must be at least 1 px".into(),
            ));
        }
        if edge != long {
            let f = edge as f64 / long as f64;
            let (w, h) = (
                ((img.width as f64 * f).round() as usize).max(1),
                ((img.height as f64 * f).round() as usize).max(1),
            );
            img = geom::resize(&img, w, h);
            wcs = wcs.map(|t| t.scale(f));
            resampled = true;
            steps.push(format!("resize to {w}×{h} (Lanczos3)"));
        }
    }

    // FITS keeps the source's row order; images are written top-down.
    let out_bottom_up = bottom_up && spec.format == Format::Fits && preview.is_none();
    if out_bottom_up {
        img = geom::flip(&img, false);
        wcs = wcs.map(|t| t.flip(false, img.width as f64, img.height as f64));
    }

    match &spec.stretch {
        Stretch::None => {}
        Stretch::Auto { .. } => apply_stf(&mut img, &auto, 0.0),
        Stretch::Asinh => {
            let s = auto[0];
            apply_stf(&mut img, &[Stf { midtones: 0.5, ..s }], s.midtones);
        }
        Stretch::Custom { stf, asinh } => {
            if stf.is_empty() {
                return Err(ExportError::Invalid(
                    "current-view stretch has no parameters".into(),
                ));
            }
            apply_stf(&mut img, stf, *asinh);
        }
    }
    if spec.stretch != Stretch::None {
        steps.push(format!(
            "display stretch: {} (non-linear)",
            spec.stretch.label()
        ));
    }
    if spec.card {
        if spec.format == Format::Fits {
            return Err(ExportError::Invalid(
                "a share card is an image; pick PNG, JPEG, WebP, AVIF or TIFF".into(),
            ));
        }
        let cap = crate::card::caption(info, spec.stretch.label(), spec.private);
        img = crate::card::compose(&img, &cap);
        steps.push("share card: caption strip added".into());
    }
    Ok(Rendered {
        image: img,
        steps,
        wcs,
        resampled,
        debayered: opened.cfa.is_some() && spec.debayer,
        normalized_from: opened.normalized_from,
        bottom_up: out_bottom_up,
    })
}

/// Same as the viewer's `asinhStrength`: k so asinh(k·bg)/asinh(k) = 0.25.
fn asinh_strength(background: f32) -> f32 {
    let target = 0.25f64;
    let bg = background as f64;
    if bg <= 0.0 || bg >= target {
        return 1.0;
    }
    let (mut lo, mut hi) = (1e-3f64, 1e7f64);
    for _ in 0..80 {
        let k = (lo * hi).sqrt();
        if (k * bg).asinh() / k.asinh() < target {
            lo = k;
        } else {
            hi = k;
        }
    }
    (lo * hi).sqrt() as f32
}

/// Per-plane stretch (a single entry applies to every plane). Same formula
/// as the viewer's shader.
fn apply_stf(img: &mut Image, stf: &[Stf], asinh: f32) {
    use rayon::prelude::*;
    let plane = img.width * img.height;
    let norm = if asinh > 0.0 { asinh.asinh() } else { 1.0 };
    img.data
        .par_chunks_mut(plane)
        .enumerate()
        .for_each(|(p, d)| {
            let s = stf[p.min(stf.len() - 1)];
            let span = (s.highlights - s.shadows).max(1e-9);
            for v in d {
                let x = ((*v - s.shadows) / span).clamp(0.0, 1.0);
                *v = if asinh > 0.0 {
                    (asinh * x).asinh() / norm
                } else {
                    crate::stretch::mtf(s.midtones, x)
                };
            }
        });
}

/// Interleave planes and quantize for an image format.
pub fn raster(img: &Image, bits: u8) -> Raster {
    let plane = img.width * img.height;
    let n = img.planes;
    let at = |i: usize| img.data[(i % n) * plane + i / n].clamp(0.0, 1.0);
    let samples = match bits {
        8 => Samples::U8(
            (0..plane * n)
                .map(|i| (at(i) * 255.0).round() as u8)
                .collect(),
        ),
        16 => Samples::U16(
            (0..plane * n)
                .map(|i| (at(i) * 65535.0).round() as u16)
                .collect(),
        ),
        _ => Samples::F32((0..plane * n).map(at).collect()),
    };
    Raster {
        width: img.width,
        height: img.height,
        channels: n,
        samples,
    }
}

/// Keys the output FITS header rewrites itself (structure, compression,
/// checksums) and never copies from the source.
fn structural(key: &str) -> bool {
    matches!(
        key,
        "SIMPLE"
            | "XTENSION"
            | "BITPIX"
            | "EXTEND"
            | "PCOUNT"
            | "GCOUNT"
            | "TFIELDS"
            | "BZERO"
            | "BSCALE"
            | "BLANK"
            | "CHECKSUM"
            | "DATASUM"
            | "DATAMIN"
            | "DATAMAX"
            | "END"
            | "THEAP"
            | "EXTNAME"
            | "ZIMAGE"
            | "ZBITPIX"
            | "ZCMPTYPE"
            | "ZQUANTIZ"
            | "ZDITHER0"
            | "ZSIMPLE"
            | "ZEXTEND"
            | "ZHECKSUM"
            | "ZDATASUM"
            | "ZTENSION"
            | "ZPCOUNT"
            | "ZGCOUNT"
            | "ZBLANK"
            | "ZSCALE"
            | "ZZERO"
    ) || [
        "NAXIS", "ZNAXIS", "ZTILE", "ZNAME", "ZVAL", "TTYPE", "TFORM", "TUNIT",
    ]
    .iter()
    .any(|p| key.starts_with(p) && key[p.len()..].chars().all(|c| c.is_ascii_digit()))
}

fn wcs_key(key: &str) -> bool {
    let sip = ["A_", "B_", "AP_", "BP_"]
        .iter()
        .any(|p| key.starts_with(p));
    sip || matches!(
        key,
        "WCSAXES" | "CROTA1" | "CROTA2" | "LONPOLE" | "LATPOLE" | "EQUINOX" | "RADESYS"
    ) || ["CRPIX", "CRVAL", "CDELT", "CTYPE", "CUNIT"]
        .iter()
        .any(|p| key.starts_with(p) && key[p.len()..].chars().all(|c| c.is_ascii_digit()))
        || (key.starts_with("CD") || key.starts_with("PC")) && key[2..].contains('_')
}

fn card(key: &str, v: NewValue, comment: &str) -> Vec<String> {
    format_card(key, &v, Some(comment))
}

/// Header records (without END) for a FITS export.
pub fn fits_records(fits: &Fits, info: &Info, r: &Rendered, spec: &ExportSpec) -> Vec<String> {
    let hdu = info.image.as_ref().map_or(0, |i| i.hdu);
    let src = fits.hdus[hdu].header();
    let img = &r.image;
    let mut out: Vec<String> = Vec::new();
    out.extend(card(
        "SIMPLE",
        NewValue::Logical(true),
        "conforms to FITS standard",
    ));
    out.extend(card("BITPIX", NewValue::Integer(-32), "32-bit float"));
    out.extend(card(
        "NAXIS",
        NewValue::Integer(if img.planes > 1 { 3 } else { 2 }),
        "number of axes",
    ));
    out.extend(card("NAXIS1", NewValue::Integer(img.width as i64), "width"));
    out.extend(card(
        "NAXIS2",
        NewValue::Integer(img.height as i64),
        "height",
    ));
    if img.planes > 1 {
        out.extend(card(
            "NAXIS3",
            NewValue::Integer(img.planes as i64),
            "colour planes",
        ));
    }

    let scrub: Vec<Op> = if spec.private {
        fittle_core::privacy::scrub_ops(fits, &info.path)
    } else {
        Vec::new()
    };
    let rewrite_wcs = r.wcs.is_some() && (r.resampled || spec.crop.is_some() || r.bottom_up);
    let drop_bayer = r.debayered;
    for c in &src.cards {
        let k = c.keyword.as_str();
        if structural(k) || (rewrite_wcs && wcs_key(k)) {
            continue;
        }
        if drop_bayer && matches!(k, "BAYERPAT" | "XBAYROFF" | "YBAYROFF" | "COLORTYP") {
            continue;
        }
        if !matches!(c.value, Value::Commentary(_)) {
            match scrub
                .iter()
                .find(|o| matches!(o, Op::Unset { key } | Op::Set { key, .. } if key == k))
            {
                Some(Op::Unset { .. }) => continue,
                Some(Op::Set {
                    key,
                    value,
                    comment,
                }) => {
                    out.extend(format_card(key, value, comment.as_deref()));
                    continue;
                }
                _ => {}
            }
        }
        out.extend(c.raw.iter().cloned());
    }
    if let Some(t) = r.wcs.filter(|_| rewrite_wcs) {
        let s = |k: &str, v: &str, c: &str| card(k, NewValue::String(v.into()), c);
        let f = |k: &str, v: f64, c: &str| card(k, NewValue::Float(v), c);
        out.extend(card("WCSAXES", NewValue::Integer(2), "celestial axes"));
        out.extend(s("CTYPE1", "RA---TAN", "gnomonic projection"));
        out.extend(s("CTYPE2", "DEC--TAN", "gnomonic projection"));
        out.extend(f("CRVAL1", t.crval[0], "RA of reference pixel (deg)"));
        out.extend(f("CRVAL2", t.crval[1], "Dec of reference pixel (deg)"));
        out.extend(f("CRPIX1", t.crpix[0], "reference pixel x"));
        out.extend(f("CRPIX2", t.crpix[1], "reference pixel y"));
        out.extend(f("CD1_1", t.cd[0][0], "deg/px"));
        out.extend(f("CD1_2", t.cd[0][1], "deg/px"));
        out.extend(f("CD2_1", t.cd[1][0], "deg/px"));
        out.extend(f("CD2_2", t.cd[1][1], "deg/px"));
        if let Some(e) = src.float("EQUINOX") {
            out.extend(f("EQUINOX", e, "equinox of coordinates"));
        }
        if let Some(rs) = src.string("RADESYS") {
            out.extend(s("RADESYS", rs, "reference frame"));
        }
    }
    let mut history = vec![format!(
        "Exported by Fittle {} from {}",
        env!("CARGO_PKG_VERSION"),
        file_name(&info.path)
    )];
    history.extend(r.steps.iter().cloned());
    if spec.stretch == Stretch::None {
        history.push("pixel values are physical (linear), as in the source".into());
    } else {
        history.push("pixel values are display-stretched 0-1, not linear data".into());
    }
    if rewrite_wcs && r.resampled && src.cards.iter().any(|c| c.keyword == "A_ORDER") {
        history.push("SIP distortion terms dropped: geometry changed".into());
    }
    for h in history {
        out.extend(format_history(&h));
    }
    out
}

fn format_history(text: &str) -> Vec<String> {
    let chars: Vec<char> = text
        .chars()
        .filter(|c| c.is_ascii() && !c.is_control())
        .collect();
    chars
        .chunks(72)
        .map(|c| format!("HISTORY {}", c.iter().collect::<String>()))
        .collect()
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// A path in `dir` named `stem.ext` that does not exist and is not the
/// source; `_2`, `_3`… are appended as needed.
pub fn unique_path(dir: &Path, stem: &str, ext: &str, source: &Path) -> PathBuf {
    let src = source.canonicalize().ok();
    let taken = |p: &Path| {
        p.exists()
            || src
                .as_deref()
                .is_some_and(|s| p.canonicalize().ok().as_deref() == Some(s))
    };
    let mut p = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while taken(&p) {
        p = dir.join(format!("{stem}_{n}.{ext}"));
        n += 1;
    }
    p
}

/// Encode rendered pixels to `out` (which must not exist). Writes to a
/// temporary sibling first, so a failed export leaves nothing behind.
pub fn write(
    out: &Path,
    source: &Path,
    fits: &Fits,
    info: &Info,
    r: &Rendered,
    spec: &ExportSpec,
) -> Result<u64, ExportError> {
    if out.exists() {
        return Err(ExportError::Invalid(format!(
            "{} already exists; exports never overwrite",
            out.display()
        )));
    }
    if let (Ok(a), Some(b)) = (
        source.canonicalize(),
        out.parent().and_then(|d| d.canonicalize().ok()),
    ) {
        if out.file_name().is_some_and(|n| b.join(n) == a) {
            return Err(ExportError::Invalid(
                "the output would replace the source".into(),
            ));
        }
    }
    let tmp = out.with_file_name(format!(
        ".{}.fittle-part",
        out.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    ));
    let xmp = spec
        .metadata
        .then(|| crate::xmp::packet(info, &r.steps, spec.private));
    let x = xmp.as_deref();
    let res = match spec.format {
        Format::Png { bits } => {
            encode::png(&tmp, &raster(&r.image, if bits == 16 { 16 } else { 8 }), x)
        }
        Format::Jpeg { quality } => encode::jpeg(&tmp, &raster(&r.image, 8), quality, x),
        Format::Webp => encode::webp(&tmp, &raster(&r.image, 8), x),
        Format::Tiff { bits } => encode::tiff(&tmp, &raster(&r.image, bits), x),
        Format::Avif { quality } => encode::avif(
            &tmp,
            &raster(&r.image, 8),
            quality,
            // The summary line carries no site or serial numbers.
            spec.metadata
                .then(|| encode::exif(&crate::xmp::summary(info), "Fittle")),
        ),
        Format::Fits => {
            let (lo, hi) = r.normalized_from;
            let physical = spec.stretch == Stretch::None;
            let data: Vec<u8> = r
                .image
                .data
                .iter()
                .flat_map(|v| {
                    let v = if physical { v * (hi - lo) + lo } else { *v };
                    v.to_be_bytes()
                })
                .collect();
            encode::fits(&tmp, &fits_records(fits, info, r, spec), &data)
        }
    };
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    // No-clobber rename: hard-link then unlink, so a file that appeared
    // meanwhile is never replaced.
    match std::fs::hard_link(&tmp, out) {
        Ok(()) => std::fs::remove_file(&tmp)?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = std::fs::remove_file(&tmp);
            return Err(ExportError::Invalid(format!(
                "{} already exists; exports never overwrite",
                out.display()
            )));
        }
        // File systems without hard links (exFAT, some shares): checked rename.
        Err(_) if !out.exists() => std::fs::rename(&tmp, out)?,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }
    }
    Ok(std::fs::metadata(out)?.len())
}

/// Open, render and write in one call. `out` is a directory or a file path;
/// a directory gets `template` (see `fittle_core::naming`), default `{name}`.
pub fn export(
    source: &Path,
    out: &Path,
    template: Option<&str>,
    spec: &ExportSpec,
) -> Result<Exported, ExportError> {
    let fits = Fits::open(source)?;
    let info = fittle_core::info_from(&fits, &source.to_string_lossy());
    let r = render(source, &fits, &info, spec)?;
    let path = if out.is_dir() {
        let (stem, _) = fittle_core::naming::render(template.unwrap_or("{name}"), &info, 1);
        unique_path(out, &stem, spec.format.ext(), source)
    } else {
        out.to_path_buf()
    };
    let bytes = write(&path, source, &fits, &info, &r, spec)?;
    Ok(Exported {
        path: path.to_string_lossy().into_owned(),
        width: r.image.width,
        height: r.image.height,
        channels: r.image.planes,
        format: spec.format,
        bytes,
        steps: r.steps,
        wcs: r.wcs.is_some() && spec.format == Format::Fits,
    })
}

/// What an export will produce, from the header alone (no decode): output
/// name, size and an approximate file size. Used for the live dialog.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub file_name: String,
    /// Template tokens the file lacks (written as `unknown`).
    pub missing: Vec<String>,
    pub width: usize,
    pub height: usize,
    pub channels: usize,
    /// Rough; compressed formats depend on content.
    pub estimate_bytes: u64,
    /// Why this spec can't be applied, if it can't.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

pub fn plan(info: &Info, spec: &ExportSpec, template: &str) -> Plan {
    let (name, missing) = fittle_core::naming::render(template, info, 1);
    let file_name = format!("{name}.{}", spec.format.ext());
    let img = info.image.as_ref();
    let (mut w, mut h) = img.map_or((0, 0), |i| (i.width as usize, i.height as usize));
    let planes = img.map_or(1, |i| if i.planes == 3 { 3 } else { 1 });
    let cfa = planes == 1 && info.fields.bayer.is_some();
    let channels = if cfa && spec.debayer { 3 } else { planes };
    let mut problem = img.is_none().then(|| "no image in this file".to_string());
    if ![0, 90, 180, 270].contains(&spec.rotate) {
        problem = Some("rotate must be 0, 90, 180 or 270".into());
    }
    if cfa
        && !spec.debayer
        && (spec.rotate != 0
            || spec.flip_horizontal
            || spec.flip_vertical
            || spec.bin.is_some_and(|b| b > 1)
            || spec.long_edge.is_some())
    {
        problem = Some("a raw colour (CFA) frame can only be cropped; debayer it to rotate, flip, bin or resize".into());
    }
    if let Some(c) = spec.crop {
        if c.x >= w || c.y >= h || c.width == 0 || c.height == 0 {
            problem = Some("crop is outside the image".into());
        } else {
            (w, h) = (c.width.min(w - c.x), c.height.min(h - c.y));
            if cfa && !spec.debayer {
                (w, h) = (w & !1, h & !1);
            }
        }
    }
    if spec.rotate % 180 == 90 {
        (w, h) = (h, w);
    }
    if let Some(n) = spec.bin.filter(|&n| n > 1) {
        (w, h) = (w / n as usize, h / n as usize);
    }
    if let Some(edge) = spec.long_edge.filter(|&e| e > 0) {
        let f = edge as f64 / w.max(h).max(1) as f64;
        (w, h) = (
            ((w as f64 * f).round() as usize).max(1),
            ((h as f64 * f).round() as usize).max(1),
        );
    }
    let mut channels = channels;
    if spec.card {
        if spec.format == Format::Fits {
            problem = Some("a share card is an image; pick PNG, JPEG, WebP, AVIF or TIFF".into());
        } else {
            (w, h, _) = crate::card::size(w, h);
            channels = 3;
        }
    }
    let samples = (w * h * channels) as f64;
    let estimate = match spec.format {
        Format::Png { bits } => samples * bits as f64 / 8.0 * 0.6,
        Format::Jpeg { quality } => samples * (0.05 + 0.25 * (quality as f64 / 100.0).powi(3)),
        Format::Webp => samples * 0.45,
        Format::Tiff { bits } => samples * bits as f64 / 8.0,
        Format::Fits => (samples * 4.0 / 2880.0).ceil() * 2880.0 + 2880.0 * 4.0,
        Format::Avif { quality } => samples * (0.02 + 0.12 * (quality as f64 / 100.0).powi(3)),
    };
    Plan {
        file_name,
        missing,
        width: w,
        height: h,
        channels,
        estimate_bytes: estimate as u64,
        problem,
    }
}
