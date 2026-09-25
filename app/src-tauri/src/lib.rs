//! Tauri shell. Commands are thin adapters over `fittle-*` crates; no FITS
//! logic lives here (CLAUDE.md rule 3).

use std::sync::Mutex;

use fittle_core::{Fits, HEADER_SCHEMA, HeaderDoc};
use fittle_image::PreviewInfo;
use tauri::State;
use tauri::ipc::Response;

/// Pixels of the most recent preview, handed to the webview as raw bytes by
/// `preview_pixels` so they never pass through JSON.
#[derive(Default)]
struct PendingPixels(Mutex<Option<Vec<u8>>>);

/// `fittle header --json` for one file.
#[tauri::command]
async fn header(path: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let fits = Fits::open(&path).map_err(|e| e.to_string())?;
        let doc = HeaderDoc {
            schema: HEADER_SCHEMA,
            path: &path,
            fits: &fits,
        };
        serde_json::to_value(doc).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Decode the first image HDU into a display preview. Returns its metadata and
/// stages the pixels for `preview_pixels`.
#[tauri::command]
async fn preview_info(
    path: String,
    max_edge: usize,
    pending: State<'_, PendingPixels>,
) -> Result<PreviewInfo, String> {
    let (info, bytes) = tauri::async_runtime::spawn_blocking(move || {
        let fits = Fits::open(&path).map_err(|e| e.to_string())?;
        let hdu = fits
            .hdus
            .iter()
            .find(|h| !h.shape.is_empty())
            .ok_or("file has no image HDU")?;
        let preview = fittle_image::preview(&path, hdu, max_edge).map_err(|e| e.to_string())?;
        let bytes: Vec<u8> = preview
            .pixels
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        Ok::<_, String>((preview.info, bytes))
    })
    .await
    .map_err(|e| e.to_string())??;
    *pending.0.lock().unwrap() = Some(bytes);
    Ok(info)
}

#[tauri::command]
fn preview_pixels(pending: State<'_, PendingPixels>) -> Result<Response, String> {
    let bytes = pending
        .0
        .lock()
        .unwrap()
        .take()
        .ok_or("no preview pending")?;
    Ok(Response::new(bytes))
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
        .manage(PendingPixels::default())
        .invoke_handler(tauri::generate_handler![
            header,
            preview_info,
            preview_pixels,
            initial_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running Fittle");
}
