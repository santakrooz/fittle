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
