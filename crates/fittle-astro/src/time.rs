//! Time: ISO-8601 parsing, Julian day, sidereal time.

use crate::wrap360;

/// A calendar instant in UTC (no time-zone handling; FITS DATE-OBS is UTC).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: f64,
}

/// Parse `YYYY-MM-DD[THH:MM[:SS[.fff]]]` (also accepts a space or trailing Z).
pub fn parse_datetime(s: &str) -> Option<DateTime> {
    let s = s.trim().trim_end_matches('Z');
    // Legacy FITS forms: `DD/MM/YY` (years 1900–1999) and `YYYY/MM/DD`.
    if let [a, b, c] = s.split('/').map(str::trim).collect::<Vec<_>>()[..] {
        let (year, month, day) = if a.len() == 4 {
            (a.parse().ok()?, b.parse().ok()?, c.parse().ok()?)
        } else {
            (
                1900 + c.parse::<i32>().ok()?,
                b.parse().ok()?,
                a.parse().ok()?,
            )
        };
        let ok = (1..=12).contains(&month) && (1..=31).contains(&day);
        return ok.then_some(DateTime {
            year,
            month,
            day,
            hour: 0,
            minute: 0,
            second: 0.0,
        });
    }
    let (date, time) = match s.split_once(['T', ' ']) {
        Some((d, t)) => (d, t),
        None => (s, "00:00:00"),
    };
    let mut d = date.split('-');
    let year = d.next()?.parse().ok()?;
    let month = d.next()?.parse().ok()?;
    let day = d.next()?.parse().ok()?;
    let mut t = time.split(':');
    let hour = t.next()?.parse().ok()?;
    let minute = t.next().map_or(Some(0), |m| m.parse().ok())?;
    let second = t.next().map_or(Some(0.0), |x| x.parse().ok())?;
    let ok = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second < 61.0;
    ok.then_some(DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
    })
}

/// Julian day (Meeus ch. 7), Gregorian calendar.
pub fn julian_day(t: &DateTime) -> f64 {
    let (mut y, mut m) = (t.year as f64, t.month as f64);
    if m <= 2.0 {
        y -= 1.0;
        m += 12.0;
    }
    let a = (y / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();
    let frac = (t.hour as f64 + t.minute as f64 / 60.0 + t.second / 3600.0) / 24.0;
    (365.25 * (y + 4716.0)).floor() + (30.6001 * (m + 1.0)).floor() + t.day as f64 + b - 1524.5
        + frac
}

/// Greenwich mean sidereal time in degrees (Meeus 12.4).
pub fn gmst(jd: f64) -> f64 {
    let t = (jd - 2451545.0) / 36525.0;
    wrap360(
        280.46061837 + 360.98564736629 * (jd - 2451545.0) + 0.000387933 * t * t
            - t * t * t / 38710000.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        let t = parse_datetime("2026-09-03T04:26:24.385679").unwrap();
        assert_eq!(
            (t.year, t.month, t.day, t.hour, t.minute),
            (2026, 9, 3, 4, 26)
        );
        assert!((t.second - 24.385679).abs() < 1e-9);
        assert!(parse_datetime("2026-09-03").is_some());
        assert!(parse_datetime("not a date").is_none());
        assert!(parse_datetime("2026-13-01T00:00:00").is_none());
        let t = parse_datetime(" 1/04/95").unwrap();
        assert_eq!((t.year, t.month, t.day), (1995, 4, 1));
        let t = parse_datetime("1997/01/10").unwrap();
        assert_eq!((t.year, t.month, t.day), (1997, 1, 10));
    }

    #[test]
    fn jd_meeus_7a() {
        // 1957 Oct 4.81 = JD 2436116.31
        let t = DateTime {
            year: 1957,
            month: 10,
            day: 4,
            hour: 19,
            minute: 26,
            second: 24.0,
        };
        assert!((julian_day(&t) - 2436116.31).abs() < 1e-6);
    }

    #[test]
    fn gmst_meeus_12a() {
        // 1987 Apr 10 0h UT: 13h10m46.3668s = 197.693195°
        assert!((gmst(2446895.5) - 197.693195).abs() < 1e-4);
    }
}
