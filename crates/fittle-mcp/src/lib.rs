//! Fittle's MCP server (stdio): read tools, preview images, and dry-run-first
//! writes over the same `fittle-*` functions the CLI and GUI use.
//!
//! Safety (CLAUDE.md rule 5): every path must sit under an allowed root
//! folder; every tool that writes defaults to `dry_run: true` and returns the
//! plan; exports and compression only ever create new files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use fittle_core::edit::{NewValue, Op, Options};
use fittle_image::export::{self, Crop, ExportSpec, Format, Stretch};
use fittle_image::fpack;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, ListResourcesResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
    ResourceContents, ServerCapabilities, ServerConfig,
};
use rmcp::schemars::{self, JsonSchema};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::{Value, json};

const MAX_FILES: usize = 10_000;
/// How many per-file results a batch returns in full.
const SAMPLE: usize = 20;

// ---- allowed roots ------------------------------------------------------

/// Folders the server may read and write under.
#[derive(Debug, Clone)]
pub struct Roots(Vec<PathBuf>);

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/").or(p.strip_prefix("~\\")) {
        Some(rest) => home().map_or_else(|| PathBuf::from(p), |h| h.join(rest)),
        None if p == "~" => home().unwrap_or_else(|| PathBuf::from(p)),
        None => PathBuf::from(p),
    }
}

impl Roots {
    /// Explicit roots, else `FITTLE_MCP_ROOTS` (path-list), else the home folder.
    pub fn new(dirs: Vec<PathBuf>) -> Roots {
        let dirs = if !dirs.is_empty() {
            dirs
        } else if let Some(v) = std::env::var_os("FITTLE_MCP_ROOTS") {
            std::env::split_paths(&v).collect()
        } else {
            home().into_iter().collect()
        };
        Roots(
            dirs.into_iter()
                .filter_map(|d| expand(&d.to_string_lossy()).canonicalize().ok())
                .collect(),
        )
    }

    pub fn dirs(&self) -> &[PathBuf] {
        &self.0
    }

    /// Resolve `p` (which may not exist yet) and require it under a root.
    pub fn check(&self, p: &str) -> Result<PathBuf, String> {
        let path = expand(p);
        let resolved = match path.canonicalize() {
            Ok(c) => c,
            Err(_) => {
                let parent = path
                    .parent()
                    .filter(|d| !d.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
                let name = path
                    .file_name()
                    .ok_or_else(|| format!("{p}: not a file path"))?;
                parent
                    .canonicalize()
                    .map_err(|_| format!("{p}: folder does not exist"))?
                    .join(name)
            }
        };
        if self.0.iter().any(|r| resolved.starts_with(r)) {
            Ok(resolved)
        } else {
            Err(format!(
                "{p} is outside the allowed folders ({}); start `fittle mcp --root <dir>` to allow more",
                self.0
                    .iter()
                    .map(|r| r.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
    }

    /// Paths from an explicit list and/or a glob, FITS files only, all checked.
    fn files(&self, paths: &[String], pattern: Option<&str>) -> Result<Vec<PathBuf>, String> {
        let mut out = Vec::new();
        for p in paths {
            out.push(self.check(p)?);
        }
        if let Some(g) = pattern {
            let g = expand(g).to_string_lossy().into_owned();
            for e in glob::glob(&g)
                .map_err(|e| format!("bad glob: {e}"))?
                .flatten()
            {
                if e.is_file() && fittle_scan::is_fits(&e) {
                    out.push(self.check(&e.to_string_lossy())?);
                }
                if out.len() > MAX_FILES {
                    return Err(format!("more than {MAX_FILES} files; narrow the glob"));
                }
            }
        }
        out.sort();
        out.dedup();
        if out.is_empty() {
            return Err("no FITS files matched".into());
        }
        Ok(out)
    }
}

// ---- tool arguments -----------------------------------------------------

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PathsArg {
    /// FITS files (absolute, or starting with ~/).
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HeaderArg {
    pub path: String,
    /// Only this HDU (0 = primary).
    pub hdu: Option<usize>,
    /// Keep keywords containing this text (case-insensitive).
    pub grep: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PreviewArg {
    pub path: String,
    /// Long edge in pixels (default 768, at most 2048).
    pub max_edge: Option<u32>,
    /// `auto` (default), `linked`, `asinh` or `none`.
    pub stretch: Option<String>,
    /// `jpeg` (default) or `png`.
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PathArg {
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DiffArg {
    pub a: String,
    pub b: String,
    pub hdu: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanArg {
    /// Folder to summarize.
    pub path: String,
    /// Include subfolders.
    #[serde(default)]
    pub recursive: bool,
    /// Also return one row per file (at most 500).
    #[serde(default)]
    pub include_files: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetArg {
    /// Files to edit.
    #[serde(default)]
    pub paths: Vec<String>,
    /// And/or a glob, e.g. `~/Astro/NGC6995/**/*.fit`.
    pub glob: Option<String>,
    /// Keywords to set: numbers, booleans or strings.
    #[serde(default)]
    pub set: BTreeMap<String, Value>,
    /// Keywords to remove.
    #[serde(default)]
    pub unset: Vec<String>,
    /// Keywords to rename, old → new.
    #[serde(default)]
    pub rename: BTreeMap<String, String>,
    /// Comment for keywords being set.
    pub comment: Option<String>,
    /// Plan only (default true). Set false to write.
    #[serde(default = "yes")]
    pub dry_run: bool,
    /// Keep a .bak of each original (default true).
    #[serde(default = "yes")]
    pub backup: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScrubArg {
    #[serde(default)]
    pub paths: Vec<String>,
    pub glob: Option<String>,
    /// Plan only (default true).
    #[serde(default = "yes")]
    pub dry_run: bool,
    #[serde(default = "yes")]
    pub backup: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportArg {
    pub path: String,
    /// Output folder (default: beside the source). Must be allowed.
    pub out_dir: Option<String>,
    /// png, png16, jpeg, webp, avif, tiff8, tiff16, tiff32, fits (default png16).
    pub format: Option<String>,
    /// auto (default), linked, asinh, none.
    pub stretch: Option<String>,
    pub long_edge: Option<usize>,
    /// [x, y, width, height] in displayed orientation.
    pub crop: Option<[usize; 4]>,
    /// 0, 90, 180 or 270 (clockwise).
    pub rotate: Option<u16>,
    #[serde(default)]
    pub flip_horizontal: bool,
    #[serde(default)]
    pub flip_vertical: bool,
    pub bin: Option<u8>,
    /// Add the share-card caption strip.
    #[serde(default)]
    pub card: bool,
    /// Keep site coordinates and serials (default false: they are removed).
    #[serde(default)]
    pub keep_private: bool,
    /// File-name template, e.g. `{object}_{filter}_{integration}`.
    pub template: Option<String>,
    /// Plan only (default true): returns the name and size it would write.
    #[serde(default = "yes")]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PackArg {
    #[serde(default)]
    pub paths: Vec<String>,
    pub glob: Option<String>,
    /// Expand (funpack) instead of compress.
    #[serde(default)]
    pub unpack: bool,
    /// Plan only (default true).
    #[serde(default = "yes")]
    pub dry_run: bool,
}

// ---- helpers ------------------------------------------------------------

fn ok(v: Value) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![ContentBlock::text(
        serde_json::to_string_pretty(&v).unwrap_or_default(),
    )]))
}

/// A problem the agent should see and can fix (bad path, bad value).
fn fail(msg: impl Into<String>) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::error(vec![ContentBlock::text(msg.into())]))
}

macro_rules! tri {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return fail(e.to_string()),
        }
    };
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ErrorData> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))
}

fn stretch_of(s: Option<&str>) -> Result<Stretch, String> {
    Ok(match s.unwrap_or("auto") {
        "auto" => Stretch::Auto { linked: false },
        "linked" => Stretch::Auto { linked: true },
        "asinh" => Stretch::Asinh,
        "none" | "linear" => Stretch::None,
        other => {
            return Err(format!(
                "stretch '{other}': use auto, linked, asinh or none"
            ));
        }
    })
}

fn new_value(v: &Value) -> Result<NewValue, String> {
    Ok(match v {
        Value::Bool(b) => NewValue::Logical(*b),
        Value::Number(n) if n.is_i64() => NewValue::Integer(n.as_i64().unwrap_or(0)),
        Value::Number(n) => NewValue::Float(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => NewValue::String(s.clone()),
        other => return Err(format!("unsupported value {other}")),
    })
}

/// Plan every file, then (unless dry run) apply; nothing is written if any
/// plan fails. Returns the batch summary.
fn edit_batch(
    files: Vec<PathBuf>,
    ops_for: impl Fn(&Path) -> Result<Vec<Op>, String>,
    dry_run: bool,
    backup: bool,
) -> Result<Value, String> {
    let opts = Options {
        backup,
        history: true,
        checksum: true,
        hdu: None,
    };
    let mut plans = Vec::new();
    for f in &files {
        let ops = ops_for(f)?;
        let plan =
            fittle_core::edit::plan(f, &ops, &opts).map_err(|e| format!("{}: {e}", f.display()))?;
        plans.push((f.clone(), ops, plan));
    }
    let in_place = plans.iter().filter(|(_, _, p)| p.in_place).count();
    let changed = plans
        .iter()
        .filter(|(_, _, p)| !p.changes.is_empty())
        .count();
    let sample: Vec<Value> = plans
        .iter()
        .filter(|(_, _, p)| !p.changes.is_empty())
        .take(SAMPLE)
        .map(|(_, _, p)| json!(p))
        .collect();
    if dry_run {
        return Ok(json!({
            "dry_run": true,
            "files": files.len(),
            "would_change": changed,
            "in_place": in_place,
            "plans": sample,
            "next": "show the user this plan; call again with dry_run: false to write (backups on unless backup: false)",
        }));
    }
    let mut written = 0;
    let mut errors = Vec::new();
    for (f, ops, p) in &plans {
        if p.changes.is_empty() {
            continue;
        }
        match fittle_core::write::apply(f, ops, &opts) {
            Ok(_) => written += 1,
            Err(e) => errors.push(format!("{}: {e}", f.display())),
        }
    }
    Ok(json!({
        "dry_run": false,
        "files": files.len(),
        "written": written,
        "in_place": in_place,
        "errors": errors,
        "plans": sample,
    }))
}

fn spec_label(s: Option<&str>) -> &'static str {
    match s.unwrap_or("auto") {
        "linked" => "auto STF, linked",
        "asinh" => "arcsinh",
        "none" | "linear" => "none (linear)",
        _ => "auto STF",
    }
}

// ---- server -------------------------------------------------------------

#[derive(Clone)]
pub struct Fittle {
    roots: Arc<Roots>,
}

impl Fittle {
    pub fn new(roots: Roots) -> Fittle {
        Fittle {
            roots: Arc::new(roots),
        }
    }
}

#[tool_router]
impl Fittle {
    #[tool(
        description = "Explain FITS files: sub or stack (with confidence and the keywords that decided it), smart scope / rig / software, target, exposure and integration, site and time, derived facts (pixel scale, field of view, altitude, moon) and header health. Schema fittle.info/1.",
        annotations(read_only_hint = true)
    )]
    async fn fits_inspect(
        &self,
        Parameters(a): Parameters<PathsArg>,
    ) -> Result<CallToolResult, ErrorData> {
        let files = tri!(self.roots.files(&a.paths, None));
        let out = blocking(move || {
            files
                .iter()
                .map(|f| match fittle_core::info(f) {
                    Ok(i) => json!(i),
                    Err(e) => json!({ "path": f, "error": e.to_string() }),
                })
                .collect::<Vec<_>>()
        })
        .await?;
        ok(if out.len() == 1 {
            out.into_iter().next().unwrap_or_default()
        } else {
            json!(out)
        })
    }

    #[tool(
        description = "Header records of a FITS file, per HDU, with parsed values, comments and issues. Optionally one HDU or keywords matching `grep`. Schema fittle.header/1.",
        annotations(read_only_hint = true)
    )]
    async fn fits_header(
        &self,
        Parameters(a): Parameters<HeaderArg>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = tri!(self.roots.check(&a.path));
        let v = blocking(move || -> Result<Value, String> {
            let fits = fittle_core::Fits::open(&path).map_err(|e| e.to_string())?;
            let view = fits.filtered(a.hdu, a.grep.as_deref());
            let p = path.to_string_lossy();
            Ok(json!(fittle_core::HeaderDoc {
                schema: fittle_core::HEADER_SCHEMA,
                path: &p,
                fits: &view
            }))
        })
        .await?;
        ok(tri!(v))
    }

    #[tool(
        description = "A stretched preview image of a FITS file (debayered if colour), so you can see it. The stretch is for display only; the file is unchanged.",
        annotations(read_only_hint = true)
    )]
    async fn fits_preview(
        &self,
        Parameters(a): Parameters<PreviewArg>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = tri!(self.roots.check(&a.path));
        let stretch = tri!(stretch_of(a.stretch.as_deref()));
        let edge = a.max_edge.unwrap_or(768).clamp(64, 2048) as usize;
        let png = a.format.as_deref() == Some("png");
        let r = blocking(move || -> Result<(Vec<u8>, usize, usize, String), String> {
            let s = fittle_image::ViewSession::open(&path).map_err(|e| e.to_string())?;
            let o = s
                .image
                .as_ref()
                .ok_or_else(|| s.image_error.clone().unwrap_or_else(|| "no image".into()))?;
            let spec = ExportSpec {
                stretch,
                ..Default::default()
            };
            let r = export::preview(o, &s.info, &spec, edge).map_err(|e| e.to_string())?;
            let raster = export::raster(&r.image, 8);
            let bytes = if png {
                fittle_image::encode::png_bytes(&raster)
            } else {
                fittle_image::encode::jpeg_bytes(&raster, 85)
            }
            .map_err(|e| e.to_string())?;
            Ok((
                bytes,
                r.image.width,
                r.image.height,
                fittle_image::xmp::summary(&s.info),
            ))
        })
        .await?;
        let (bytes, w, h, summary) = tri!(r);
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        let mime = if png { "image/png" } else { "image/jpeg" };
        Ok(CallToolResult::success(vec![
            ContentBlock::image(data, mime),
            ContentBlock::text(format!(
                "{w}×{h} preview, display stretch {}. {summary}",
                spec_label(a.stretch.as_deref())
            )),
        ]))
    }

    #[tool(
        description = "Pixel statistics per channel (min, max, mean, median, MAD, clipped fractions) on the normalized image, the physical range 0 and 1 map to, and the auto-stretch parameters. Star count and HFR are not available yet.",
        annotations(read_only_hint = true)
    )]
    async fn fits_stats(
        &self,
        Parameters(a): Parameters<PathArg>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = tri!(self.roots.check(&a.path));
        let v = blocking(move || -> Result<Value, String> {
            let s = fittle_image::ViewSession::open(&path).map_err(|e| e.to_string())?;
            let o = s
                .image
                .as_ref()
                .ok_or_else(|| s.image_error.clone().unwrap_or_else(|| "no image".into()))?;
            let info = o.info();
            let d = o.display(info.default_mode);
            Ok(json!({
                "path": s.path,
                "width": info.width,
                "height": info.height,
                "planes": info.planes,
                "mode": info.default_mode,
                "normalized_from": info.normalized_from,
                "channels": d.stats,
                "auto_stf": d.stf,
            }))
        })
        .await?;
        ok(tri!(v))
    }

    #[tool(
        description = "Compare two FITS headers: added, removed and changed keywords, with the calibration impact (e.g. gain or temperature mismatch). Schema fittle.diff/1.",
        annotations(read_only_hint = true)
    )]
    async fn fits_diff(
        &self,
        Parameters(a): Parameters<DiffArg>,
    ) -> Result<CallToolResult, ErrorData> {
        let (pa, pb) = (tri!(self.roots.check(&a.a)), tri!(self.roots.check(&a.b)));
        let v = blocking(move || {
            fittle_core::diff::diff_files(&pa.to_string_lossy(), &pb.to_string_lossy(), a.hdu)
                .map(|d| json!(d))
                .map_err(|e| e.to_string())
        })
        .await?;
        ok(tri!(v))
    }

    #[tool(
        description = "Summarize a folder of FITS files: frame counts, stacks, nights, light subs and integration per target and filter, and warnings (mixed gain or sub length, damaged files). Schema fittle.scan/1.",
        annotations(read_only_hint = true)
    )]
    async fn fits_scan_folder(
        &self,
        Parameters(a): Parameters<ScanArg>,
    ) -> Result<CallToolResult, ErrorData> {
        let dir = tri!(self.roots.check(&a.path));
        let v = blocking(move || -> Result<Value, String> {
            let entries = if a.recursive {
                fittle_scan::list_recursive(&dir)
            } else {
                fittle_scan::list(&dir)
            }
            .map_err(|e| format!("{}: {e}", dir.display()))?;
            let summary = fittle_scan::summarize(&dir.to_string_lossy(), &entries);
            let mut v = json!(summary);
            if a.include_files {
                v["entries"] = json!(entries.iter().take(500).collect::<Vec<_>>());
            }
            Ok(v)
        })
        .await?;
        ok(tri!(v))
    }

    #[tool(
        description = "Set, remove or rename header keywords in one or more files (paths and/or glob). DRY RUN BY DEFAULT: returns each file's diff, whether it can be edited in place, and consequences; call again with dry_run: false only after the user agrees. Pixel data is never touched; backups on by default.",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    async fn fits_set_keywords(
        &self,
        Parameters(a): Parameters<SetArg>,
    ) -> Result<CallToolResult, ErrorData> {
        let files = tri!(self.roots.files(&a.paths, a.glob.as_deref()));
        let mut ops = Vec::new();
        for (k, v) in &a.set {
            ops.push(Op::Set {
                key: k.to_uppercase(),
                value: tri!(new_value(v)),
                comment: a.comment.clone(),
            });
        }
        for k in &a.unset {
            ops.push(Op::Unset {
                key: k.to_uppercase(),
            });
        }
        for (from, to) in &a.rename {
            ops.push(Op::Rename {
                from: from.to_uppercase(),
                to: to.to_uppercase(),
            });
        }
        if ops.is_empty() {
            return fail("nothing to do: give set, unset or rename");
        }
        let (dry, backup) = (a.dry_run, a.backup);
        let v = blocking(move || edit_batch(files, |_| Ok(ops.clone()), dry, backup)).await?;
        ok(tri!(v))
    }

    #[tool(
        description = "Remove site coordinates, observer names and serial numbers from headers before sharing. DRY RUN BY DEFAULT: returns the edits; call with dry_run: false to write (backups on).",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    async fn fits_scrub(
        &self,
        Parameters(a): Parameters<ScrubArg>,
    ) -> Result<CallToolResult, ErrorData> {
        let files = tri!(self.roots.files(&a.paths, a.glob.as_deref()));
        let (dry, backup) = (a.dry_run, a.backup);
        let v = blocking(move || {
            edit_batch(
                files,
                |p| {
                    let fits = fittle_core::Fits::open(p).map_err(|e| e.to_string())?;
                    Ok(fittle_core::privacy::scrub_ops(&fits, &p.to_string_lossy()))
                },
                dry,
                backup,
            )
        })
        .await?;
        ok(tri!(v))
    }

    #[tool(
        description = "Export a FITS file to PNG, JPEG, WebP, AVIF, TIFF or FITS with a display stretch, crop, rotate, flip, bin, resize, debayer and an optional share-card caption. Always a NEW file (never overwrites). DRY RUN BY DEFAULT: returns the file name, size and any problem; call with dry_run: false to write.",
        annotations(destructive_hint = false, idempotent_hint = false)
    )]
    async fn fits_export(
        &self,
        Parameters(a): Parameters<ExportArg>,
    ) -> Result<CallToolResult, ErrorData> {
        let src = tri!(self.roots.check(&a.path));
        let out_dir = match &a.out_dir {
            Some(d) => tri!(self.roots.check(d)),
            None => src.parent().map(Path::to_path_buf).unwrap_or_default(),
        };
        let format = match a.format.as_deref() {
            None => Format::Png { bits: 16 },
            Some(f) => match Format::parse(f) {
                Some(f) => f,
                None => {
                    return fail(format!(
                        "format '{f}': use png, png16, jpeg, webp, avif, tiff8/16/32 or fits"
                    ));
                }
            },
        };
        let spec = ExportSpec {
            format,
            stretch: tri!(stretch_of(a.stretch.as_deref())),
            crop: a.crop.map(|[x, y, width, height]| Crop {
                x,
                y,
                width,
                height,
            }),
            rotate: a.rotate.unwrap_or(0),
            flip_horizontal: a.flip_horizontal,
            flip_vertical: a.flip_vertical,
            bin: a.bin,
            long_edge: a.long_edge,
            card: a.card,
            private: !a.keep_private,
            ..Default::default()
        };
        let template = a.template.unwrap_or_else(|| "{name}".into());
        let dry = a.dry_run;
        let v = blocking(move || -> Result<Value, String> {
            let info = fittle_core::info(&src).map_err(|e| e.to_string())?;
            let plan = export::plan(&info, &spec, &template);
            if dry || plan.problem.is_some() {
                return Ok(json!({
                    "dry_run": true,
                    "out_dir": out_dir,
                    "plan": plan,
                    "next": "call again with dry_run: false to write",
                }));
            }
            std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
            export::export(&src, &out_dir, Some(&template), &spec)
                .map(|e| json!(e))
                .map_err(|e| e.to_string())
        })
        .await?;
        ok(tri!(v))
    }

    #[tool(
        description = "Compress FITS files losslessly (fpack: RICE_1 for integers, GZIP_2 for floats) or expand .fz files (unpack: true). Writes NEW files beside the sources, verified by decoding them again; never replaces anything. DRY RUN BY DEFAULT.",
        annotations(destructive_hint = false, idempotent_hint = false)
    )]
    async fn fits_fpack(
        &self,
        Parameters(a): Parameters<PackArg>,
    ) -> Result<CallToolResult, ErrorData> {
        let files = tri!(self.roots.files(&a.paths, a.glob.as_deref()));
        let (unpack, dry) = (a.unpack, a.dry_run);
        let roots = self.roots.clone();
        let v = blocking(move || -> Result<Value, String> {
            let mut rows = Vec::new();
            for f in &files {
                let out = if unpack {
                    fpack::unpacked_name(f)
                } else {
                    fpack::packed_name(f)
                };
                roots.check(&out.to_string_lossy())?;
                if dry {
                    rows.push(json!({ "source": f, "path": out, "exists": out.exists() }));
                    continue;
                }
                let r = if unpack {
                    fpack::funpack(f, &out)
                } else {
                    fpack::fpack(f, &out, &Default::default())
                };
                rows.push(match r {
                    Ok(r) => json!(r),
                    Err(e) => json!({ "source": f, "error": e.to_string() }),
                });
            }
            Ok(json!({ "dry_run": dry, "files": rows }))
        })
        .await?;
        ok(tri!(v))
    }
}

const INSTRUCTIONS: &str = "Fittle reads astrophotography FITS files. Start with fits_inspect (what is this file?) or fits_scan_folder (what is in this folder?), and fits_preview to look at an image. Tools that write (fits_set_keywords, fits_scrub, fits_export, fits_fpack) are dry-run by default: show the user the plan and only call again with dry_run: false after they agree. Header edits never change pixel data; exports and compression always create new files. Paths must be inside the allowed folders. Resources fits://keywords, fits://quirks and fits://scopes explain keywords and vendor behaviour.";

#[tool_handler]
impl ServerHandler for Fittle {
    fn get_info(&self) -> ServerConfig {
        let mut server = Implementation::new("fittle", env!("CARGO_PKG_VERSION"));
        server.title = Some("Fittle".into());
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(server)
        .with_instructions(INSTRUCTIONS)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new("fits://keywords", "keywords")
                .with_description(
                    "FITS keyword dictionary: meaning, units and which apps write each keyword",
                )
                .with_mime_type("application/json"),
            Resource::new("fits://quirks", "quirks")
                .with_description(
                    "Capture and processing software fingerprints and header quirks (apps.json)",
                )
                .with_mime_type("application/json"),
            Resource::new("fits://scopes", "scopes")
                .with_description(
                    "Smart-scope registry: match rules and optics (scope-profiles.json)",
                )
                .with_mime_type("application/json"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let text = match request.uri.as_str() {
            "fits://keywords" => {
                serde_json::to_string_pretty(fittle_core::dict::KEYWORDS).unwrap_or_default()
            }
            "fits://quirks" => fittle_core::vendor::APPS_JSON.to_string(),
            "fits://scopes" => fittle_core::vendor::SCOPES_JSON.to_string(),
            other => {
                return Err(ErrorData::resource_not_found(
                    format!("no resource {other}"),
                    None,
                ));
            }
        };
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, request.uri.clone()).with_mime_type("application/json"),
        ])
        .into())
    }
}

/// Run the server on stdin/stdout until the client disconnects.
pub fn run_stdio(roots: Roots) -> Result<(), Box<dyn std::error::Error>> {
    if roots.dirs().is_empty() {
        return Err("no allowed folders: pass --root <dir> (or set FITTLE_MCP_ROOTS)".into());
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let service = Fittle::new(roots).serve(rmcp::transport::stdio()).await?;
        service.waiting().await?;
        Ok(())
    })
}
