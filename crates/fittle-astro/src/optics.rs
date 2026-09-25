//! Plate scale, field of view and sampling.

/// Arcseconds per radian / 1000 (µm → mm).
const ARCSEC_PER_UM_PER_MM: f64 = 206.264_806;

/// Pixel scale in arcsec/px from pixel size (µm, binned) and focal length (mm).
pub fn pixel_scale(pixel_um: f64, focal_mm: f64) -> Option<f64> {
    (pixel_um > 0.0 && focal_mm > 0.0).then(|| ARCSEC_PER_UM_PER_MM * pixel_um / focal_mm)
}

/// Pixel scale (arcsec/px) from a WCS CD matrix (degrees/px).
pub fn wcs_scale(cd: [[f64; 2]; 2]) -> Option<f64> {
    let det = (cd[0][0] * cd[1][1] - cd[0][1] * cd[1][0]).abs();
    (det > 0.0).then(|| det.sqrt() * 3600.0)
}

/// Field of view in arcminutes for an image of `width` × `height` pixels.
pub fn field_of_view(scale_arcsec: f64, width: u64, height: u64) -> (f64, f64) {
    (
        scale_arcsec * width as f64 / 60.0,
        scale_arcsec * height as f64 / 60.0,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sampling {
    Over,
    Well,
    Under,
}

/// Sampling relative to typical amateur seeing (≈2–4″ FWHM): Nyquist wants
/// 2–3 px per FWHM, so 0.8–2.0″/px is "well sampled".
pub fn sampling(scale_arcsec: f64) -> Sampling {
    if scale_arcsec < 0.8 {
        Sampling::Over
    } else if scale_arcsec <= 2.0 {
        Sampling::Well
    } else {
        Sampling::Under
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seestar_s50() {
        // 2.9 µm at 250 mm → 2.39″/px; 1080×1920 → 43′ × 76′ (PLAN mock).
        let s = pixel_scale(2.9, 250.0).unwrap();
        assert!((s - 2.393).abs() < 0.001);
        let (w, h) = field_of_view(s, 1080, 1920);
        assert_eq!((w.round(), h.round()), (43.0, 77.0));
        assert_eq!(sampling(s), Sampling::Under);
        assert!(pixel_scale(2.9, 0.0).is_none());
    }

    #[test]
    fn wcs() {
        let s = wcs_scale([[-0.000392, 0.0], [0.0, 0.000392]]).unwrap();
        assert!((s - 1.4112).abs() < 1e-4);
    }
}
