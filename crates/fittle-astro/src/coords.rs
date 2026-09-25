//! Equatorial ↔ horizontal coordinates, separations, sexagesimal parsing.

use crate::{deg, rad, time::gmst, wrap360};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Equatorial {
    /// Right ascension, degrees.
    pub ra: f64,
    /// Declination, degrees.
    pub dec: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Horizontal {
    /// Altitude above the horizon, degrees.
    pub alt: f64,
    /// Azimuth from north through east, degrees.
    pub az: f64,
}

/// Position of `target` for an observer at latitude `lat`, longitude `lon`
/// (east positive) at Julian day `jd` (UT). No refraction.
pub fn to_horizontal(target: Equatorial, lat: f64, lon: f64, jd: f64) -> Horizontal {
    let ha = rad(wrap360(gmst(jd) + lon - target.ra));
    let (phi, dec) = (rad(lat), rad(target.dec));
    let alt = (phi.sin() * dec.sin() + phi.cos() * dec.cos() * ha.cos()).asin();
    let az =
        (-dec.cos() * ha.sin()).atan2(dec.sin() * phi.cos() - dec.cos() * ha.cos() * phi.sin());
    Horizontal {
        alt: deg(alt),
        az: wrap360(deg(az)),
    }
}

/// Relative airmass (Kasten & Young 1989). `None` below the horizon.
pub fn airmass(alt: f64) -> Option<f64> {
    (alt > 0.0).then(|| 1.0 / (rad(alt).sin() + 0.50572 * (alt + 6.07995).powf(-1.6364)))
}

/// Great-circle separation in degrees (haversine; stable for small angles).
pub fn angular_separation(a: Equatorial, b: Equatorial) -> f64 {
    let (d1, d2) = (rad(a.dec), rad(b.dec));
    let dra = rad(a.ra - b.ra);
    let h = ((d2 - d1) / 2.0).sin().powi(2) + d1.cos() * d2.cos() * (dra / 2.0).sin().powi(2);
    deg(2.0 * h.sqrt().min(1.0).asin())
}

/// Parse right ascension. Numbers are degrees; sexagesimal strings
/// (`"20 57 10.8"`, `"20:57:10.8"`, `"20h57m10.8s"`) are hours.
pub fn parse_ra(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Ok(v) = s.parse::<f64>() {
        return (0.0..360.0).contains(&v).then_some(v);
    }
    let h = sexagesimal(s)?;
    (0.0..24.0).contains(&h).then_some(h * 15.0)
}

/// Parse declination in degrees, decimal or sexagesimal (`"+31 14 07"`).
pub fn parse_dec(s: &str) -> Option<f64> {
    let s = s.trim();
    let v = s.parse::<f64>().ok().or_else(|| sexagesimal(s))?;
    (-90.0..=90.0).contains(&v).then_some(v)
}

/// Parse a longitude in degrees, decimal or sexagesimal (`"-105 16 00"`).
pub fn parse_lon(s: &str) -> Option<f64> {
    let s = s.trim();
    let v = s.parse::<f64>().ok().or_else(|| sexagesimal(s))?;
    (-180.0..=360.0).contains(&v).then_some(v)
}

fn sexagesimal(s: &str) -> Option<f64> {
    let neg = s.starts_with('-');
    let body = s.trim_start_matches(['+', '-']);
    let parts: Vec<f64> = body
        .split(|c: char| {
            c.is_whitespace() || matches!(c, ':' | 'h' | 'm' | 's' | 'd' | '°' | '\'' | '"')
        })
        .filter(|p| !p.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let v = parts
        .iter()
        .zip([1.0, 60.0, 3600.0])
        .map(|(p, d)| p / d)
        .sum::<f64>();
    Some(if neg { -v } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_meeus_13b() {
        // Venus from USNO, 1987 Apr 10 19:21:00 UT: h = 15.1249°, A = 68.0337°
        // (Meeus azimuth is from south; ours is from north). Meeus uses apparent
        // sidereal time; mean time differs by nutation, so allow 0.01°.
        let jd = 2446896.30625;
        let venus = Equatorial {
            ra: 347.3193375,
            dec: -6.719892,
        };
        let lat = 38.0 + 55.0 / 60.0 + 17.0 / 3600.0;
        let lon = -(77.0 + 3.0 / 60.0 + 56.0 / 3600.0);
        let h = to_horizontal(venus, lat, lon, jd);
        assert!((h.alt - 15.1249).abs() < 0.01, "{h:?}");
        assert!((h.az - (68.0337 + 180.0)).abs() < 0.01, "{h:?}");
    }

    #[test]
    fn airmass_values() {
        assert!((airmass(90.0).unwrap() - 1.0).abs() < 1e-3);
        assert!((airmass(30.0).unwrap() - 1.995).abs() < 0.01);
        assert!(airmass(-1.0).is_none());
    }

    #[test]
    fn separation() {
        let a = Equatorial {
            ra: 10.0,
            dec: 20.0,
        };
        assert!(angular_separation(a, a) < 1e-9);
        let b = Equatorial {
            ra: 10.0,
            dec: 21.0,
        };
        assert!((angular_separation(a, b) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn sexagesimal_parsing() {
        assert!((parse_ra("20 57 10.8").unwrap() - 314.295).abs() < 1e-6);
        assert!((parse_ra("20h57m10.8s").unwrap() - 314.295).abs() < 1e-6);
        assert_eq!(parse_ra("342.11667"), Some(342.11667));
        assert!((parse_dec("+31 14 07").unwrap() - 31.235278).abs() < 1e-5);
        assert!((parse_dec("-05 23 28").unwrap() - -5.391111).abs() < 1e-5);
        assert!(parse_ra("25 00 00").is_none());
        assert!(parse_dec("junk").is_none());
        assert!((parse_lon("-105 16 00").unwrap() - -105.266667).abs() < 1e-5);
    }
}
