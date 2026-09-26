//! Tauri shell. Commands are thin adapters over `fittle-*` crates; no FITS
//! logic lives here (CLAUDE.md rule 3). Pixel payloads are binary:
//! `[u32 width][u32 height][u32 channels]` little-endian, then the data.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fittle_core::dict::{self, KeywordInfo};
use fittle_core::edit::{EditError, Op, Options, Plan};
use fittle_core::write::WriteReport;
use fittle_core::{Fits, HEADER_SCHEMA, HeaderDoc};
use fittle_image::encode::Samples;
use fittle_image::export::{self, ExportSpec};
use fittle_image::view::{Display, OpenedInfo};
use fittle_image::{Mode, Readout, ViewSession};
use fittle_scan::Entry;
use fittle_scan::report::Report;
use serde::Serialize;
use tauri::State;
use tauri::async_runtime::spawn_blocking;
use tauri::ipc::Response;

#[derive(Default)]
struct AppState {
    session: Mutex<Option<Arc<ViewSession>>>,
    thumbs: Mutex<HashMap<String, Arc<Vec<u8>>>>,
    /// The last session report, kept for saving.
    report: Mutex<Option<Arc<SavedReport>>>,
}

/// A report and the folder entries it was built from.
type SavedReport = (Report, Vec<Entry>);

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn packed(w: usize, h: usize, ch: usize, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    for v in [w, h, ch] {
        out.extend((v as u32).to_le_bytes());
    }
    out.extend_from_slice(data);
    out
}

/// `FITTLE_TRACE=1` logs command timings to stderr.
fn trace(what: &str, t: std::time::Instant) {
    if std::env::var_os("FITTLE_TRACE").is_some() {
        eprintln!("[fittle] {what}: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    }
}

fn current(state: &State<'_, AppState>) -> Res<Arc<ViewSession>> {
    state
        .session
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "no file open".to_string())
}

#[derive(Serialize)]
struct Opened {
    info: fittle_core::Info,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<OpenedInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_error: Option<String>,
    wcs: bool,
}

/// Open a file for viewing: `fittle info` plus decoded pixels.
#[tauri::command]
async fn open_file(path: String, state: State<'_, AppState>) -> Res<Opened> {
    let t = std::time::Instant::now();
    let s = spawn_blocking(move || ViewSession::open(&path))
        .await
        .map_err(err)?
        .map_err(err)?;
    trace("open_file (header, info, decode, stats)", t);
    let out = Opened {
        info: s.info.clone(),
        image: s.image.as_ref().map(|o| o.info()),
        image_error: s.image_error.clone(),
        wcs: s.wcs.is_some(),
    };
    *state.session.lock().unwrap() = Some(Arc::new(s));
    Ok(out)
}

/// Size, stats and auto-STF of the open image in a display mode.
#[tauri::command]
async fn display(mode: Mode, state: State<'_, AppState>) -> Res<Display> {
    let s = current(&state)?;
    spawn_blocking(move || {
        s.image
            .as_ref()
            .map(|o| o.display(mode).clone())
            .ok_or("no image".to_string())
    })
    .await
    .map_err(err)?
}

/// Whole-image preview (f16, long edge ≤ max_edge).
#[tauri::command]
async fn preview(mode: Mode, max_edge: usize, state: State<'_, AppState>) -> Res<Response> {
    let s = current(&state)?;
    let t = std::time::Instant::now();
    let r = spawn_blocking(move || {
        let o = s.image.as_ref().ok_or("no image")?;
        let ch = o.display(mode).channels;
        let (w, h, data) = o.preview(mode, max_edge);
        Ok::<_, String>(Response::new(packed(w, h, ch, &data)))
    })
    .await
    .map_err(err)?;
    trace("preview", t);
    r
}

/// Full-resolution region of the display image (f16), box-averaged by `step`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn region(
    mode: Mode,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    step: usize,
    state: State<'_, AppState>,
) -> Res<Response> {
    let s = current(&state)?;
    spawn_blocking(move || {
        let o = s.image.as_ref().ok_or("no image")?;
        let ch = o.display(mode).channels;
        let (rw, rh, data) = o.region(mode, x, y, w, h, step);
        Ok::<_, String>(Response::new(packed(rw, rh, ch, &data)))
    })
    .await
    .map_err(err)?
}

/// Raw/normalized values and RA/Dec at a source pixel.
#[tauri::command]
fn readout(x: usize, y: usize, state: State<'_, AppState>) -> Res<Option<Readout>> {
    Ok(current(&state)?.readout(x, y))
}

#[tauri::command]
async fn list_folder(path: String) -> Res<Vec<Entry>> {
    spawn_blocking(move || fittle_scan::list(&path).map_err(err))
        .await
        .map_err(err)?
}

/// Auto-stretched RGBA thumbnail, cached per path.
#[tauri::command]
async fn thumbnail(path: String, state: State<'_, AppState>) -> Res<Response> {
    if let Some(t) = state.thumbs.lock().unwrap().get(&path) {
        return Ok(Response::new(t.as_ref().clone()));
    }
    let key = path.clone();
    let bytes = spawn_blocking(move || {
        fittle_image::thumbnail(&path, 96).map(|(w, h, px)| packed(w, h, 4, &px))
    })
    .await
    .map_err(err)?
    .ok_or("no thumbnail")?;
    state
        .thumbs
        .lock()
        .unwrap()
        .insert(key, Arc::new(bytes.clone()));
    Ok(Response::new(bytes))
}

/// `fittle header --json` for one file.
#[tauri::command]
async fn header(path: String) -> Res<serde_json::Value> {
    spawn_blocking(move || {
        let fits = Fits::open(&path).map_err(err)?;
        serde_json::to_value(HeaderDoc {
            schema: HEADER_SCHEMA,
            path: &path,
            fits: &fits,
        })
        .map_err(err)
    })
    .await
    .map_err(err)?
}

/// What `ops` would do to `path` (dry run; nothing is written).
#[tauri::command]
async fn plan_edit(path: String, ops: Vec<Op>, options: Options) -> Res<Plan> {
    spawn_blocking(move || fittle_core::edit::plan(&path, &ops, &options).map_err(err))
        .await
        .map_err(err)?
}

#[derive(Serialize)]
struct FileResult {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<WriteReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Apply `ops` to every file. All files are planned and validated first; a
/// validation error anywhere writes nothing.
#[tauri::command]
async fn apply_edits(
    paths: Vec<String>,
    ops: Vec<Op>,
    options: Options,
    state: State<'_, AppState>,
) -> Res<Vec<FileResult>> {
    let results = spawn_blocking(move || {
        for p in &paths {
            if let Err(e) = fittle_core::edit::plan(p, &ops, &options) {
                return Err(match e {
                    EditError::Invalid(m) => format!("{p}: {m}"),
                    other => format!("{p}: {other}"),
                });
            }
        }
        Ok(paths
            .into_iter()
            .map(|p| match fittle_core::write::apply(&p, &ops, &options) {
                Ok(r) => FileResult {
                    path: p,
                    report: Some(r),
                    error: None,
                },
                Err(e) => FileResult {
                    path: p,
                    report: None,
                    error: Some(e.to_string()),
                },
            })
            .collect::<Vec<_>>())
    })
    .await
    .map_err(err)??;
    // Headers changed on disk; drop cached views.
    *state.session.lock().unwrap() = None;
    Ok(results)
}

/// Output name, size and estimate for an export of the open file.
#[tauri::command]
fn export_plan(
    spec: ExportSpec,
    template: String,
    state: State<'_, AppState>,
) -> Res<export::Plan> {
    let s = current(&state)?;
    Ok(export::plan(&s.info, &spec, &template))
}

/// What the export will look like (8-bit, long edge ≤ max_edge).
#[tauri::command]
async fn export_preview(
    spec: ExportSpec,
    max_edge: usize,
    state: State<'_, AppState>,
) -> Res<Response> {
    let s = current(&state)?;
    let t = std::time::Instant::now();
    let r = spawn_blocking(move || {
        let o = s.image.as_ref().ok_or("no image")?;
        let r = export::preview(o, &s.info, &spec, max_edge).map_err(err)?;
        let Samples::U8(data) = export::raster(&r.image, 8).samples else {
            unreachable!("8-bit raster")
        };
        Ok::<_, String>(Response::new(packed(
            r.image.width,
            r.image.height,
            r.image.planes,
            &data,
        )))
    })
    .await
    .map_err(err)?;
    trace("export_preview", t);
    r
}

/// Export the open file into `dir` (default: next to the source). Never
/// replaces an existing file.
#[tauri::command]
async fn export_image(
    spec: ExportSpec,
    template: String,
    dir: Option<String>,
    state: State<'_, AppState>,
) -> Res<export::Exported> {
    let s = current(&state)?;
    spawn_blocking(move || {
        let src = std::path::PathBuf::from(&s.path);
        let dir = dir
            .map(std::path::PathBuf::from)
            .or_else(|| src.parent().map(|p| p.to_path_buf()))
            .ok_or("no output folder")?;
        export::export(&src, &dir, Some(&template), &spec).map_err(err)
    })
    .await
    .map_err(err)?
}

/// fpack (or funpack) a file into a new file beside it. Never overwrites.
#[tauri::command]
async fn pack_file(path: String, unpack: bool) -> Res<fittle_image::fpack::PackReport> {
    use fittle_image::fpack;
    spawn_blocking(move || {
        let src = std::path::PathBuf::from(&path);
        if unpack {
            fpack::funpack(&src, &fpack::unpacked_name(&src)).map_err(err)
        } else {
            fpack::fpack(
                &src,
                &fpack::packed_name(&src),
                &fpack::PackOptions::default(),
            )
            .map_err(err)
        }
    })
    .await
    .map_err(err)?
}

/// Session report for a folder; `grade` also measures every light sub.
#[tauri::command]
async fn session_report(
    path: String,
    recursive: bool,
    grade: bool,
    rules: Option<String>,
    state: State<'_, AppState>,
) -> Res<Report> {
    let t = std::time::Instant::now();
    let r = spawn_blocking(move || {
        let rules = rules
            .as_deref()
            .map(fittle_scan::grade::Rules::parse)
            .transpose()?
            .unwrap_or_default();
        let entries = if recursive {
            fittle_scan::list_recursive(&path)
        } else {
            fittle_scan::list(&path)
        }
        .map_err(err)?;
        let r = fittle_scan::report::report(&path, &entries, grade.then_some(&rules));
        Ok::<_, String>(Arc::new((r, entries)))
    })
    .await
    .map_err(err)??;
    trace(
        if grade {
            "session_report (graded)"
        } else {
            "session_report"
        },
        t,
    );
    *state.report.lock().unwrap() = Some(r.clone());
    Ok(r.0.clone())
}

/// Save the last report into its folder (md, html, json or astrobin); a new file.
#[tauri::command]
async fn save_report(format: String, state: State<'_, AppState>) -> Res<String> {
    let r = state
        .report
        .lock()
        .unwrap()
        .clone()
        .ok_or("no report yet")?;
    spawn_blocking(move || {
        fittle_scan::report::save(&r.0, &r.1, &format)
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// Move files into `_rejected/` beside them (dry run returns the plan).
#[tauri::command]
async fn move_rejects(paths: Vec<String>, dry_run: bool) -> Res<Vec<fittle_scan::grade::Move>> {
    spawn_blocking(move || {
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        fittle_scan::grade::move_rejects(&refs, dry_run)
    })
    .await
    .map_err(err)
}

/// Match the lights in a folder to a calibration library (both recursive).
#[tauri::command]
async fn match_calibration(
    lights: String,
    library: String,
) -> Res<fittle_scan::calmatch::Matching> {
    spawn_blocking(move || {
        let l = fittle_scan::list_recursive(&lights).map_err(err)?;
        let c = fittle_scan::list_recursive(&library).map_err(err)?;
        Ok(fittle_scan::calmatch::match_calibration(
            &lights, &l, &library, &c,
        ))
    })
    .await
    .map_err(err)?
}

/// One blink frame. Binary: `[u32 w][u32 h][u32 4][u32 bottom_up]`, then
/// 9 × f32 stretch (shadows, midtones, highlights × 3 channels), then RGBA8.
/// `stf` (flat, 9 values) locks the stretch; omit it to auto-stretch.
#[tauri::command]
async fn blink_frame(path: String, max_edge: usize, stf: Option<Vec<f32>>) -> Res<Response> {
    let t = std::time::Instant::now();
    let r = spawn_blocking(move || {
        let locked: Option<Vec<fittle_image::stretch::Stf>> = stf.map(|v| {
            v.chunks_exact(3)
                .map(|c| fittle_image::stretch::Stf {
                    shadows: c[0],
                    midtones: c[1],
                    highlights: c[2],
                })
                .collect()
        });
        let f = fittle_image::thumb::blink_frame(&path, max_edge, locked.as_deref())
            .ok_or("could not read this file")?;
        let mut out = Vec::with_capacity(16 + 36 + f.rgba.len());
        for v in [f.width, f.height, 4, usize::from(f.bottom_up)] {
            out.extend((v as u32).to_le_bytes());
        }
        for i in 0..3 {
            let s = f.stf[i.min(f.stf.len() - 1)];
            for v in [s.shadows, s.midtones, s.highlights] {
                out.extend(v.to_le_bytes());
            }
        }
        out.extend_from_slice(&f.rgba);
        Ok::<_, String>(Response::new(out))
    })
    .await
    .map_err(err)?;
    trace("blink_frame", t);
    r
}

/// Star metrics for one sub (grade card when no report is loaded).
#[tauri::command]
async fn sub_stats(path: String) -> Res<fittle_image::stars::FrameStats> {
    spawn_blocking(move || {
        fittle_image::stars::measure_file(std::path::Path::new(&path))
            .map(|(_, s)| s)
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// How each keyword varies across the selected files.
#[tauri::command]
async fn keyword_spread(paths: Vec<String>) -> Res<fittle_scan::batch::Distribution> {
    spawn_blocking(move || {
        let p: Vec<std::path::PathBuf> = paths.into_iter().map(Into::into).collect();
        fittle_scan::batch::distribution(&p)
    })
    .await
    .map_err(err)
}

/// Dry run of edits over many files.
#[tauri::command]
async fn batch_plan(
    paths: Vec<String>,
    ops: Vec<Op>,
    options: Options,
) -> Res<fittle_scan::batch::BatchPlan> {
    spawn_blocking(move || {
        let p: Vec<std::path::PathBuf> = paths.into_iter().map(Into::into).collect();
        fittle_scan::batch::plan_batch(&p, &ops, &options)
    })
    .await
    .map_err(err)
}

/// Apply edits to many files (validated first); returns per-file errors.
#[tauri::command]
async fn batch_apply(
    paths: Vec<String>,
    ops: Vec<Op>,
    options: Options,
) -> Res<Vec<(String, String)>> {
    spawn_blocking(move || {
        let p: Vec<std::path::PathBuf> = paths.into_iter().map(Into::into).collect();
        fittle_scan::batch::apply_batch(&p, &ops, &options)
    })
    .await
    .map_err(err)
}

/// Built-in and saved rig profiles.
#[tauri::command]
fn rigs_list() -> Vec<fittle_core::rigs::Rig> {
    fittle_core::rigs::all()
}

/// Save a rig from a file's header (default keys when `keys` is empty).
#[tauri::command]
async fn rig_save(name: String, from: String, keys: Vec<String>) -> Res<fittle_core::rigs::Rig> {
    spawn_blocking(move || {
        let fits = Fits::open(&from).map_err(err)?;
        let h = fits.hdus[fittle_core::edit::default_hdu(&fits)].header();
        let keys: Vec<&str> = if keys.is_empty() {
            fittle_core::rigs::RIG_KEYS.to_vec()
        } else {
            keys.iter().map(String::as_str).collect()
        };
        let rig = fittle_core::rigs::from_header(&name, h, &keys);
        fittle_core::rigs::save(rig.clone())?;
        Ok(rig)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn rig_delete(name: String) -> Res<bool> {
    fittle_core::rigs::delete(&name)
}

/// The privacy-scrub edits for a file, to stage in the editor.
#[tauri::command]
async fn scrub_ops(path: String) -> Res<Vec<Op>> {
    spawn_blocking(move || {
        let fits = Fits::open(&path).map_err(err)?;
        Ok(fittle_core::privacy::scrub_ops(&fits, &path))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn dictionary() -> &'static [KeywordInfo] {
    dict::KEYWORDS
}

/// UI-side timing lines for `FITTLE_TRACE`.
#[tauri::command]
fn log(msg: String) {
    if std::env::var_os("FITTLE_TRACE").is_some() {
        eprintln!("[fittle:ui] {msg}");
    }
}

#[derive(Serialize)]
struct Launch {
    path: String,
    dir: bool,
}

/// File or folder passed on the command line (`fittle-app <path>`, later `fittle view`).
#[tauri::command]
fn initial_path() -> Option<Launch> {
    let arg = std::env::args().skip(1).find(|a| !a.starts_with('-'))?;
    let p = std::path::Path::new(&arg);
    let abs = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    Some(Launch {
        dir: abs.is_dir(),
        path: abs.to_string_lossy().to_string(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_file,
            display,
            preview,
            region,
            readout,
            list_folder,
            thumbnail,
            header,
            dictionary,
            initial_path,
            log,
            plan_edit,
            apply_edits,
            scrub_ops,
            export_plan,
            export_preview,
            export_image,
            pack_file,
            session_report,
            save_report,
            move_rejects,
            match_calibration,
            blink_frame,
            sub_stats,
            keyword_spread,
            batch_plan,
            batch_apply,
            rigs_list,
            rig_save,
            rig_delete
        ])
        .run(tauri::generate_context!())
        .expect("error while running Fittle");
}
