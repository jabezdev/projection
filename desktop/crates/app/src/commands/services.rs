
use core_engine::EngineCommand;
use protocol::{ProjectHeader, ProjectModel};

use crate::{now_ms, AppState};


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

/// Deletes a service. The loaded service cannot be deleted.
#[tauri::command]
pub fn delete_project(state: tauri::State<AppState>, project_id: i64) -> Result<(), String> {
    let loaded = state.snapshot.read().map_err(|e| e.to_string())?.project.as_ref().map(|p| p.id);
    if loaded == Some(project_id) {
        return Err("This service is loaded. Load another service before deleting it.".into());
    }
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_project(project_id).map_err(|e| e.to_string())?;
    Ok(())
}
