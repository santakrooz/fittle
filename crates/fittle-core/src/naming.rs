//! File-name templates: `{object}_{filter}_{exptime}s_{date}_{seq}`.
//! Shared by export (M4) and rename/organize (M6).

use crate::info::Info;

/// Tokens a template may use, with a one-line description each.
pub const TOKENS: &[(&str, &str)] = &[
    ("name", "source file name without extension"),
    ("object", "target (OBJECT), spaces kept"),
    ("filter", "filter name"),
    ("exptime", "sub exposure in seconds"),
    ("integration", "total integration, e.g. 18h57m"),
    ("stack", "number of stacked frames"),
    ("date", "observation date (UTC), YYYY-MM-DD"),
    ("night", "session night, YYYY-MM-DD"),
    ("time", "observation time (UTC), HHMMSS"),
    ("gain", "gain setting"),
    ("camera", "camera (INSTRUME)"),
    ("scope", "scope or telescope"),
    ("frame", "frame kind: light, dark, flat, bias"),
    ("seq", "position in a batch, 0001…"),
];

fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn compact_duration(s: f64) -> String {
    let t = s.round() as i64;
    match (t / 3600, (t % 3600) / 60) {
        (0, 0) => format!("{t}s"),
        (0, m) => format!("{m}m"),
        (h, m) => format!("{h}h{m:02}m"),
    }
}

/// Value of one token for a file (`None` when the file lacks it).
pub fn token(info: &Info, key: &str, seq: usize) -> Option<String> {
    let f = &info.fields;
    Some(match key {
        "name" => {
            let base = info.path.rsplit(['/', '\\']).next().unwrap_or(&info.path);
            base.split_once(".f")
                .map_or(base, |(stem, _)| stem)
                .to_string()
        }
        "object" => f.object.as_ref()?.value.clone(),
        "filter" => f.filter.as_ref()?.value.clone(),
        "exptime" => num(f.exposure_s.as_ref()?.value),
        "integration" => compact_duration(
            f.total_integration_s
                .as_ref()
                .or(f.exposure_s.as_ref())?
                .value,
        ),
        "stack" => f.stack_count.as_ref()?.value.to_string(),
        "date" => f.date_obs.as_ref()?.value.get(..10)?.to_string(),
        "night" => info.derived.session_night.as_ref()?.value.clone(),
        "time" => f.date_obs.as_ref()?.value.get(11..19)?.replace(':', ""),
        "gain" => num(f.gain.as_ref()?.value),
        "camera" => f.camera.as_ref()?.value.clone(),
        "scope" => info
            .origin
            .scope
            .as_ref()
            .map(|s| s.name.clone())
            .or_else(|| f.telescope.as_ref().map(|t| t.value.clone()))?,
        "frame" => format!("{:?}", info.verdict.frame).to_lowercase(),
        "seq" => format!("{:04}", seq),
        _ => return None,
    })
}

/// Characters that are unsafe in file names on some platform.
fn safe(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Render a template. Unknown or missing tokens become `unknown` so a name is
/// always produced; the list of those tokens is returned for display.
pub fn render(template: &str, info: &Info, seq: usize) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut missing = Vec::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('}') {
            Some(j) => {
                let key = &after[..j];
                match token(info, key, seq) {
                    Some(v) => out.push_str(&safe(&v)),
                    None => {
                        out.push_str("unknown");
                        missing.push(key.to_string());
                    }
                }
                rest = &after[j + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    (safe(&out), missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn info(rel: &str) -> Info {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/synthetic")
            .join(rel);
        crate::info(&p).unwrap()
    }

    #[test]
    fn renders_seestar_sub() {
        let i = info("seestar/Light_NGC 6995_20.0s_LP_20260924-213412.fit");
        let (s, missing) = render("{object}_{filter}_{exptime}s_{date}_{seq}", &i, 7);
        assert_eq!(s, "NGC 6995_LP_20s_2026-09-25_0007");
        assert!(missing.is_empty());
        assert_eq!(
            render("{name}", &i, 1).0,
            "Light_NGC 6995_20.0s_LP_20260924-213412"
        );
    }

    #[test]
    fn integration_and_missing() {
        let i = info("siril/r_pp_NGC6995_stacked.fit");
        assert_eq!(render("{object}_{integration}", &i, 1).0, "NGC 6995_1h10m");
        let (s, missing) = render("{object}_{gain}_{nope}", &i, 1);
        assert_eq!(s, "NGC 6995_unknown_unknown");
        assert_eq!(missing, ["gain", "nope"]);
        assert_eq!(safe("a/b:c"), "a_b_c");
    }
}
