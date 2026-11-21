use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use core_engine::{EngineCommand, EngineSnapshot};
use protocol::{ProjectHeader, ProjectModel, SongHeader, SongModel};

use crate::{now_ms, AppState};

use super::*;

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

pub(super) fn relink_everywhere(app: &tauri::AppHandle, map: &std::collections::BTreeMap<String, String>) -> anyhow::Result<()> {
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
