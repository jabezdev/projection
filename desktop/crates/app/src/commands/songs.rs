use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use protocol::{SongHeader, SongModel};

use crate::AppState;


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
/// Deletes a library song; services keep their own copies of it.
#[tauri::command]
pub fn delete_song(state: tauri::State<AppState>, id: i64) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_song(id).map_err(|e| e.to_string())?;
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
