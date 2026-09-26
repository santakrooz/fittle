//! `fittle export | crop | rotate | flip | bin | resize | debayer`
//!
//! All of these write new files next to the source (or into `-o`) and never
//! replace an existing file. `export` defaults to a stretched 16-bit PNG;
//! the geometry commands default to linear FITS, keeping the header.

use std::path::{Path, PathBuf};

use clap::{Args as ClapArgs, ValueEnum};
use fittle_image::export::{Crop, ExportError, ExportSpec, Exported, Format, Stretch, export};
use fittle_image::geom::BinMode;
use serde::Serialize;

use crate::exit;
use crate::fmt::{bad, good, muted};

#[derive(Clone, Copy, ValueEnum)]
pub enum StretchArg {
    /// Linear data, no stretch
    None,
    /// Auto STF per channel
    Auto,
    /// Auto STF from luminance (keeps colour balance)
    Linked,
    /// Arcsinh from the auto black point
    Asinh,
}

/// Output flags shared by every exporting command.
#[derive(ClapArgs, Clone)]
pub struct Output {
    /// Output directory, or a file name for a single input
    #[arg(short, long, value_name = "PATH")]
    out: Option<PathBuf>,
    /// png, png16, jpeg, webp, avif, tiff8, tiff16, tiff32, fits (default from -o's extension)
    #[arg(short, long, value_name = "FORMAT")]
    format: Option<String>,
    /// Display stretch baked into the pixels
    #[arg(long, value_enum)]
    stretch: Option<StretchArg>,
    /// JPEG / AVIF quality 1–100 (default 92 / 80)
    #[arg(long)]
    quality: Option<u8>,
    /// File-name template, e.g. '{object}_{filter}_{integration}' (see `fittle export --tokens`)
    #[arg(short, long)]
    template: Option<String>,
    /// Keep raw colour (CFA) frames undebayered
    #[arg(long)]
    no_debayer: bool,
    /// Don't embed the acquisition summary (XMP)
    #[arg(long)]
    no_metadata: bool,
    /// Keep site location, observer and serial numbers in the output
    #[arg(long)]
    keep_private: bool,
    /// Print JSON (schema fittle.export/1)
    #[arg(long)]
    json: bool,
}

#[derive(ClapArgs)]
pub struct ExportArgs {
    #[arg(required_unless_present = "tokens")]
    files: Vec<PathBuf>,
    #[command(flatten)]
    out: Output,
    /// Crop X,Y,W,H (displayed orientation, before rotating)
    #[arg(long, value_name = "X,Y,W,H", value_parser = parse_rect)]
    crop: Option<Crop>,
    /// Rotate clockwise by 90, 180 or 270 degrees
    #[arg(long, value_name = "DEG")]
    rotate: Option<u16>,
    /// Mirror left–right
    #[arg(long)]
    flip_h: bool,
    /// Mirror top–bottom
    #[arg(long)]
    flip_v: bool,
    /// Software bin N×N
    #[arg(long, value_name = "N")]
    bin: Option<u8>,
    /// Sum binned pixels instead of averaging
    #[arg(long)]
    bin_sum: bool,
    /// Resample so the long edge is N pixels
    #[arg(long, value_name = "N")]
    long_edge: Option<usize>,
    /// Add a share-card caption strip (target, rig, integration, date)
    #[arg(long)]
    card: bool,
    /// List file-name template tokens
    #[arg(long)]
    tokens: bool,
}

#[derive(ClapArgs)]
pub struct CropArgs {
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// X,Y,W,H in displayed orientation
    #[arg(long, value_name = "X,Y,W,H", value_parser = parse_rect)]
    rect: Crop,
    #[command(flatten)]
    out: Output,
}

#[derive(ClapArgs)]
pub struct RotateArgs {
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// 90, 180 or 270 (clockwise)
    #[arg(long, default_value_t = 90)]
    degrees: u16,
    #[command(flatten)]
    out: Output,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Axis {
    /// Mirror left–right
    H,
    /// Mirror top–bottom
    V,
}

#[derive(ClapArgs)]
pub struct FlipArgs {
    #[arg(required = true)]
    files: Vec<PathBuf>,
    #[arg(long, value_enum, default_value = "h")]
    axis: Axis,
    #[command(flatten)]
    out: Output,
}

#[derive(ClapArgs)]
pub struct BinArgs {
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Bin factor
    #[arg(short = 'n', long, default_value_t = 2)]
    factor: u8,
    /// Sum instead of average
    #[arg(long)]
    sum: bool,
    #[command(flatten)]
    out: Output,
}

#[derive(ClapArgs)]
pub struct ResizeArgs {
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Long edge in pixels
    #[arg(long, value_name = "N")]
    long_edge: usize,
    #[command(flatten)]
    out: Output,
}

#[derive(ClapArgs)]
pub struct DebayerArgs {
    #[arg(required = true)]
    files: Vec<PathBuf>,
    #[command(flatten)]
    out: Output,
}

fn parse_rect(s: &str) -> Result<Crop, String> {
    let v: Vec<usize> = s
        .split(',')
        .map(|p| {
            p.trim()
                .parse::<usize>()
                .map_err(|_| format!("'{p}' is not a whole number"))
        })
        .collect::<Result<_, _>>()?;
    match v[..] {
        [x, y, width, height] => Ok(Crop {
            x,
            y,
            width,
            height,
        }),
        _ => Err("expected X,Y,W,H".into()),
    }
}

fn invalid(msg: impl std::fmt::Display) -> u8 {
    eprintln!("fittle: {msg}");
    exit::VALIDATION
}

/// Defaults a command gives its shared output flags.
struct Defaults {
    format: Format,
    stretch: Stretch,
    template: &'static str,
}

impl Output {
    fn spec(&self, d: &Defaults) -> Result<ExportSpec, String> {
        let from_ext = self
            .out
            .as_ref()
            .filter(|p| !p.is_dir())
            .and_then(|p| p.extension())
            .and_then(|e| Format::parse(&e.to_string_lossy()));
        let mut format = match &self.format {
            Some(f) => Format::parse(f).ok_or_else(|| format!("unknown format '{f}'"))?,
            None => from_ext.unwrap_or(d.format),
        };
        let default_quality = if matches!(format, Format::Avif { .. }) {
            80
        } else {
            92
        };
        if let Format::Jpeg { quality } | Format::Avif { quality } = &mut format {
            *quality = self.quality.unwrap_or(default_quality).clamp(1, 100);
        }
        let stretch = match self.stretch {
            None => d.stretch.clone(),
            Some(StretchArg::None) => Stretch::None,
            Some(StretchArg::Auto) => Stretch::Auto { linked: false },
            Some(StretchArg::Linked) => Stretch::Auto { linked: true },
            Some(StretchArg::Asinh) => Stretch::Asinh,
        };
        Ok(ExportSpec {
            format,
            stretch,
            debayer: !self.no_debayer,
            metadata: !self.no_metadata,
            private: !self.keep_private,
            ..Default::default()
        })
    }
}

#[derive(Serialize)]
struct Doc {
    schema: &'static str,
    files: Vec<Row>,
}

#[derive(Serialize)]
struct Row {
    source: String,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    output: Option<Exported>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn run(files: &[PathBuf], out: &Output, d: Defaults, edit: impl Fn(&mut ExportSpec)) -> u8 {
    let mut spec = match out.spec(&d) {
        Ok(s) => s,
        Err(e) => return invalid(e),
    };
    edit(&mut spec);
    let target_is_file = out
        .out
        .as_ref()
        .is_some_and(|p| !p.is_dir() && p.extension().is_some());
    if target_is_file && files.len() > 1 {
        return invalid("-o names a file; give a directory for several inputs");
    }
    if let Some(dir) = out.out.as_ref().filter(|_| !target_is_file) {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("fittle: {}: {e}", dir.display());
            return exit::ERROR;
        }
    }
    let template = out.template.as_deref().unwrap_or(d.template);
    let mut rows = Vec::new();
    let mut code = exit::OK;
    for f in files {
        let dest: PathBuf = match &out.out {
            Some(p) => p.clone(),
            None => f
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .to_path_buf(),
        };
        let r = export(f, &dest, Some(template), &spec);
        let (output, error) = match r {
            Ok(o) => (Some(o), None),
            Err(e) => {
                code = code.max(match e {
                    ExportError::Invalid(_) => exit::VALIDATION,
                    _ => exit::ERROR,
                });
                (None, Some(e.to_string()))
            }
        };
        if !out.json {
            match (&output, &error) {
                (Some(o), _) => println!(
                    "{} {} {}",
                    good("✓"),
                    o.path,
                    muted(&format!(
                        "{}×{}, {} · {}",
                        o.width,
                        o.height,
                        human(o.bytes),
                        o.steps.join(", ")
                    ))
                ),
                (_, Some(e)) => eprintln!("{} {}: {e}", bad("✗"), f.display()),
                _ => {}
            }
        }
        rows.push(Row {
            source: f.to_string_lossy().into_owned(),
            output,
            error,
        });
    }
    if out.json {
        let doc = Doc {
            schema: "fittle.export/1",
            files: rows,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&doc).expect("serializable")
        );
    }
    code
}

fn human(b: u64) -> String {
    match b {
        0..1_000 => format!("{b} B"),
        1_000..1_000_000 => format!("{:.0} kB", b as f64 / 1e3),
        _ => format!("{:.1} MB", b as f64 / 1e6),
    }
}

const LINEAR: fn(&'static str) -> Defaults = |template| Defaults {
    format: Format::Fits,
    stretch: Stretch::None,
    template,
};

pub fn run_export(a: ExportArgs) -> u8 {
    if a.tokens {
        for (k, d) in fittle_core::naming::TOKENS {
            println!("{{{k}}}  {}", muted(d));
        }
        return exit::OK;
    }
    let d = Defaults {
        format: Format::Png { bits: 16 },
        stretch: Stretch::Auto { linked: false },
        template: "{name}",
    };
    run(&a.files, &a.out, d, |s| {
        s.crop = a.crop;
        s.rotate = a.rotate.unwrap_or(0);
        s.flip_horizontal = a.flip_h;
        s.flip_vertical = a.flip_v;
        s.bin = a.bin;
        s.bin_mode = if a.bin_sum {
            BinMode::Sum
        } else {
            BinMode::Average
        };
        s.long_edge = a.long_edge;
        s.card = a.card;
    })
}

pub fn run_crop(a: CropArgs) -> u8 {
    run(&a.files, &a.out, LINEAR("{name}_crop"), |s| {
        s.crop = Some(a.rect)
    })
}

pub fn run_rotate(a: RotateArgs) -> u8 {
    run(&a.files, &a.out, LINEAR("{name}_rot"), |s| {
        s.rotate = a.degrees
    })
}

pub fn run_flip(a: FlipArgs) -> u8 {
    run(&a.files, &a.out, LINEAR("{name}_flip"), |s| match a.axis {
        Axis::H => s.flip_horizontal = true,
        Axis::V => s.flip_vertical = true,
    })
}

pub fn run_bin(a: BinArgs) -> u8 {
    run(&a.files, &a.out, LINEAR("{name}_bin"), |s| {
        s.bin = Some(a.factor);
        s.bin_mode = if a.sum {
            BinMode::Sum
        } else {
            BinMode::Average
        };
    })
}

pub fn run_resize(a: ResizeArgs) -> u8 {
    run(&a.files, &a.out, LINEAR("{name}_resized"), |s| {
        s.long_edge = Some(a.long_edge)
    })
}

pub fn run_debayer(a: DebayerArgs) -> u8 {
    run(&a.files, &a.out, LINEAR("{name}_rgb"), |s| s.debayer = true)
}
