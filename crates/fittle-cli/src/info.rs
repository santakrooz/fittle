//! `fittle info`

use std::io::{self, Write};
use std::path::PathBuf;

use clap::Args as ClapArgs;
use fittle_core::canonical::{Fact, NoteLevel, Source};
use fittle_core::{Info, Severity};

use crate::exit;
use crate::fmt::{bad, bold, dec_dms, derived, duration, good, muted, num, ra_hms, warn};

#[derive(ClapArgs)]
pub struct Args {
    /// FITS files to explain
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Print JSON (schema fittle.info/1), one document per line
    #[arg(long)]
    json: bool,
}

pub fn run(args: Args) -> u8 {
    let mut out = io::stdout().lock();
    let mut code = exit::OK;
    for (i, path) in args.files.iter().enumerate() {
        let info = match fittle_core::info(path) {
            Ok(info) => info,
            Err(e) => {
                eprintln!("fittle: {}: {e}", path.display());
                code = exit::ERROR;
                continue;
            }
        };
        let r = if args.json {
            serde_json::to_writer(&mut out, &info)
                .map_err(io::Error::from)
                .and_then(|_| writeln!(out))
        } else {
            if i > 0 {
                let _ = writeln!(out);
            }
            write_human(&mut out, &info)
        };
        if let Err(e) = r {
            if e.kind() == io::ErrorKind::BrokenPipe {
                return code;
            }
            eprintln!("fittle: {e}");
            return exit::ERROR;
        }
    }
    code
}

fn row(out: &mut impl Write, label: &str, value: &str) -> io::Result<()> {
    if value.is_empty() {
        return Ok(());
    }
    writeln!(out, "  {}{value}", muted(&format!("{label:<10}")))
}

fn join(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Mark a value taken from a profile default or derived, not the header.
fn tagged<T>(f: &Fact<T>, text: String) -> String {
    match f.source {
        Source::ModelDefault { .. } => format!("{text}{}", muted("*")),
        Source::Derived { .. } => format!("{text}{}", derived("ᵈ")),
        _ => text,
    }
}

fn write_human(out: &mut impl Write, info: &Info) -> io::Result<()> {
    let v = &info.verdict;
    let f = &info.fields;
    let d = &info.derived;
    let name = info.path.rsplit(['/', '\\']).next().unwrap_or(&info.path);
    writeln!(out, "{}", muted(name))?;
    writeln!(
        out,
        "  {}  {}",
        bold(&v.label),
        good(&format!("confidence {}", v.confidence))
    )?;
    let supports = |s: &str| match s {
        "integrated" => v.integrated,
        "single" => !v.integrated,
        _ => true,
    };
    let ev: Vec<String> = v
        .evidence
        .iter()
        .filter(|e| supports(&e.supports))
        .take(5)
        .map(|e| e.text.clone())
        .collect();
    writeln!(out, "  {}", muted(&format!("evidence: {}", ev.join(" · "))))?;
    if !v.processing.is_empty() {
        let steps: Vec<&str> = v.processing.iter().map(|s| s.name).collect();
        writeln!(
            out,
            "  {}",
            muted(&format!("history:  {}", steps.join(" → ")))
        )?;
    }
    writeln!(out)?;

    // Target
    let target = match (&f.object, &info.target) {
        (Some(o), Some(t)) => join(vec![
            match &t.common_name {
                Some(c) => format!("{} ({c})", o.value),
                None => o.value.clone(),
            },
            t.kind.clone(),
            t.constellation.clone().unwrap_or_default(),
        ]),
        (Some(o), None) => o.value.clone(),
        (None, Some(t)) => format!("near {} ({})", t.id, t.kind),
        (None, None) => String::new(),
    };
    row(out, "TARGET", &target)?;
    if let (Some(r), Some(dd)) = (&f.ra, &f.dec) {
        row(
            out,
            "",
            &format!("RA {}  Dec {}", ra_hms(r.value), dec_dms(dd.value)),
        )?;
    }

    // Optics / camera
    row(
        out,
        "OPTICS",
        &join(vec![
            f.telescope
                .as_ref()
                .map_or(String::new(), |t| t.value.clone()),
            f.focal_mm.as_ref().map_or(String::new(), |x| {
                tagged(x, format!("{} mm", num(x.value, 1)))
            }),
            f.focal_ratio.as_ref().map_or(String::new(), |x| {
                tagged(x, format!("ƒ/{}", num(x.value, 1)))
            }),
            f.aperture_mm.as_ref().map_or(String::new(), |x| {
                tagged(x, format!("{} mm aperture", num(x.value, 1)))
            }),
        ]),
    )?;
    row(
        out,
        "CAMERA",
        &join(vec![
            f.camera.as_ref().map_or(String::new(), |x| x.value.clone()),
            f.sensor
                .as_ref()
                .map_or(String::new(), |x| tagged(x, x.value.clone())),
            f.pixel_um.as_ref().map_or(String::new(), |x| {
                tagged(x, format!("{} µm", num(x.value, 2)))
            }),
            f.binning
                .as_ref()
                .filter(|b| b.value > 1)
                .map_or(String::new(), |b| format!("bin {}", b.value)),
            f.gain
                .as_ref()
                .map_or(String::new(), |x| format!("gain {}", num(x.value, 2))),
            f.offset
                .as_ref()
                .map_or(String::new(), |x| format!("offset {}", num(x.value, 0))),
            f.sensor_temp_c
                .as_ref()
                .map_or(String::new(), |x| format!("{} °C", num(x.value, 1))),
            f.bayer.as_ref().map_or(String::new(), |x| x.value.clone()),
        ]),
    )?;

    let mount = join(vec![
        f.mount.as_ref().map_or(String::new(), |m| m.value.clone()),
        f.pier_side.as_ref().map_or(String::new(), |p| {
            format!("pier {}", p.value.to_lowercase())
        }),
    ]);
    row(out, "MOUNT", &mount)?;

    // Exposure
    let sub = f
        .exposure_s
        .as_ref()
        .map(|e| format!("{} s", num(e.value, 3)));
    let exposure = match (&f.stack_count, &f.total_integration_s, &sub) {
        (Some(n), Some(t), Some(s)) if v.integrated && t.value >= 1.0 => {
            format!("{} × {s} = {}", n.value, tagged(t, duration(t.value)))
        }
        (Some(n), _, Some(s)) if v.integrated => format!("{} × {s}", n.value),
        (_, _, Some(s)) => s.clone(),
        _ => String::new(),
    };
    row(
        out,
        "EXPOSURE",
        &join(vec![
            exposure,
            f.filter
                .as_ref()
                .map_or(String::new(), |x| format!("filter {}", x.value)),
        ]),
    )?;
    if let (Some(lat), Some(lon)) = (&f.site_lat, &f.site_lon) {
        row(
            out,
            "SITE",
            &format!("{:.2}, {:.2}", lat.value, lon.value).replace('-', "−"),
        )?;
    }
    let time = f.date_obs.as_ref().map_or(String::new(), |t| {
        let whole = t.value.split('.').next().unwrap_or(&t.value);
        format!("{} UTC", whole.replace('T', " "))
    });
    let night = d
        .session_night
        .as_ref()
        .map_or(String::new(), |n| derived(&format!("night of {}", n.value)));
    row(out, "TIME", &join(vec![time, night]))?;

    // Origin
    let mut who: Vec<String> = Vec::new();
    if let Some(s) = &info.origin.scope {
        who.push(s.name.clone());
    }
    who.extend(
        info.origin
            .software
            .iter()
            .filter(|s| info.origin.scope.is_none() || s.matched.id != "seestar-app")
            .map(|s| s.matched.name.clone()),
    );
    row(out, "MADE BY", &who.join(" → "))?;

    // Derived
    let mut dv: Vec<String> = Vec::new();
    if let Some(s) = d.wcs_scale.as_ref().or(d.pixel_scale.as_ref()) {
        dv.push(format!(
            "scale {}″/px{}",
            num(s.value, 2),
            if d.plate_solved { " (solved)" } else { "" }
        ));
    }
    if let Some(fv) = &d.fov_arcmin {
        dv.push(format!(
            "FOV {}′×{}′",
            fv.value[0].round(),
            fv.value[1].round()
        ));
    }
    if let Some(s) = &d.sampling {
        dv.push(format!("{}-sampled", s.value));
    }
    if let Some(a) = &d.altitude {
        dv.push(format!("alt {}°", a.value.round()));
    }
    if let Some(a) = &d.airmass {
        dv.push(format!("airmass {}", num(a.value, 2)));
    }
    if let Some(m) = &d.moon {
        let mut s = format!("moon {}%", (m.value.illumination * 100.0).round());
        if let Some(sep) = m.value.separation {
            s.push_str(&format!(" {}° away", sep.round()));
        }
        if m.value.altitude.is_some_and(|a| a < 0.0) {
            s.push_str(" (set)");
        }
        dv.push(s);
    }
    if let Some(s) = d.sky.as_ref().filter(|s| s.value != "night") {
        dv.push(s.value.clone());
    }
    if !dv.is_empty() {
        writeln!(out)?;
        writeln!(
            out,
            "  {}{}",
            derived(&format!("{:<10}", "derived")),
            dv.join(" · ")
        )?;
    }
    if let Some(img) = &info.image {
        let depth = match img.bitpix {
            Some(8) => "u8".to_string(),
            Some(16) => "16-bit".into(),
            Some(32) => "32-bit int".into(),
            Some(-32) => "32-bit float".into(),
            Some(-64) => "64-bit float".into(),
            Some(b) => format!("BITPIX {b}"),
            None => String::new(),
        };
        let planes = if img.planes == 3 {
            "RGB".to_string()
        } else if img.planes > 1 {
            format!("{} planes", img.planes)
        } else {
            String::new()
        };
        let fz = if img.compressed {
            "tile-compressed".to_string()
        } else {
            String::new()
        };
        writeln!(
            out,
            "  {}{}",
            derived(&format!("{:<10}", "image")),
            join(vec![
                format!("{}×{}", img.width, img.height),
                depth,
                planes,
                fz
            ])
        )?;
    }

    // Notes and health
    let notes: Vec<String> = info
        .notes
        .iter()
        .map(|n| match n.level {
            NoteLevel::Warning => format!("{} {}", warn("⚠"), n.message),
            NoteLevel::Info => format!("{} {}", muted("·"), muted(&n.message)),
        })
        .chain(info.health.iter().map(|i| match i.severity {
            Severity::Error => format!("{} {}: {}", bad("✗"), i.code, i.message),
            _ => format!("{} {}: {}", warn("⚠"), i.code, i.message),
        }))
        .collect();
    if !notes.is_empty() {
        writeln!(out)?;
        for n in notes {
            writeln!(out, "  {n}")?;
        }
    }
    if info.fields.serials.iter().any(|s| !s.value.is_empty()) {
        let keys: Vec<&str> = info.fields.serials.iter().map(|s| s.key.as_str()).collect();
        writeln!(
            out,
            "  {} {}",
            muted("·"),
            muted(&format!(
                "serial number in {} (scrub before sharing)",
                keys.join(", ")
            ))
        )?;
    }
    let marks = [(muted("*"), "profile default"), (derived("ᵈ"), "derived")];
    writeln!(
        out,
        "  {}",
        muted(
            &marks
                .iter()
                .map(|(m, t)| format!("{m} {t}"))
                .collect::<Vec<_>>()
                .join("   ")
        )
    )?;
    Ok(())
}
