//! Operator UI commands: engine control, the song and service library, and
//! media files. Output and display commands live in `displays`, settings in
//! `settings`.

use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use core_engine::{EngineCommand, EngineSnapshot};
use protocol::{ProjectHeader, ProjectModel, SongHeader, SongModel};

use crate::{now_ms, AppState};

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

#[tauri::command]
pub fn resolve_media_path(app: tauri::AppHandle, path: String) -> Result<String, String> {
    let file = std::fs::canonicalize(&path).map_err(|e| format!("Cannot open media: {e}"))?;
    if !file.is_file() { return Err("Media must be a file".into()); }
    app.asset_protocol_scope().allow_file(&file).map_err(|e| e.to_string())?;
    Ok(file.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn video_thumbnail(app: tauri::AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::hash::{Hash, Hasher};
        let file = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        file.hash(&mut hash); metadata.len().hash(&mut hash); metadata.modified().ok().hash(&mut hash);
        let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("video-thumbnails");
        std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        let poster = cache.join(format!("{:x}.jpg", hash.finish()));
        if !poster.exists() {
            // Decoded in-process: packaged builds ship FFmpeg libraries, not its CLI.
            let frame = media_engine::poster(&file.to_string_lossy(), 640).map_err(|e| format!("Thumbnail: {e:#}"))?;
            let temporary = cache.join(format!("{}.jpg", uuid::Uuid::new_v4()));
            image::save_buffer_with_format(&temporary, &frame.rgb, frame.width, frame.height, image::ColorType::Rgb8, image::ImageFormat::Jpeg).map_err(|e| e.to_string())?;
            std::fs::rename(&temporary, &poster).map_err(|e| e.to_string())?;
        }
        app.asset_protocol_scope().allow_file(&poster).map_err(|e| e.to_string())?;
        Ok(poster.to_string_lossy().into_owned())
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn choose_presentation_images(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Choose presentation images").add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp"]).pick_files(move |files| { let _ = sender.send(files); });
    let files = receiver.await.map_err(|e| e.to_string())?.unwrap_or_default();
    files.into_iter().map(|file| {
        let path = file.into_path().map_err(|e| e.to_string())?;
        app.asset_protocol_scope().allow_file(&path).map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    }).collect()
}

#[tauri::command]
pub fn play(state: tauri::State<AppState>) -> Result<(), String> {
    state.send(EngineCommand::Play)
}

#[tauri::command]
pub fn pause(state: tauri::State<AppState>) -> Result<(), String> {
    state.send(EngineCommand::Pause)
}

#[tauri::command]
pub fn stop(state: tauri::State<AppState>) -> Result<(), String> {
    state.send(EngineCommand::Stop)
}

#[tauri::command]
pub fn set_active_text(state: tauri::State<AppState>, text: String) -> Result<(), String> {
    state.send(EngineCommand::SetActiveText { text })
}

#[tauri::command]
pub fn set_display(state: tauri::State<AppState>, display: protocol::DisplayMode) -> Result<(), String> {
    state.send(EngineCommand::SetDisplay { display })
}

#[tauri::command]
pub fn set_background(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    state.send(EngineCommand::SetBackground { path })
}

#[tauri::command]
pub fn nav_next(state: tauri::State<AppState>) -> Result<(), String> {
    state.send(EngineCommand::Next)
}

#[tauri::command]
pub fn nav_prev(state: tauri::State<AppState>) -> Result<(), String> {
    state.send(EngineCommand::Prev)
}

#[tauri::command]
pub fn cue(state: tauri::State<AppState>, item_id: i64, index: u32) -> Result<(), String> {
    state.send(EngineCommand::Cue { item_id, index })
}

/// Any typed engine command; local controls use the same vocabulary as
/// WebSocket clients.
#[tauri::command]
pub fn engine_command(state: tauri::State<AppState>, command: EngineCommand) -> Result<(), String> {
    state.send(command)
}

#[tauri::command]
pub fn go_to_item(state: tauri::State<AppState>, item_id: i64) -> Result<(), String> {
    state.send(EngineCommand::GoToItem { item_id })
}

#[tauri::command]
pub fn go_to_step(state: tauri::State<AppState>, index: u32) -> Result<(), String> {
    state.send(EngineCommand::GoToStep { index })
}

#[tauri::command]
pub fn seek(state: tauri::State<AppState>, time_ms: i64) -> Result<(), String> {
    state.send(EngineCommand::Seek { time_ms })
}

#[tauri::command]
pub fn get_engine_snapshot(state: tauri::State<AppState>) -> EngineSnapshot {
    state.snapshot.read().expect("snapshot lock poisoned").clone()
}

#[tauri::command]
pub fn list_projects(state: tauri::State<AppState>) -> Result<Vec<ProjectHeader>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_projects().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_project(state: tauri::State<AppState>, name: String) -> Result<ProjectModel, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.create_project(name, now_ms()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_project(state: tauri::State<AppState>, project_id: i64) -> Result<ProjectModel, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_project(project_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_project(state: tauri::State<AppState>, project_id: i64) -> Result<(), String> {
    state.send(EngineCommand::LoadProject { project_id })
}

#[tauri::command]
pub fn save_project(state: tauri::State<AppState>, project: ProjectModel) -> Result<(), String> {
    state.send(EngineCommand::SaveProject { project })
}

#[tauri::command]
pub fn list_songs(state: tauri::State<AppState>) -> Result<Vec<SongHeader>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_songs().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_song(state: tauri::State<AppState>, title: String, lyrics: String) -> Result<SongModel, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.create_song(title, lyrics).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_song(state: tauri::State<AppState>, id: i64) -> Result<SongModel, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_song(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_song(state: tauri::State<AppState>, song: SongModel) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_song(&song).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn choose_media_file(app: tauri::AppHandle, kind: String) -> Result<Option<String>, String> {
    let extensions: &[&str] = match kind.as_str() {
        "image" => &["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        "audio" => &["mp3", "wav", "ogg", "flac", "m4a", "aac"],
        "video" => &["mp4", "mov", "mkv", "webm", "avi"],
        "logo" => &["png", "jpg", "jpeg", "webp", "gif", "bmp", "mp4", "m4v", "mov", "mkv", "webm", "avi"],
        _ => &["png", "jpg", "jpeg", "webp", "mp4", "mov", "mkv", "webm"],
    };
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Choose media").add_filter("Media files", extensions).pick_file(move |file| { let _ = sender.send(file); });
    receiver.await.map_err(|e| e.to_string())?.map(|file| file.into_path().map(|path| path.to_string_lossy().into_owned()).map_err(|e| e.to_string())).transpose()
}

#[tauri::command]
pub fn list_media(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<Vec<protocol::MediaModel>, String> {
    let media = state.db.lock().map_err(|e| e.to_string())?.list_media().map_err(|e| e.to_string())?;
    for item in &media { app.asset_protocol_scope().allow_file(&item.path).map_err(|e| e.to_string())?; }
    Ok(media)
}

#[tauri::command]
pub fn create_media(state: tauri::State<AppState>, app: tauri::AppHandle, title: String, kind: protocol::MediaKind, path: String) -> Result<protocol::MediaModel, String> {
    let file = std::fs::canonicalize(path.trim()).map_err(|e| format!("Cannot open media file: {e}"))?;
    if !file.is_file() { return Err("Choose a media file, not a directory.".into()); }
    if title.trim().is_empty() { return Err("Enter a media title.".into()); }
    app.asset_protocol_scope().allow_file(&file).map_err(|e| e.to_string())?;
    state.db.lock().map_err(|e| e.to_string())?.create_media(title.trim().into(), kind, file.to_string_lossy().into()).map_err(|e| e.to_string())
}
