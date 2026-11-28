use protocol::model::{new_id, ServiceTemplate};

use crate::{now_ms, AppState};

#[tauri::command]
pub fn list_templates(state: tauri::State<AppState>) -> Result<Vec<ServiceTemplate>, String> {
    state.db.lock().map_err(|e| e.to_string())?.list_templates().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_template(state: tauri::State<AppState>, template: ServiceTemplate) -> Result<(), String> {
    let name = Some(template.name.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Template".into());
    let template = ServiceTemplate { name, ..template };
    state.db.lock().map_err(|e| e.to_string())?.save_template(&template, now_ms()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_template(state: tauri::State<AppState>, template_id: String) -> Result<(), String> {
    state.db.lock().map_err(|e| e.to_string())?.delete_template(&template_id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn create_service_from_template(state: tauri::State<AppState>, template_id: Option<String>, parts: Vec<String>, title: String, scheduled_at_ms: i64) -> Result<protocol::model::Service, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let template = match &template_id {
        Some(id) => db.template(id).map_err(|e| e.to_string())?.ok_or("That template was deleted")?,
        None => ServiceTemplate { id: String::new(), name: "Service".into(), title: None, weekday: None, start_minutes: None, parts: vec![] },
    };
    let service = template.instantiate(&parts, Some(&title), scheduled_at_ms);
    db.save_service(&service, now_ms()).map_err(|e| e.to_string())?;
    Ok(service)
}

#[tauri::command]
pub fn save_service_as_template(state: tauri::State<AppState>, service_id: String, template_id: Option<String>, name: String, keep_songs: bool) -> Result<ServiceTemplate, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let service = db.service(&service_id).map_err(|e| e.to_string())?.ok_or("Service not found")?;
    let existing = match &template_id {
        Some(id) => db.template(id).map_err(|e| e.to_string())?,
        None => None,
    };
    let name = Some(name.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| service.title.clone());
    let made = ServiceTemplate::from_service(template_id.unwrap_or_else(new_id), name, &service, keep_songs);
    let template = match existing {
        Some(old) => ServiceTemplate { name: made.name, parts: made.parts, ..old },
        None => made,
    };
    db.save_template(&template, now_ms()).map_err(|e| e.to_string())?;
    Ok(template)
}
