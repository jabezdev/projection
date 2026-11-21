use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use super::*;

#[derive(Default)]
pub struct ChosenPdf(std::sync::Mutex<Option<std::path::PathBuf>>);

#[tauri::command]
pub async fn choose_pdf(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Import a PDF as slides").add_filter("PDF", &["pdf"]).pick_file(move |file| { let _ = sender.send(file); });
    let Some(file) = receiver.await.map_err(|e| e.to_string())? else { return Ok(None) };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    *app.state::<ChosenPdf>().0.lock().map_err(|e| e.to_string())? = Some(path);
    Ok(Some(name))
}

#[tauri::command]
pub async fn read_chosen_pdf(app: tauri::AppHandle) -> Result<tauri::ipc::Response, String> {
    let path = app.state::<ChosenPdf>().0.lock().map_err(|e| e.to_string())?.clone().ok_or("No PDF was chosen")?;
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(&path).map_err(|e| format!("Reading {}: {e}", path.display())))
        .await.map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub async fn save_slide_image(app: tauri::AppHandle, request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("Send the slide as raw bytes".into()) };
    if !bytes.starts_with(b"\x89PNG") {
        return Err("The slide is not a PNG".into());
    }
    let name = request.headers().get("x-slide-name").and_then(|v| v.to_str().ok()).unwrap_or("slide.png").to_string();
    let (bytes, data_dir) = (bytes.clone(), app.state::<DataDir>().0.clone());
    let path = tauri::async_runtime::spawn_blocking(move || crate::library::import_bytes(&data_dir, &name, &bytes).map_err(|e| format!("Saving the slide: {e}")))
        .await.map_err(|e| e.to_string())??;
    app.asset_protocol_scope().allow_file(&path).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}
