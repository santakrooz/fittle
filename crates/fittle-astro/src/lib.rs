//! Positional astronomy for "where and when": time, coordinates, alt/az,
//! airmass, low-precision sun and moon, and plate-scale math.
//!
//! Pure functions, no I/O, no dependencies. Accuracy targets are "good enough
//! to explain a sub" (≈0.02° sun, ≈0.3° moon), not ephemeris grade. Angles
//! are degrees unless a name says otherwise.

pub mod coords;
pub mod optics;
pub mod time;
pub mod wcs;

pub use coords::{
    Equatorial, Horizontal, airmass, angular_separation, parse_dec, parse_lon, parse_ra,
    to_horizontal,
};
pub use time::{julian_day, parse_datetime};

use std::f64::consts::PI;

pub(crate) fn rad(d: f64) -> f64 {
    d * PI / 180.0
}

pub(crate) fn deg(r: f64) -> f64 {
    r * 180.0 / PI
}

/// Normalize to [0, 360).
pub(crate) fn wrap360(d: f64) -> f64 {
    d.rem_euclid(360.0)
}

/// Julian centuries since J2000.0.
fn centuries(jd: f64) -> f64 {
    (jd - 2451545.0) / 36525.0
}

/// Mean obliquity of the ecliptic.
fn obliquity(jd: f64) -> f64 {
    23.439291 - 0.0130042 * centuries(jd)
}

fn ecliptic_to_equatorial(lambda: f64, beta: f64, jd: f64) -> Equatorial {
    let (l, b, e) = (rad(lambda), rad(beta), rad(obliquity(jd)));
    let ra = (l.sin() * e.cos() - b.tan() * e.sin()).atan2(l.cos());
    let dec = (b.sin() * e.cos() + b.cos() * e.sin() * l.sin()).asin();
    Equatorial {
        ra: wrap360(deg(ra)),
        dec: deg(dec),
    }
}

/// Geocentric solar position (Meeus ch. 25, low precision).
pub fn sun(jd: f64) -> Equatorial {
    ecliptic_to_equatorial(sun_longitude(jd), 0.0, jd)
}

fn sun_longitude(jd: f64) -> f64 {
    let t = centuries(jd);
    let l0 = 280.46646 + 36000.76983 * t;
    let m = rad(357.52911 + 35999.05029 * t);
    let c = (1.914602 - 0.004817 * t) * m.sin()
        + 0.019993 * (2.0 * m).sin()
        + 0.000289 * (3.0 * m).sin();
    wrap360(l0 + c)
}

/// Geocentric lunar position (Astronomical Almanac low-precision series).
pub fn moon(jd: f64) -> Equatorial {
    let (lambda, beta) = moon_ecliptic(jd);
    ecliptic_to_equatorial(lambda, beta, jd)
}

fn moon_ecliptic(jd: f64) -> (f64, f64) {
    let t = centuries(jd);
    let s = |a: f64, b: f64| rad(a + b * t).sin();
    let lambda = 218.32 + 481267.881 * t + 6.29 * s(135.0, 477198.87) - 1.27 * s(259.3, -413335.36)
        + 0.66 * s(235.7, 890534.22)
        + 0.21 * s(269.9, 954397.74)
        - 0.19 * s(357.5, 35999.05)
        - 0.11 * s(186.5, 966404.03);
    let beta = 5.13 * s(93.3, 483202.02) + 0.28 * s(228.2, 960400.89)
        - 0.28 * s(318.3, 6003.15)
        - 0.17 * s(217.6, -407332.21);
    (wrap360(lambda), beta)
}

/// Moon phase facts at an instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoonPhase {
    /// Illuminated fraction, 0 (new) to 1 (full).
    pub illumination: f64,
    /// True between new and full.
    pub waxing: bool,
}

pub fn moon_phase(jd: f64) -> MoonPhase {
    let ls = sun_longitude(jd);
    let (lm, bm) = moon_ecliptic(jd);
    // Elongation, then phase angle ≈ 180° − elongation (moon ≪ sun distance).
    let elong = deg((rad(bm).cos() * rad(lm - ls).cos()).acos());
    let phase_angle = rad(180.0 - elong);
    MoonPhase {
        illumination: (1.0 + phase_angle.cos()) / 2.0,
        waxing: wrap360(lm - ls) < 180.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_meeus_25a() {
        // 1992 Oct 13.0 TD: RA 198.38083°, Dec −7.78507°.
        let s = sun(2448908.5);
        assert!((s.ra - 198.38083).abs() < 0.02, "{s:?}");
        assert!((s.dec - -7.78507).abs() < 0.02, "{s:?}");
    }

    #[test]
    fn moon_meeus_47a() {
        // 1992 Apr 12.0 TD: RA 134.688470°, Dec 13.768368°.
        let m = moon(2448724.5);
        assert!((m.ra - 134.688470).abs() < 0.3, "{m:?}");
        assert!((m.dec - 13.768368).abs() < 0.3, "{m:?}");
    }

    #[test]
    fn moon_phase_meeus_48a() {
        // 1992 Apr 12.0: illuminated fraction 0.6786, waxing.
        let p = moon_phase(2448724.5);
        assert!((p.illumination - 0.6786).abs() < 0.01, "{p:?}");
        assert!(p.waxing);
    }
}
