use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use core_engine::{EngineCommand, EngineSnapshot};
use protocol::{ProjectHeader, ProjectModel, SongHeader, SongModel};

use crate::{now_ms, AppState};

use super::*;

/// Problems found while starting, shown to the operator once the UI loads.
pub struct StartupWarnings(pub Vec<String>);

#[tauri::command]
pub fn get_startup_warnings(warnings: tauri::State<StartupWarnings>) -> Vec<String> {
    warnings.0.clone()
}

/// Opens the log folder in the system file manager.
#[tauri::command]
pub fn open_logs_folder() -> Result<(), String> {
    let dir = crate::logging::logs_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    reveal(&dir)
}

/// Opens `dir` in Explorer, Finder or the desktop's file manager.
pub fn reveal(dir: &std::path::Path) -> Result<(), String> {
    let program = if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(program).arg(dir).spawn().map(drop).map_err(|e| format!("Cannot open {}: {e}", dir.display()))
}

/// The data folder in use (library, settings, logs, backups).
pub struct DataDir(pub std::path::PathBuf);

#[derive(serde::Serialize)]
pub struct AppInfo { version: &'static str, data_dir: String, last_backup_ms: Option<i64> }

#[tauri::command]
pub fn get_app_info(data: tauri::State<DataDir>) -> AppInfo {
    let last = crate::backup::last_automatic(&data.0).and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok());
    AppInfo { version: env!("CARGO_PKG_VERSION"), data_dir: data.0.to_string_lossy().into_owned(), last_backup_ms: last.map(|d| d.as_millis() as i64) }
}

/// Asks where to save, then writes a backup zip there. `None` when cancelled.
#[tauri::command]
pub async fn export_backup(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Export backup").add_filter("Zip archive", &["zip"]).set_file_name(crate::backup::export_name()).save_file(move |file| { let _ = sender.send(file); });
    let Some(file) = receiver.await.map_err(|e| e.to_string())? else { return Ok(None) };
    let dest = file.into_path().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let data_dir = app.state::<DataDir>().0.clone();
        let logo = app.state::<crate::settings::OutputState>().config.lock().map(|c| c.logo_path.clone()).unwrap_or_default();
        let db = app.state::<AppState>().db.clone();
        let db = db.lock().map_err(|e| e.to_string())?;
        crate::backup::write(&db, &data_dir, &logo, &dest).map_err(|e| format!("Backup failed: {e:#}"))?;
        log::info!("Exported a backup to {}", dest.display());
        Ok(Some(dest.to_string_lossy().into_owned()))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn open_backups_folder(data: tauri::State<DataDir>) -> Result<(), String> {
    let dir = data.0.join(crate::backup::FOLDER);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    reveal(&dir)
}
