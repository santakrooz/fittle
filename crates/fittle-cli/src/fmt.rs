//! Human formatting shared by commands. Colour only when stdout supports it
//! (respects NO_COLOR / non-TTY).

use owo_colors::{OwoColorize, Stream::Stdout};

pub fn muted(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.bright_black())
        .to_string()
}
pub fn good(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.green()).to_string()
}
pub fn derived(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.cyan()).to_string()
}
pub fn warn(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.yellow()).to_string()
}
pub fn bad(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.red()).to_string()
}
pub fn bold(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.bold()).to_string()
}

/// `20h57m10.8s`
pub fn ra_hms(deg: f64) -> String {
    let h = deg / 15.0;
    let (hh, rem) = (h.floor(), (h - h.floor()) * 60.0);
    let (mm, ss) = (rem.floor(), (rem - rem.floor()) * 60.0);
    let (mut hh, mut mm, mut ss) = (hh as i64, mm as i64, (ss * 10.0).round() / 10.0);
    if ss >= 60.0 {
        ss -= 60.0;
        mm += 1;
    }
    if mm >= 60 {
        mm -= 60;
        hh = (hh + 1) % 24;
    }
    format!("{hh:02}h{mm:02}m{ss:04.1}s")
}

/// `+31°14′07″`
pub fn dec_dms(deg: f64) -> String {
    let sign = if deg < 0.0 { '−' } else { '+' };
    let a = deg.abs();
    let total = (a * 3600.0).round() as i64;
    format!(
        "{sign}{:02}°{:02}′{:02}″",
        total / 3600,
        (total / 60) % 60,
        total % 60
    )
}

/// `4 h 12 m`, `50 m`, `20 s`
pub fn duration(s: f64) -> String {
    let t = s.round() as i64;
    match (t / 3600, (t % 3600) / 60, t % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, _) => format!("{m} m"),
        (h, m, _) => format!("{h} h {m:02} m"),
    }
}

/// Trim trailing zeros: 250.0 → `250`, 2.90000009 → `2.9`.
pub fn num(v: f64, places: usize) -> String {
    let s = format!("{v:.places$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(ra_hms(314.295), "20h57m10.8s");
        assert_eq!(dec_dms(31.235278), "+31°14′07″");
        assert_eq!(dec_dms(-5.391111), "−05°23′28″");
        assert_eq!(duration(15900.0), "4 h 25 m");
        assert_eq!(duration(20.0), "20 s");
        assert_eq!(num(2.90000009536743, 2), "2.9");
        assert_eq!(num(250.0, 1), "250");
    }
}
