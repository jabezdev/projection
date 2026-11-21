use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use core_engine::{EngineCommand, EngineSnapshot};
use protocol::{ProjectHeader, ProjectModel, SongHeader, SongModel};

use crate::{now_ms, AppState};

use super::*;

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
