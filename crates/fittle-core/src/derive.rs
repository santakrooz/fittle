//! Derived facts: computed from canonical fields, always labelled derived.

use fittle_astro::optics::{Sampling, field_of_view, pixel_scale, sampling, wcs_scale};
use fittle_astro::{
    Equatorial, airmass, angular_separation, julian_day, moon, moon_phase, parse_datetime, sun,
    to_horizontal,
};
use serde::Serialize;

use crate::canonical::{Canonical, Fact, Note, NoteLevel, Source};
use crate::header::Header;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MoonFacts {
    /// Illuminated fraction 0–1.
    pub illumination: f64,
    pub waxing: bool,
    /// Degrees above the horizon at the site (needs site).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub altitude: Option<f64>,
    /// Degrees from the target (needs RA/Dec).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separation: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Derived {
    /// Arcsec/px from pixel size and focal length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_scale: Option<Fact<f64>>,
    /// Arcsec/px from the plate solution (WCS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wcs_scale: Option<Fact<f64>>,
    /// Arcminutes, [width, height].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fov_arcmin: Option<Fact<[f64; 2]>>,
    /// `over`, `well` or `under` for typical 2–4″ seeing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<Fact<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub altitude: Option<Fact<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub azimuth: Option<Fact<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub airmass: Option<Fact<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moon: Option<Fact<MoonFacts>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sun_altitude: Option<Fact<f64>>,
    /// `night`, `astronomical twilight`, `nautical twilight`, `civil twilight`, `day`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sky: Option<Fact<String>>,
    /// Calendar date of the evening the session started (local solar time).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_night: Option<Fact<String>>,
    /// True when the header carries a plate solution.
    pub plate_solved: bool,
}

fn d<T>(value: T, from: &[&str]) -> Fact<T> {
    Fact {
        value,
        source: Source::Derived {
            from: from.iter().map(|s| s.to_string()).collect(),
        },
    }
}

fn round(v: f64, places: i32) -> f64 {
    let p = 10f64.powi(places);
    (v * p).round() / p
}

pub fn derive(
    f: &Canonical,
    h: &Header,
    width: u64,
    height: u64,
    notes: &mut Vec<Note>,
) -> Derived {
    let mut out = Derived::default();

    // Plate scale and field.
    if let (Some(px), Some(fl)) = (&f.pixel_um, &f.focal_mm) {
        out.pixel_scale = pixel_scale(px.value, fl.value)
            .map(|s| d(round(s, 3), &["pixel size", "focal length"]));
    }
    out.wcs_scale = wcs_cd(h)
        .and_then(wcs_scale)
        .map(|s| d(round(s, 3), &["WCS"]));
    out.plate_solved = out.wcs_scale.is_some();
    if let (Some(a), Some(b)) = (&out.pixel_scale, &out.wcs_scale) {
        let ratio = a.value / b.value;
        if !(0.95..=1.05).contains(&ratio) {
            notes.push(Note {
                level: NoteLevel::Warning,
                message: format!(
                    "Pixel size and focal length give {:.2}″/px but the plate solution says {:.2}″/px; \
                     the focal length is probably wrong (plate solution implies {:.0} mm)",
                    a.value,
                    b.value,
                    f.focal_mm.as_ref().map_or(0.0, |fl| fl.value * ratio),
                ),
            });
        }
    }
    let scale = out
        .wcs_scale
        .as_ref()
        .or(out.pixel_scale.as_ref())
        .map(|s| s.value);
    if let Some(s) = scale {
        if width > 0 && height > 0 {
            let (w, hh) = field_of_view(s, width, height);
            out.fov_arcmin = Some(d(
                [round(w, 1), round(hh, 1)],
                &["pixel scale", "image size"],
            ));
        }
        let label = match sampling(s) {
            Sampling::Over => "over",
            Sampling::Well => "well",
            Sampling::Under => "under",
        };
        out.sampling = Some(d(label.to_string(), &["pixel scale"]));
    }

    // Sky position and conditions at DATE-OBS.
    let Some(t) = f.date_obs.as_ref().and_then(|t| parse_datetime(&t.value)) else {
        return out;
    };
    let jd = julian_day(&t);
    let site = f
        .site_lat
        .as_ref()
        .zip(f.site_lon.as_ref())
        .map(|(a, b)| (a.value, b.value));
    let target = f.ra.as_ref().zip(f.dec.as_ref()).map(|(r, dd)| Equatorial {
        ra: r.value,
        dec: dd.value,
    });

    if let (Some((lat, lon)), Some(tg)) = (site, target) {
        let hz = to_horizontal(tg, lat, lon, jd);
        let from = &["RA", "DEC", "site", "DATE-OBS"];
        out.altitude = Some(d(round(hz.alt, 1), from));
        out.azimuth = Some(d(round(hz.az, 1), from));
        out.airmass = airmass(hz.alt).map(|x| d(round(x, 2), from));
    }
    let phase = moon_phase(jd);
    let mpos = moon(jd);
    out.moon = Some(d(
        MoonFacts {
            illumination: round(phase.illumination, 2),
            waxing: phase.waxing,
            altitude: site.map(|(lat, lon)| round(to_horizontal(mpos, lat, lon, jd).alt, 1)),
            separation: target.map(|tg| round(angular_separation(tg, mpos), 1)),
        },
        &["DATE-OBS"],
    ));
    if let Some((lat, lon)) = site {
        let alt = to_horizontal(sun(jd), lat, lon, jd).alt;
        out.sun_altitude = Some(d(round(alt, 1), &["site", "DATE-OBS"]));
        let sky = match alt {
            a if a < -18.0 => "night",
            a if a < -12.0 => "astronomical twilight",
            a if a < -6.0 => "nautical twilight",
            a if a < 0.0 => "civil twilight",
            _ => "day",
        };
        out.sky = Some(d(sky.to_string(), &["site", "DATE-OBS"]));
        // Local mean solar time minus 12 h gives the evening's date.
        let shifted = jd + lon / 360.0 - 0.5;
        out.session_night = Some(d(calendar_date(shifted), &["site longitude", "DATE-OBS"]));
    }
    out
}

/// CD matrix from `CDi_j`, or from `CDELTn` (+ `CROTA2`).
fn wcs_cd(h: &Header) -> Option<[[f64; 2]; 2]> {
    if let (Some(a), Some(dd)) = (h.float("CD1_1"), h.float("CD2_2")) {
        return Some([
            [a, h.float("CD1_2").unwrap_or(0.0)],
            [h.float("CD2_1").unwrap_or(0.0), dd],
        ]);
    }
    let (c1, c2) = (h.float("CDELT1")?, h.float("CDELT2")?);
    h.string("CTYPE1")?;
    let rot = h.float("CROTA2").unwrap_or(0.0).to_radians();
    Some([
        [c1 * rot.cos(), -c2 * rot.sin()],
        [c1 * rot.sin(), c2 * rot.cos()],
    ])
}

/// Gregorian date (YYYY-MM-DD) of a Julian day (Meeus ch. 7).
fn calendar_date(jd: f64) -> String {
    let z = (jd + 0.5).floor();
    let a = if z < 2299161.0 {
        z
    } else {
        let alpha = ((z - 1867216.25) / 36524.25).floor();
        z + 1.0 + alpha - (alpha / 4.0).floor()
    };
    let b = a + 1524.0;
    let c = ((b - 122.1) / 365.25).floor();
    let dd = (365.25 * c).floor();
    let e = ((b - dd) / 30.6001).floor();
    let day = b - dd - (30.6001 * e).floor();
    let month = if e < 14.0 { e - 1.0 } else { e - 13.0 };
    let year = if month > 2.0 { c - 4716.0 } else { c - 4715.0 };
    format!("{:04}-{:02}-{:02}", year as i64, month as i64, day as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar() {
        assert_eq!(calendar_date(2436116.31), "1957-10-04");
        assert_eq!(calendar_date(2451544.5), "2000-01-01");
    }
}
