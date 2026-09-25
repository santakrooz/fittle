//! Gnomonic (TAN) world coordinates: pixel → sky. SIP distortion terms are
//! not applied (error is typically < 1″ near the centre, more at corners).

use crate::{Equatorial, deg, rad, wrap360};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tan {
    /// Sky position of the reference pixel (degrees).
    pub crval: [f64; 2],
    /// Reference pixel, FITS 1-based.
    pub crpix: [f64; 2],
    /// Linear transform, degrees per pixel.
    pub cd: [[f64; 2]; 2],
}

impl Tan {
    /// Sky position of a 0-based pixel in stored order.
    pub fn pixel_to_sky(&self, x: f64, y: f64) -> Equatorial {
        let (dx, dy) = (x + 1.0 - self.crpix[0], y + 1.0 - self.crpix[1]);
        let xi = rad(self.cd[0][0] * dx + self.cd[0][1] * dy);
        let eta = rad(self.cd[1][0] * dx + self.cd[1][1] * dy);
        let (ra0, dec0) = (rad(self.crval[0]), rad(self.crval[1]));
        let den = dec0.cos() - eta * dec0.sin();
        let ra = ra0 + xi.atan2(den);
        let dec = (dec0.sin() + eta * dec0.cos()).atan2((xi * xi + den * den).sqrt());
        Equatorial {
            ra: wrap360(deg(ra)),
            dec: deg(dec),
        }
    }

    /// Position angle of image "up" (+y) east of north, degrees.
    pub fn rotation(&self) -> f64 {
        wrap360(deg((-self.cd[0][1]).atan2(self.cd[1][1])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_pixel_maps_to_crval() {
        let t = Tan {
            crval: [10.0, 41.0],
            crpix: [100.0, 50.0],
            cd: [[-0.001, 0.0], [0.0, 0.001]],
        };
        let s = t.pixel_to_sky(99.0, 49.0);
        assert!((s.ra - 10.0).abs() < 1e-9 && (s.dec - 41.0).abs() < 1e-9);
        // One pixel up is +0.001° in Dec; one pixel right is east-to-west (RA decreases).
        let up = t.pixel_to_sky(99.0, 50.0);
        assert!((up.dec - 41.001).abs() < 1e-6);
        let right = t.pixel_to_sky(100.0, 49.0);
        assert!(right.ra < 10.0);
        assert!((t.rotation() - 0.0).abs() < 1e-9);
    }
}

/// Geometric transforms of a TAN solution, so exported images keep a valid
/// plate solution. Pixel coordinates follow FITS (1-based centres).
impl Tan {
    /// Crop starting at 0-based pixel (x0, y0).
    pub fn crop(mut self, x0: f64, y0: f64) -> Tan {
        self.crpix[0] -= x0;
        self.crpix[1] -= y0;
        self
    }

    /// Resample by `f` output pixels per input pixel (0.5 = half size,
    /// bin 2 = 0.5). Pixel edges stay aligned.
    pub fn scale(mut self, f: f64) -> Tan {
        for i in 0..2 {
            self.crpix[i] = (self.crpix[i] - 0.5) * f + 0.5;
        }
        for row in &mut self.cd {
            for v in row {
                *v /= f;
            }
        }
        self
    }

    /// Mirror columns (`horizontal`) or rows in an image `w` × `h`.
    pub fn flip(mut self, horizontal: bool, w: f64, h: f64) -> Tan {
        let (axis, size) = if horizontal { (0, w) } else { (1, h) };
        self.crpix[axis] = size + 1.0 - self.crpix[axis];
        for row in &mut self.cd {
            row[axis] = -row[axis];
        }
        self
    }

    /// Rotate the image 90° clockwise (as displayed with row 0 at the top) in
    /// an image of height `h`; a `w` × `h` image becomes `h` × `w`.
    pub fn rotate_cw(self, h: f64) -> Tan {
        // New pixel (x', y') = (h + 1 − y, x): old x = y', old y = h + 1 − x'.
        let c = self.cd;
        Tan {
            crval: self.crval,
            crpix: [h + 1.0 - self.crpix[1], self.crpix[0]],
            // d(old)/d(new): dx = dy', dy = −dx'
            cd: [[-c[0][1], c[0][0]], [-c[1][1], c[1][0]]],
        }
    }
}

#[cfg(test)]
mod transform_tests {
    use super::*;

    fn tan() -> Tan {
        Tan {
            crval: [83.8, -5.39],
            crpix: [512.3, 300.7],
            cd: [[-2.1e-4, 3.0e-5], [2.9e-5, 2.1e-4]],
        }
    }

    fn same(a: crate::Equatorial, b: crate::Equatorial) {
        assert!(
            (a.ra - b.ra).abs() < 1e-9 && (a.dec - b.dec).abs() < 1e-9,
            "{a:?} vs {b:?}"
        );
    }

    #[test]
    fn crop_keeps_sky() {
        let t = tan();
        same(
            t.pixel_to_sky(700.0, 400.0),
            t.crop(100.0, 50.0).pixel_to_sky(600.0, 350.0),
        );
    }

    #[test]
    fn scale_keeps_sky_at_pixel_centres() {
        // Bin 2: output pixel (i, j) covers input pixels 2i..2i+1, centre at 2i + 0.5.
        let t = tan();
        let s = t.scale(0.5);
        same(
            t.pixel_to_sky(2.0 * 100.0 + 0.5, 2.0 * 60.0 + 0.5),
            s.pixel_to_sky(100.0, 60.0),
        );
    }

    #[test]
    fn flips_and_rotation_keep_sky() {
        let (w, h) = (1024.0, 600.0);
        let t = tan();
        let (x, y) = (700.0, 123.0);
        same(
            t.pixel_to_sky(x, y),
            t.flip(true, w, h).pixel_to_sky(w - 1.0 - x, y),
        );
        same(
            t.pixel_to_sky(x, y),
            t.flip(false, w, h).pixel_to_sky(x, h - 1.0 - y),
        );
        // Clockwise: 0-based (x, y) → (h − 1 − y, x).
        same(
            t.pixel_to_sky(x, y),
            t.rotate_cw(h).pixel_to_sky(h - 1.0 - y, x),
        );
    }
}
