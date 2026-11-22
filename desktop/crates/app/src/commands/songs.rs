use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use core_engine::EngineCommand;
use protocol::model::{new_id, Song, SongVersion};

use crate::{now_ms, AppState};

#[tauri::command]
pub fn list_songs(state: tauri::State<AppState>, include_archived: Option<bool>) -> Result<Vec<Song>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_library(include_archived.unwrap_or(false)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_song(state: tauri::State<AppState>, id: String) -> Result<Song, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.song(&id).map_err(|e| e.to_string())?.ok_or_else(|| format!("Song {id} not found"))
}

#[tauri::command]
pub fn get_song_version(state: tauri::State<AppState>, id: String) -> Result<SongVersion, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.song_version(&id).map_err(|e| e.to_string())?.ok_or_else(|| format!("Song version {id} not found"))
}

#[tauri::command]
pub fn create_song(state: tauri::State<AppState>, mut song: Song) -> Result<Song, String> {
    song.id = new_id();
    song.current.id = new_id();
    song.current.number = 1;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.insert_song(&song, now_ms()).map_err(|e| e.to_string())?;
    Ok(song)
}

#[tauri::command]
pub fn update_song(state: tauri::State<AppState>, song: Song) -> Result<Song, String> {
    let updated = state.db.lock().map_err(|e| e.to_string())?.save_song(&song, now_ms()).map_err(|e| e.to_string())?;
    state.send(EngineCommand::RefreshService)?;
    Ok(updated)
}

#[tauri::command]
pub fn archive_song(state: tauri::State<AppState>, id: String, archived: bool) -> Result<(), String> {
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let mut song = db.song(&id).map_err(|e| e.to_string())?.ok_or_else(|| format!("Song {id} not found"))?;
        song.archived = archived;
        db.save_song(&song, now_ms()).map_err(|e| e.to_string())?;
    }
    state.send(EngineCommand::RefreshService)
}

#[derive(serde::Serialize)]
pub struct SongImport { imported: Vec<Song>, failed: Vec<String> }

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
                app.state::<AppState>().db.lock().map_err(|e| e.to_string())?.insert_song(&song, now_ms()).map_err(|e| e.to_string())?;
                result.imported.push(song);
            }
            Err(e) => result.failed.push(format!("{name}: {e:#}")),
        }
    }
    Ok(result)
}
