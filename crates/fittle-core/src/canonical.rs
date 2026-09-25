//! Canonical, vendor-agnostic fields. Every value carries its source (a
//! keyword, the scope profile, the file name, or a derivation) so the UI can
//! show evidence and mark defaults and derived values as such.

use serde::Serialize;

use crate::card::Value;
use crate::filename::FileNameFacts;
use crate::header::Header;
use crate::vendor::{Quirk, ScopeProfile, pattern_matches};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Keyword {
        key: String,
    },
    /// Hardware default from the matched scope profile.
    ModelDefault {
        model: String,
    },
    Filename,
    Derived {
        from: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fact<T> {
    pub value: T,
    pub source: Source,
}

impl<T> Fact<T> {
    fn key(value: T, key: &str) -> Self {
        Fact {
            value,
            source: Source::Keyword {
                key: key.to_string(),
            },
        }
    }
    fn model(value: T, model: &str) -> Self {
        Fact {
            value,
            source: Source::ModelDefault {
                model: model.to_string(),
            },
        }
    }
    fn derived(value: T, from: &[&str]) -> Self {
        Fact {
            value,
            source: Source::Derived {
                from: from.iter().map(|s| s.to_string()).collect(),
            },
        }
    }
    fn filename(value: T) -> Self {
        Fact {
            value,
            source: Source::Filename,
        }
    }
    /// `KEY=value`, `file name: value`, or `value (derived)` for evidence lists.
    pub fn describe(&self, value: impl std::fmt::Display) -> String {
        match &self.source {
            Source::Keyword { key } => format!("{key}={value}"),
            Source::Filename => format!("file name: {value}"),
            Source::ModelDefault { model } => format!("{value} ({model} default)"),
            Source::Derived { from } => format!("{value} (derived from {})", from.join(", ")),
        }
    }

    /// Keyword this value came from, if any.
    pub fn keyword(&self) -> Option<&str> {
        match &self.source {
            Source::Keyword { key } => Some(key),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteLevel {
    Info,
    Warning,
}

/// A plain-language remark about how the header was read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Note {
    pub level: NoteLevel,
    pub message: String,
}

/// A keyword holding a device serial number (relevant to the privacy scrub).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Serial {
    pub key: String,
    pub value: String,
}

macro_rules! fields {
    ($($(#[$m:meta])* $name:ident: $ty:ty),* $(,)?) => {
        #[derive(Debug, Clone, Default, PartialEq, Serialize)]
        pub struct Canonical {
            $($(#[$m])* #[serde(skip_serializing_if = "Option::is_none")] pub $name: Option<Fact<$ty>>,)*
            #[serde(skip_serializing_if = "Vec::is_empty")]
            pub serials: Vec<Serial>,
        }
    };
}

fields! {
    // Target
    object: String,
    /// Degrees.
    ra: f64,
    /// Degrees.
    dec: f64,
    // Optics
    telescope: String,
    focal_mm: f64,
    aperture_mm: f64,
    focal_ratio: f64,
    // Camera
    camera: String,
    sensor: String,
    /// Binned pixel size.
    pixel_um: f64,
    binning: i64,
    gain: f64,
    offset: f64,
    egain: f64,
    sensor_temp_c: f64,
    set_temp_c: f64,
    bayer: String,
    row_order: String,
    // Filter / exposure
    filter: String,
    frame_type: String,
    exposure_s: f64,
    stack_count: i64,
    total_integration_s: f64,
    // Time
    date_obs: String,
    date_local: String,
    // Site
    site_lat: f64,
    site_lon: f64,
    site_elev_m: f64,
    // Mount / guiding
    mount: String,
    pier_side: String,
    focus_pos: i64,
    focus_temp_c: f64,
}

/// Standard keyword candidates per field, most authoritative first. Vendor
/// quirks (data/apps.json) prepend their own keys.
const KEYS: &[(&str, &[&str])] = &[
    ("object", &["OBJECT", "OBJNAME"]),
    ("telescope", &["TELESCOP"]),
    ("focal_mm", &["FOCALLEN", "FOCAL"]),
    ("aperture_mm", &["APTDIA", "APERTURE"]),
    ("focal_ratio", &["FOCRATIO"]),
    ("camera", &["INSTRUME"]),
    ("pixel_um", &["XPIXSZ", "PIXSIZE1", "PIXSZ"]),
    ("binning", &["XBINNING", "CCDXBIN", "BINNING"]),
    ("gain", &["GAIN"]),
    ("offset", &["OFFSET", "BLKLEVEL"]),
    ("egain", &["EGAIN"]),
    (
        "sensor_temp_c",
        &["CCD-TEMP", "CCD_TEMP", "SENSTEMP", "TEMPERAT"],
    ),
    ("set_temp_c", &["SET-TEMP", "SET_TEMP"]),
    ("bayer", &["BAYERPAT", "COLORTYP"]),
    ("row_order", &["ROWORDER"]),
    ("filter", &["FILTER", "FILTNAME"]),
    ("frame_type", &["IMAGETYP", "FRAMETYP", "FRAME"]),
    ("exposure_s", &["EXPTIME", "EXPOSURE", "EXP_TIME"]),
    ("stack_count", &["STACKCNT", "NCOMBINE", "NSTACK"]),
    ("total_integration_s", &["LIVETIME", "TOTALEXP"]),
    ("date_obs", &["DATE-OBS", "DATE-BEG"]),
    ("date_local", &["DATE-LOC"]),
    ("site_lat", &["SITELAT", "OBSLAT", "LAT-OBS"]),
    ("site_lon", &["SITELONG", "OBSLONG", "LONG-OBS"]),
    (
        "site_elev_m",
        &["SITEELEV", "OBSGEO-H", "ALT-OBS", "OBSALT"],
    ),
    ("mount", &["MOUNT"]),
    ("pier_side", &["PIERSIDE"]),
    ("focus_pos", &["FOCPOS", "FOCUSPOS"]),
    ("focus_temp_c", &["FOCTEMP", "FOCUSTEM"]),
];

/// Inputs for reading one file's canonical fields.
pub struct Context<'a> {
    pub header: &'a Header,
    /// Image planes (3 = already-debayered RGB).
    pub planes: u64,
    pub profile: Option<&'a ScopeProfile>,
    pub quirks: &'a [&'a Quirk],
    pub file: &'a FileNameFacts,
}

struct Reader<'a> {
    ctx: &'a Context<'a>,
    ignored: Vec<&'a str>,
}

impl<'a> Reader<'a> {
    /// Standard keywords first, then vendor keys: a standard key written by
    /// later software (e.g. Siril's LIVETIME) beats a stale vendor one.
    fn keys(&self, field: &str) -> Vec<&'a str> {
        let mut out: Vec<&'a str> = Vec::new();
        if let Some((_, k)) = KEYS.iter().find(|(f, _)| *f == field) {
            out.extend(k.iter().copied());
        }
        for q in self.ctx.quirks {
            if let Some(k) = q.keys.get(field) {
                out.extend(
                    k.iter()
                        .map(String::as_str)
                        .filter(|k| !out.contains(k))
                        .collect::<Vec<_>>(),
                );
            }
        }
        out.retain(|k| !self.ignored.contains(k));
        out
    }

    fn string(&self, field: &str) -> Option<Fact<String>> {
        self.keys(field).into_iter().find_map(|k| {
            let v = self.ctx.header.string(k)?.trim();
            let v = self
                .ctx
                .quirks
                .iter()
                .find_map(|q| {
                    q.aliases
                        .get(k)
                        .and_then(|m| m.iter().find(|(from, _)| from.eq_ignore_ascii_case(v)))
                })
                .map_or(v, |(_, to)| to.as_str());
            (!v.is_empty()).then(|| Fact::key(v.to_string(), k))
        })
    }

    fn factor(&self, key: &str) -> f64 {
        self.ctx
            .quirks
            .iter()
            .flat_map(|q| &q.scale)
            .find(|s| s.key == key)
            .map_or(1.0, |s| s.factor)
    }

    /// Numbers; non-positive values count as missing for `positive` fields.
    fn number(&self, field: &str, positive: bool) -> Option<Fact<f64>> {
        self.keys(field).into_iter().find_map(|k| {
            let v = match self.ctx.header.value(k)? {
                Value::String(s) => s.trim().parse().ok()?,
                v => v.as_f64()?,
            };
            let v = v * self.factor(k);
            (!positive || v > 0.0).then(|| Fact::key(v, k))
        })
    }

    fn int(&self, field: &str) -> Option<Fact<i64>> {
        self.number(field, true).map(|f| Fact {
            value: f.value.round() as i64,
            source: f.source,
        })
    }
}

/// Read canonical fields, applying vendor quirks and profile defaults.
pub fn read(ctx: &Context) -> (Canonical, Vec<Note>) {
    let h = ctx.header;
    let mut notes = Vec::new();
    let mut c = Canonical::default();
    let ignored: Vec<&str> = ctx
        .quirks
        .iter()
        .flat_map(|q| q.ignore.iter())
        .map(|r| r.key.as_str())
        .collect();
    for q in ctx.quirks {
        for sc in q.scale.iter().filter(|sc| h.get(&sc.key).is_some()) {
            notes.push(Note {
                level: NoteLevel::Info,
                message: format!("{} × {}: {}", sc.key, sc.factor, sc.reason),
            });
        }
        for r in &q.ignore {
            if h.get(&r.key).is_some() {
                notes.push(Note {
                    level: NoteLevel::Info,
                    message: format!("{} ignored: {}", r.key, r.reason),
                });
            }
        }
    }
    let r = Reader { ctx, ignored };
    let model = ctx.profile.map(|p| p.display.as_str());

    // Serials and mount-in-TELESCOP quirks decide what TELESCOP means.
    let mut telescop_is_not_scope = false;
    for q in ctx.quirks {
        for s in &q.serial {
            if let Some(v) = h.string(&s.key) {
                c.serials.push(Serial {
                    key: s.key.clone(),
                    value: v.trim().to_string(),
                });
                telescop_is_not_scope |= s.key == "TELESCOP";
            }
        }
        for m in &q.mount_in {
            if let Some(v) = h
                .string(&m.key)
                .filter(|v| m.patterns.iter().any(|p| pattern_matches(p, v)))
            {
                c.mount = Some(Fact::key(v.trim().to_string(), &m.key));
                telescop_is_not_scope |= m.key == "TELESCOP";
                notes.push(Note {
                    level: NoteLevel::Info,
                    message: format!("{}='{}': {}", m.key, v.trim(), m.reason),
                });
            }
        }
    }

    for q in ctx.quirks {
        notes.extend(q.notes.iter().map(|n| Note {
            level: NoteLevel::Info,
            message: n.clone(),
        }));
    }
    c.object = r.string("object");
    (c.ra, c.dec) = coordinates(h);
    // Vendor coordinate keys (degrees) when the standard ones are absent.
    c.ra = c.ra.take().or_else(|| {
        r.number("ra", false)
            .filter(|f| (0.0..360.0).contains(&f.value))
    });
    c.dec = c.dec.take().or_else(|| {
        r.number("dec", false)
            .filter(|f| (-90.0..=90.0).contains(&f.value))
    });
    c.telescope = if telescop_is_not_scope {
        None
    } else {
        r.string("telescope")
    };
    if c.telescope.is_none() {
        c.telescope = model.map(|m| Fact::model(m.to_string(), m));
    }

    c.focal_mm = r.number("focal_mm", true);
    c.aperture_mm = r.number("aperture_mm", true);
    c.pixel_um = r.number("pixel_um", true);
    if let Some(p) = ctx.profile {
        let m = &p.display;
        let raw_focal = h.float("FOCALLEN");
        fill(
            &mut c.focal_mm,
            p.focal_length_mm,
            m,
            "FOCALLEN",
            raw_focal,
            "mm",
            &mut notes,
        );
        fill(
            &mut c.aperture_mm,
            p.aperture_mm,
            m,
            "APERTURE",
            None,
            "mm",
            &mut notes,
        );
        fill(
            &mut c.pixel_um,
            p.pixel_size_um,
            m,
            "XPIXSZ",
            None,
            "µm",
            &mut notes,
        );
        if let Some(s) = &p.sensor {
            c.sensor = Some(Fact::model(s.clone(), m));
        }
    }
    c.focal_ratio = r
        .number("focal_ratio", true)
        .or_else(|| match (&c.focal_mm, &c.aperture_mm) {
            (Some(f), Some(a)) => Some(Fact::derived(
                f.value / a.value,
                &["focal length", "aperture"],
            )),
            _ => None,
        });

    c.camera = r.string("camera");
    c.binning = r
        .int("binning")
        .or_else(|| ctx.file.binning.map(Fact::filename));
    c.gain = r
        .number("gain", false)
        .or_else(|| ctx.file.gain.map(|g| Fact::filename(g as f64)));
    c.offset = r.number("offset", false);
    c.egain = r.number("egain", true);
    c.sensor_temp_c = r
        .number("sensor_temp_c", false)
        .or_else(|| ctx.file.temp_c.map(Fact::filename));
    c.set_temp_c = r.number("set_temp_c", false);
    c.bayer = r.string("bayer");
    if let Some(b) = c.bayer.as_ref().filter(|_| ctx.planes == 3) {
        notes.push(Note {
            level: NoteLevel::Info,
            message: format!(
                "{}={} ignored: the image is already debayered RGB",
                b.keyword().unwrap_or("BAYERPAT"),
                b.value
            ),
        });
        c.bayer = None;
    }
    c.row_order = r.string("row_order").or_else(|| {
        ctx.quirks
            .iter()
            .find_map(|q| q.row_order.clone())
            .map(|v| Fact {
                value: v,
                source: Source::ModelDefault {
                    model: model.unwrap_or("capture app").to_string(),
                },
            })
    });

    c.filter = r
        .string("filter")
        .or_else(|| ctx.file.filter.clone().map(Fact::filename));
    c.frame_type = r
        .string("frame_type")
        .or_else(|| ctx.file.prefix.clone().map(Fact::filename));
    c.exposure_s = r
        .number("exposure_s", false)
        .or_else(|| ctx.file.exposure_s.map(Fact::filename));
    c.stack_count = r
        .int("stack_count")
        .or_else(|| history_count(h, ctx.quirks))
        .or_else(|| ctx.file.frames.map(Fact::filename));
    c.total_integration_s = r.number("total_integration_s", true);
    // Stackers that write the total into EXPTIME (data/apps.json).
    let stacked_name = matches!(
        ctx.file.prefix.as_deref(),
        Some("stacked" | "dso_stacked" | "result" | "integration")
    );
    let exptime_total = ctx
        .quirks
        .iter()
        .filter_map(|q| q.exptime_is_total.as_deref())
        .any(|w| w == "always" || (w == "stacked_file" && stacked_name));
    if exptime_total {
        if let Some(e) = c.exposure_s.take().filter(|e| {
            e.keyword()
                .is_some_and(|k| k == "EXPTIME" || k == "EXPOSURE")
        }) {
            notes.push(Note {
                level: NoteLevel::Info,
                message: format!(
                    "{}={} is the total integration for this software",
                    e.keyword().unwrap(),
                    e.value
                ),
            });
            // A vendor per-sub key (e.g. ASTAP LUM_EXP), else the file name, else total ÷ count.
            let per_sub = ctx
                .quirks
                .iter()
                .flat_map(|q| q.keys.get("exposure_s").into_iter().flatten())
                .find_map(|k| h.float(k).map(|v| Fact::key(v, k)));
            c.exposure_s = per_sub
                .or_else(|| ctx.file.exposure_s.map(Fact::filename))
                .or_else(|| {
                    c.stack_count.as_ref().filter(|n| n.value > 1).map(|n| {
                        Fact::derived(e.value / n.value as f64, &["EXPTIME", "stack count"])
                    })
                });
            if c.stack_count.is_none() {
                if let Some(sub) = c.exposure_s.as_ref().filter(|s| s.value > 0.0) {
                    let n = (e.value / sub.value).round();
                    if n >= 2.0 && (n * sub.value - e.value).abs() <= 0.01 * e.value {
                        c.stack_count = Some(Fact::derived(n as i64, &["EXPTIME", "sub exposure"]));
                    }
                }
            }
            if c.total_integration_s.is_none() {
                c.total_integration_s = Some(e);
            }
        }
    }
    // Some stackers write the total into EXPTIME. Detect it when the file
    // name gives the sub length (`M31_302x10s_…`) and the numbers agree.
    if let (Some(n), Some(e), Some(sub)) = (&c.stack_count, &c.exposure_s, ctx.file.exposure_s) {
        let total = n.value as f64 * sub;
        if n.value > 1
            && e.keyword().is_some()
            && sub > 0.0
            && (e.value - total).abs() <= 0.01 * total
        {
            notes.push(Note {
                level: NoteLevel::Info,
                message: format!(
                    "{}={} is the total integration ({} × {} s from the file name)",
                    e.keyword().unwrap(),
                    e.value,
                    n.value,
                    sub
                ),
            });
            if c.total_integration_s.is_none() {
                c.total_integration_s = Some(e.clone());
            }
            c.exposure_s = Some(Fact::filename(sub));
        }
    }
    if let (Some(n), Some(e)) = (&c.stack_count, &c.exposure_s) {
        let expected = n.value as f64 * e.value;
        if let Some(t) = c
            .total_integration_s
            .as_ref()
            .filter(|t| n.value > 1 && t.value < 0.5 * expected)
        {
            notes.push(Note {
                level: NoteLevel::Warning,
                message: format!(
                    "{}={} contradicts {} × {} s; using {} × exposure",
                    t.keyword().unwrap_or("total"),
                    t.value,
                    n.value,
                    e.value,
                    n.keyword().unwrap_or("frames"),
                ),
            });
            c.total_integration_s = None;
        }
        if c.total_integration_s.is_none() && n.value > 1 {
            c.total_integration_s = Some(Fact::derived(expected, &["stack count", "exposure"]));
        }
    }

    c.date_obs = r.string("date_obs").map(|d| with_time_of_day(h, d));
    c.date_local = r.string("date_local");
    c.site_lat = r
        .number("site_lat", false)
        .or_else(|| sexagesimal(h, &r.keys("site_lat")));
    c.site_lon = r.number("site_lon", false).or_else(|| {
        r.keys("site_lon").iter().find_map(|k| {
            h.string(k)
                .and_then(fittle_astro::parse_lon)
                .map(|v| Fact::key(v, k))
        })
    });
    c.site_elev_m = r.number("site_elev_m", false);
    if c.mount.is_none() {
        c.mount = r.string("mount");
    }
    c.pier_side = r.string("pier_side");
    c.focus_pos = r.int("focus_pos");
    c.focus_temp_c = r.number("focus_temp_c", false);
    (c, notes)
}

/// Old headers split date and time (`DATE-OBS='1997/01/10'` + `TIME-OBS`/`UT`).
/// Normalize to ISO `YYYY-MM-DDTHH:MM:SS` when both parts are available.
fn with_time_of_day(h: &Header, date: Fact<String>) -> Fact<String> {
    let Some(t) = fittle_astro::parse_datetime(&date.value) else {
        return date;
    };
    let has_time = date.value.contains(['T', ' ', ':']);
    let tod = if has_time {
        None
    } else {
        ["TIME-OBS", "UT", "UTSTART", "UT-START"]
            .iter()
            .find_map(|k| Some((*k, h.string(k)?.trim().to_string())))
    };
    let (hh, mm, ss, extra) = match tod.as_ref().and_then(|(k, v)| hms(v).map(|x| (k, x))) {
        Some((k, (hh, mm, ss))) => (hh, mm, ss, Some(*k)),
        None => (t.hour, t.minute, t.second, None),
    };
    let iso = format!(
        "{:04}-{:02}-{:02}T{hh:02}:{mm:02}:{:06.3}",
        t.year, t.month, t.day, ss
    );
    if iso.starts_with(&date.value) && extra.is_none() {
        return date;
    }
    let from: Vec<&str> = std::iter::once(date.keyword().unwrap_or("DATE-OBS"))
        .chain(extra)
        .collect();
    Fact::derived(iso, &from)
}

/// Frame count from a HISTORY line such as `ImageIntegration.numberOfImages: 96`.
fn history_count(h: &Header, quirks: &[&Quirk]) -> Option<Fact<i64>> {
    quirks
        .iter()
        .flat_map(|q| &q.history_count)
        .find_map(|prefix| {
            h.history().find_map(|line| {
                let rest = line.trim().strip_prefix(prefix.as_str())?;
                let n: i64 = rest
                    .trim()
                    .split(|c: char| !c.is_ascii_digit())
                    .next()?
                    .parse()
                    .ok()?;
                Some(Fact {
                    value: n,
                    source: Source::Keyword {
                        key: format!("HISTORY {prefix}"),
                    },
                })
            })
        })
}

fn hms(s: &str) -> Option<(u32, u32, f64)> {
    let mut p = s.split(':');
    Some((
        p.next()?.trim().parse().ok()?,
        p.next()?.trim().parse().ok()?,
        p.next().map_or(Some(0.0), |x| x.trim().parse().ok())?,
    ))
}

/// Use the profile's value when the header lacks one; note header/profile
/// disagreements so the registry can be corrected.
fn fill(
    slot: &mut Option<Fact<f64>>,
    default: Option<f64>,
    model: &str,
    key: &str,
    raw: Option<f64>,
    unit: &str,
    notes: &mut Vec<Note>,
) {
    let Some(d) = default else { return };
    match slot {
        None => {
            if let Some(v) = raw {
                notes.push(Note {
                    level: NoteLevel::Warning,
                    message: format!("{key}={v} in header; used {model} default ({d} {unit})"),
                });
            }
            *slot = Some(Fact::model(d, model));
        }
        Some(f) if (f.value - d).abs() / d > 0.05 => notes.push(Note {
            level: NoteLevel::Info,
            message: format!(
                "{key}={} differs from the {model} profile ({d} {unit}); using the header",
                f.value
            ),
        }),
        _ => {}
    }
}

/// RA/Dec in degrees: numeric RA/DEC keys are degrees, OBJCTRA/OBJCTDEC are
/// sexagesimal (hours / degrees), and WCS CRVAL is the last resort.
fn coordinates(h: &Header) -> (Option<Fact<f64>>, Option<Fact<f64>>) {
    let numeric = |k: &str| h.float(k).map(|v| Fact::key(v, k));
    let ra = numeric("RA")
        .filter(|f| (0.0..360.0).contains(&f.value))
        .or_else(|| {
            h.string("RA")
                .and_then(fittle_astro::parse_ra)
                .map(|v| Fact::key(v, "RA"))
        })
        .or_else(|| {
            h.string("OBJCTRA")
                .and_then(fittle_astro::parse_ra)
                .map(|v| Fact::key(v, "OBJCTRA"))
        })
        .or_else(|| wcs_axis(h, "RA", 1));
    let dec = numeric("DEC")
        .filter(|f| (-90.0..=90.0).contains(&f.value))
        .or_else(|| {
            h.string("DEC")
                .and_then(fittle_astro::parse_dec)
                .map(|v| Fact::key(v, "DEC"))
        })
        .or_else(|| {
            h.string("OBJCTDEC")
                .and_then(fittle_astro::parse_dec)
                .map(|v| Fact::key(v, "OBJCTDEC"))
        })
        .or_else(|| wcs_axis(h, "DEC", 2));
    (ra, dec)
}

fn wcs_axis(h: &Header, axis: &str, n: u8) -> Option<Fact<f64>> {
    h.string(&format!("CTYPE{n}"))
        .filter(|t| t.trim().starts_with(axis))?;
    let key = format!("CRVAL{n}");
    h.float(&key).map(|v| Fact::key(v, &key))
}

fn sexagesimal(h: &Header, keys: &[&str]) -> Option<Fact<f64>> {
    keys.iter().find_map(|k| {
        h.string(k)
            .and_then(fittle_astro::parse_dec)
            .map(|v| Fact::key(v, k))
    })
}
