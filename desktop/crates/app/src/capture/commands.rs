use std::sync::Arc;

use protocol::{CaptureKind, CaptureSource};
use serde::Serialize;

use super::*;

#[derive(Serialize)]
pub struct CaptureMethod {
    name: &'static str,
    label: &'static str,
    system_picker: bool,
}

#[derive(Serialize)]
pub struct CaptureMethods {
    current: Option<&'static str>,
    choice: String,
    methods: Vec<CaptureMethod>,
}

#[tauri::command]
pub fn get_capture_methods(captures: tauri::State<'_, Arc<Captures>>) -> CaptureMethods {
    CaptureMethods {
        current: captures.backend().map(|b| b.name()),
        choice: captures.choice.lock().expect("capture choice poisoned").clone(),
        methods: captures.backends.iter().map(|b| CaptureMethod { name: b.name(), label: b.label(), system_picker: b.system_picker() }).collect(),
    }
}

#[tauri::command]
pub fn set_capture_method(captures: tauri::State<'_, Arc<Captures>>, config: tauri::State<'_, crate::settings::ConfigStore>, name: String) -> Result<(), String> {
    if !name.is_empty() && !captures.backends.iter().any(|b| b.name() == name) {
        return Err(format!("Unknown capture method {name}"));
    }
    config.update(|c| c.capture_method = name.clone())?;
    captures.set_choice(&name);
    Ok(())
}

#[tauri::command]
pub async fn list_capture_sources(captures: tauri::State<'_, Arc<Captures>>) -> Result<Vec<CaptureSource>, String> {
    let backend = captures.backend().ok_or("No screen capture method is available on this computer")?;
    tauri::async_runtime::spawn_blocking(move || backend.sources().map_err(|e| format!("Listing screens and windows: {e:#}")))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn pick_capture_source(captures: tauri::State<'_, Arc<Captures>>, kind: CaptureKind) -> Result<Option<CaptureSource>, String> {
    let backend = captures.backend().ok_or("No screen capture method is available on this computer")?;
    tauri::async_runtime::spawn_blocking(move || backend.pick(kind).map_err(|e| format!("{e:#}"))).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_capture_status(captures: tauri::State<'_, Arc<Captures>>) -> Vec<CaptureStatus> {
    captures.status()
}
