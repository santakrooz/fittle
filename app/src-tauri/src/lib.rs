//! Tauri shell. Commands are thin adapters over `fittle-*` crates; no FITS
//! logic lives here (CLAUDE.md rule 3). Pixel payloads are binary:
//! `[u32 width][u32 height][u32 channels]` little-endian, then the data.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fittle_core::dict::{self, KeywordInfo};
use fittle_core::{Fits, HEADER_SCHEMA, HeaderDoc};
use fittle_image::view::{Display, OpenedInfo};
use fittle_image::{Mode, Readout, ViewSession};
use fittle_scan::Entry;
use serde::Serialize;
use tauri::State;
use tauri::async_runtime::spawn_blocking;
use tauri::ipc::Response;

#[derive(Default)]
struct AppState {
    session: Mutex<Option<Arc<ViewSession>>>,
    thumbs: Mutex<HashMap<String, Arc<Vec<u8>>>>,
}

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

/// File passed on the command line (`fittle-app <file>`, later `fittle view`).
#[tauri::command]
fn initial_path() -> Option<String> {
    std::env::args().skip(1).find(|a| !a.starts_with('-'))
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
            log
        ])
        .run(tauri::generate_context!())
        .expect("error while running Fittle");
}
