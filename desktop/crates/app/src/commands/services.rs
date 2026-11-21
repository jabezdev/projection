use core_engine::EngineCommand;
use protocol::model::{new_id, Service};

use crate::{now_ms, AppState};

#[tauri::command]
pub fn list_services(state: tauri::State<AppState>) -> Result<Vec<Service>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_services().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_service(state: tauri::State<AppState>, title: String, scheduled_at_ms: i64) -> Result<Service, String> {
    let title = Some(title.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| "Service".into());
    let service = Service { id: new_id(), scheduled_at_ms, title, leader: None, notes: None, ended_at_ms: None, items: Vec::new() };
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.save_service(&service, now_ms()).map_err(|e| e.to_string())?;
    Ok(service)
}

#[tauri::command]
pub fn get_service(state: tauri::State<AppState>, service_id: String) -> Result<Service, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.service(&service_id).map_err(|e| e.to_string())?.ok_or_else(|| format!("Service {service_id} not found"))
}

#[tauri::command]
pub fn load_service(state: tauri::State<AppState>, service_id: String) -> Result<(), String> {
    state.send(EngineCommand::LoadService { service_id })
}

#[tauri::command]
pub fn save_service(state: tauri::State<AppState>, service: Service) -> Result<(), String> {
    state.send(EngineCommand::SaveService { service })
}

#[tauri::command]
pub fn end_service(state: tauri::State<AppState>, service_id: String) -> Result<(), String> {
    state.db.lock().map_err(|e| e.to_string())?.end_service(&service_id, now_ms()).map_err(|e| e.to_string())?;
    reload_if_loaded(&state, &service_id)
}

#[tauri::command]
pub fn freeze_past_services(state: tauri::State<AppState>, today_starts_at_ms: i64) -> Result<usize, String> {
    let frozen = state.db.lock().map_err(|e| e.to_string())?.freeze_past_services(today_starts_at_ms).map_err(|e| e.to_string())?;
    if frozen > 0 {
        if let Some(id) = loaded(&state)? {
            reload_if_loaded(&state, &id)?;
        }
    }
    Ok(frozen)
}

#[tauri::command]
pub fn delete_service(state: tauri::State<AppState>, service_id: String) -> Result<(), String> {
    if loaded(&state)?.as_deref() == Some(service_id.as_str()) {
        return Err("This service is loaded. Load another service before deleting it.".into());
    }
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_service(&service_id).map_err(|e| e.to_string())?;
    Ok(())
}

fn loaded(state: &AppState) -> Result<Option<String>, String> {
    Ok(state.snapshot.read().map_err(|e| e.to_string())?.service.as_ref().map(|s| s.id().to_string()))
}

fn reload_if_loaded(state: &AppState, service_id: &str) -> Result<(), String> {
    if loaded(state)?.as_deref() != Some(service_id) {
        return Ok(());
    }
    let service = state.db.lock().map_err(|e| e.to_string())?.service(service_id).map_err(|e| e.to_string())?;
    match service {
        Some(service) => state.send(EngineCommand::SaveService { service }),
        None => Ok(()),
    }
}
