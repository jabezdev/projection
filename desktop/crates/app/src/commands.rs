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
    let paths = files.into_iter().map(|file| file.into_path().map_err(|e| e.to_string())).collect::<Result<Vec<_>, _>>()?;
    let mut imported = Vec::with_capacity(paths.len());
    for path in paths {
        imported.push(import_into_library(&app, path).await?);
    }
    Ok(imported)
}

/// The PDF chosen for import, read back by `read_chosen_pdf`.
#[derive(Default)]
pub struct ChosenPdf(std::sync::Mutex<Option<std::path::PathBuf>>);

/// Asks for a PDF to import as slides; returns its file name.
#[tauri::command]
pub async fn choose_pdf(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Import a PDF as slides").add_filter("PDF", &["pdf"]).pick_file(move |file| { let _ = sender.send(file); });
    let Some(file) = receiver.await.map_err(|e| e.to_string())? else { return Ok(None) };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    *app.state::<ChosenPdf>().0.lock().map_err(|e| e.to_string())? = Some(path);
    Ok(Some(name))
}

/// The chosen PDF's bytes, for the operator UI to render.
#[tauri::command]
pub async fn read_chosen_pdf(app: tauri::AppHandle) -> Result<tauri::ipc::Response, String> {
    let path = app.state::<ChosenPdf>().0.lock().map_err(|e| e.to_string())?.clone().ok_or("No PDF was chosen")?;
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(&path).map_err(|e| format!("Reading {}: {e}", path.display())))
        .await.map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Saves one rendered slide (PNG bytes as the request body, its file name in
/// the `x-slide-name` header) into the media folder; returns its path.
#[tauri::command]
pub async fn save_slide_image(app: tauri::AppHandle, request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("Send the slide as raw bytes".into()) };
    if !bytes.starts_with(b"\x89PNG") {
        return Err("The slide is not a PNG".into());
    }
    let name = request.headers().get("x-slide-name").and_then(|v| v.to_str().ok()).unwrap_or("slide.png").to_string();
    let (bytes, data_dir) = (bytes.clone(), app.state::<DataDir>().0.clone());
    let path = tauri::async_runtime::spawn_blocking(move || crate::library::import_bytes(&data_dir, &name, &bytes).map_err(|e| format!("Saving the slide: {e}")))
        .await.map_err(|e| e.to_string())??;
    app.asset_protocol_scope().allow_file(&path).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Copies a chosen file into the library's media folder (off the UI thread)
/// and lets the operator UI load it.
async fn import_into_library(app: &tauri::AppHandle, path: std::path::PathBuf) -> Result<String, String> {
    let data_dir = app.state::<DataDir>().0.clone();
    let copy = tauri::async_runtime::spawn_blocking(move || crate::library::import_file(&data_dir, &path).map_err(|e| format!("Copying {} into the library: {e}", path.display())))
        .await.map_err(|e| e.to_string())??;
    app.asset_protocol_scope().allow_file(&copy).map_err(|e| e.to_string())?;
    Ok(copy.to_string_lossy().into_owned())
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
    let Some(file) = receiver.await.map_err(|e| e.to_string())? else { return Ok(None) };
    let path = file.into_path().map_err(|e| e.to_string())?;
    // The logo goes straight into the settings, so it is copied now; other
    // media is copied when imported (`create_media`).
    if kind == "logo" {
        return import_into_library(&app, path).await.map(Some);
    }
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn list_media(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<Vec<protocol::MediaModel>, String> {
    let media = state.db.lock().map_err(|e| e.to_string())?.list_media().map_err(|e| e.to_string())?;
    for item in &media { app.asset_protocol_scope().allow_file(&item.path).map_err(|e| e.to_string())?; }
    Ok(media)
}

#[tauri::command]
pub async fn create_media(app: tauri::AppHandle, title: String, kind: protocol::MediaKind, path: String) -> Result<protocol::MediaModel, String> {
    let file = std::fs::canonicalize(path.trim()).map_err(|e| format!("Cannot open media file: {e}"))?;
    if !file.is_file() { return Err("Choose a media file, not a directory.".into()); }
    if title.trim().is_empty() { return Err("Enter a media title.".into()); }
    let copy = import_into_library(&app, file).await?;
    let db = app.state::<AppState>().db.clone();
    let created = db.lock().map_err(|e| e.to_string())?.create_media(title.trim().into(), kind, copy);
    created.map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct MissingMedia { path: String, title: Option<String>, services: Vec<String> }

/// Media the library or a service uses whose file is gone.
#[tauri::command]
pub fn list_missing_media(state: tauri::State<AppState>, output: tauri::State<crate::settings::OutputState>) -> Result<Vec<MissingMedia>, String> {
    let logo = output.config.lock().map(|c| c.logo_path.clone()).unwrap_or_default();
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let media = crate::backup::media_list(&db, &logo).map_err(|e| e.to_string())?;
    Ok(media.into_iter().filter(|m| !m.exists).map(|m| MissingMedia { path: m.path, title: m.title, services: m.services }).collect())
}

#[derive(serde::Serialize)]
pub struct Relinked { found: usize, missing: usize }

/// Asks for a folder, finds missing media there by file name and points the
/// library, every service, library songs and the logo at the found files.
/// `None` when cancelled.
#[tauri::command]
pub async fn relink_media(app: tauri::AppHandle) -> Result<Option<Relinked>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Choose a folder to search for missing media").pick_folder(move |folder| { let _ = sender.send(folder); });
    let Some(folder) = receiver.await.map_err(|e| e.to_string())? else { return Ok(None) };
    let folder = folder.into_path().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let missing: Vec<String> = list_missing_media(app.state(), app.state())?.into_iter().map(|m| m.path).collect();
        let found = crate::library::search(&folder, &missing);
        relink_everywhere(&app, &found).map_err(|e| format!("Relinking media: {e:#}"))?;
        log::info!("Relinked {} of {} missing media files from {}", found.len(), missing.len(), folder.display());
        Ok(Some(Relinked { found: found.len(), missing: missing.len() - found.len() }))
    }).await.map_err(|e| e.to_string())?
}

fn relink_everywhere(app: &tauri::AppHandle, map: &std::collections::BTreeMap<String, String>) -> anyhow::Result<()> {
    if map.is_empty() {
        return Ok(());
    }
    let state = app.state::<AppState>();
    let loaded = state.snapshot.read().map_err(|_| anyhow::anyhow!("snapshot lock poisoned"))?.project.as_ref().map(|p| p.id);
    {
        let db = state.db.lock()?;
        for (old, new) in map {
            db.relink_media(old, new)?;
            app.asset_protocol_scope().allow_file(new)?;
        }
        for header in db.list_projects()? {
            let project = db.get_project(header.id)?;
            let mut value = serde_json::to_value(&project)?;
            if !crate::library::replace_paths(&mut value, map) {
                continue;
            }
            let mut project: ProjectModel = serde_json::from_value(value)?;
            if Some(project.id) == loaded {
                // The engine owns the loaded service; it saves and re-resolves media.
                state.send(EngineCommand::SaveProject { project }).map_err(anyhow::Error::msg)?;
            } else {
                db.save_project(&mut project, now_ms())?;
            }
        }
        for header in db.list_songs()? {
            let mut song = db.get_song(header.id)?;
            let mut lyrics = serde_json::Value::String(song.lyrics.clone());
            if crate::library::replace_paths(&mut lyrics, map) {
                song.lyrics = lyrics.as_str().unwrap_or_default().to_string();
                db.update_song(&song)?;
            }
        }
    }
    let output = app.state::<crate::settings::OutputState>();
    let config = output.config.lock().map_err(|_| anyhow::anyhow!("settings lock poisoned"))?.clone();
    if let Some(logo) = map.get(&config.logo_path) {
        crate::settings::set_output_state(output, app.clone(), "config".into(), protocol::OutputConfig { logo_path: logo.clone(), ..config }).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

#[derive(serde::Serialize)]
pub struct SongImport { imported: Vec<SongHeader>, failed: Vec<String> }

/// Asks for OpenLyrics XML files and adds each as a library song.
#[tauri::command]
pub async fn import_openlyrics(app: tauri::AppHandle) -> Result<SongImport, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Import OpenLyrics songs").add_filter("OpenLyrics", &["xml"]).pick_files(move |files| { let _ = sender.send(files); });
    let files = receiver.await.map_err(|e| e.to_string())?.unwrap_or_default();
    let mut result = SongImport { imported: Vec::new(), failed: Vec::new() };
    for file in files {
        let path = file.into_path().map_err(|e| e.to_string())?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let parsed = std::fs::read_to_string(&path).map_err(anyhow::Error::from).and_then(|xml| core_engine::openlyrics::parse(&xml));
        match parsed {
            Ok(song) => {
                let created = app.state::<AppState>().db.lock().map_err(|e| e.to_string())?.create_song(song.title, song.document).map_err(|e| e.to_string())?;
                result.imported.push(SongHeader { id: created.id, title: created.title });
            }
            Err(e) => result.failed.push(format!("{name}: {e:#}")),
        }
    }
    Ok(result)
}
