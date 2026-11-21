use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use protocol::model::{new_id, MediaFile, MediaKind};

use crate::AppState;

use super::*;

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

pub(super) async fn import_into_library(app: &tauri::AppHandle, path: std::path::PathBuf) -> Result<String, String> {
    let data_dir = app.state::<DataDir>().0.clone();
    let copy = tauri::async_runtime::spawn_blocking(move || crate::library::import_file(&data_dir, &path).map_err(|e| format!("Copying {} into the library: {e}", path.display())))
        .await.map_err(|e| e.to_string())??;
    app.asset_protocol_scope().allow_file(&copy).map_err(|e| e.to_string())?;
    Ok(copy.to_string_lossy().into_owned())
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
    if kind == "logo" {
        return import_into_library(&app, path).await.map(Some);
    }
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn list_media(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<Vec<MediaFile>, String> {
    let media = state.db.lock().map_err(|e| e.to_string())?.list_media_files().map_err(|e| e.to_string())?;
    for path in media.iter().filter_map(|m| m.path.as_ref()) { app.asset_protocol_scope().allow_file(path).map_err(|e| e.to_string())?; }
    Ok(media)
}

#[tauri::command]
pub async fn create_media(app: tauri::AppHandle, title: String, kind: MediaKind, path: String) -> Result<MediaFile, String> {
    let file = std::fs::canonicalize(path.trim()).map_err(|e| format!("Cannot open media file: {e}"))?;
    if !file.is_file() { return Err("Choose a media file, not a directory.".into()); }
    if title.trim().is_empty() { return Err("Enter a media title.".into()); }
    let copy = import_into_library(&app, file).await?;
    add_media(&app, title.trim().into(), kind, copy)
}

pub(super) fn add_media(app: &tauri::AppHandle, title: String, kind: MediaKind, path: String) -> Result<MediaFile, String> {
    let db = app.state::<AppState>().db.clone();
    let db = db.lock().map_err(|e| e.to_string())?;
    if let Some(existing) = db.media_file_at(&path).map_err(|e| e.to_string())? {
        return Ok(existing);
    }
    let media = MediaFile { id: new_id(), title, kind, hash: None, path: Some(path) };
    db.save_media_file(&media).map_err(|e| e.to_string())?;
    Ok(media)
}

#[tauri::command]
pub fn delete_media(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_media_file(&id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn rename_media(state: tauri::State<AppState>, id: String, title: String) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() { return Err("Enter a title.".into()); }
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let mut media = db.media_file(&id).map_err(|e| e.to_string())?.ok_or_else(|| format!("Media {id} not found"))?;
    media.title = title.into();
    db.save_media_file(&media).map_err(|e| e.to_string())
}

pub fn media_kind_of(path: &std::path::Path) -> Option<MediaKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => Some(MediaKind::Image),
        "mp4" | "m4v" | "mov" | "mkv" | "webm" | "avi" => Some(MediaKind::Video),
        "mp3" | "wav" | "ogg" | "flac" | "m4a" | "aac" => Some(MediaKind::Audio),
        _ => None,
    }
}

#[tauri::command]
pub async fn import_media_files(app: tauri::AppHandle) -> Result<Vec<MediaFile>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Import media")
        .add_filter("Images, videos and audio", &["png", "jpg", "jpeg", "webp", "gif", "bmp", "mp4", "m4v", "mov", "mkv", "webm", "avi", "mp3", "wav", "ogg", "flac", "m4a", "aac"])
        .pick_files(move |files| { let _ = sender.send(files); });
    let Some(files) = receiver.await.map_err(|e| e.to_string())? else { return Ok(Vec::new()) };
    let mut imported = Vec::new();
    for file in files {
        let path = file.into_path().map_err(|e| e.to_string())?;
        let Some(kind) = media_kind_of(&path) else { continue };
        let title = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        let copy = import_into_library(&app, path).await?;
        app.asset_protocol_scope().allow_file(&copy).map_err(|e| e.to_string())?;
        imported.push(add_media(&app, title, kind, copy)?);
    }
    Ok(imported)
}
