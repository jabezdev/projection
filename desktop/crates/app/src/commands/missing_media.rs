use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use core_engine::EngineCommand;
use protocol::model::ItemContent;

use crate::{now_ms, AppState};
use crate::system::backup;

#[derive(serde::Serialize)]
pub struct MissingMedia { path: String, title: Option<String>, services: Vec<String> }

#[tauri::command]
pub fn list_missing_media(state: tauri::State<AppState>, output: tauri::State<crate::settings::OutputState>) -> Result<Vec<MissingMedia>, String> {
    let logo = output.config.lock().map(|c| c.logo_path.clone()).unwrap_or_default();
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let media = backup::media_list(&db, &logo).map_err(|e| e.to_string())?;
    Ok(media.into_iter().filter(|m| !m.exists).map(|m| MissingMedia { path: m.path, title: m.title, services: m.services }).collect())
}

#[derive(serde::Serialize)]
pub struct Relinked { found: usize, missing: usize }

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
    let loaded = state.snapshot.read().map_err(|_| anyhow::anyhow!("snapshot lock poisoned"))?.service.as_ref().map(|s| s.id().to_string());
    {
        let db = state.db.lock()?;
        for (old, new) in map {
            db.relink_media_path(old, new)?;
            app.asset_protocol_scope().allow_file(new)?;
        }
        for mut service in db.list_services()? {
            let mut changed = false;
            for item in &mut service.items {
                let ItemContent::Presentation { slides, .. } = &mut item.content else { continue };
                for slide in slides.iter_mut() {
                    if let Some(new) = map.get(slide.as_str()) {
                        *slide = new.clone();
                        changed = true;
                    }
                }
            }
            if !changed {
                continue;
            }
            if loaded.as_deref() == Some(service.id.as_str()) {
                state.send(EngineCommand::SaveService { service }).map_err(anyhow::Error::msg)?;
            } else {
                db.save_service(&service, now_ms())?;
            }
        }
    }
    state.send(EngineCommand::RefreshService).map_err(anyhow::Error::msg)?;
    let output = app.state::<crate::settings::OutputState>();
    let config = output.config.lock().map_err(|_| anyhow::anyhow!("settings lock poisoned"))?.clone();
    if let Some(logo) = map.get(&config.logo_path) {
        crate::settings::set_output_state(output, app.clone(), "config".into(), protocol::OutputConfig { logo_path: logo.clone(), ..config }).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}
