use std::sync::Arc;

use bible::{BibleInfo, Bibles};
use protocol::EngineCommand;
use serde::Serialize;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use crate::AppState;

pub const BIBLE_FOLDER: &str = "bibles";
const SEARCH_LIMIT: usize = 60;

pub struct BibleStore(pub Arc<Bibles>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassageSlide {
    text: String,
    parallel: String,
    caption: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassageView {
    title: String,
    warning: Option<String>,
    slides: Vec<PassageSlide>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    reference: String,
    text: String,
}

fn store(app: &tauri::AppHandle) -> Arc<Bibles> {
    app.state::<BibleStore>().0.clone()
}

#[tauri::command]
pub fn list_bibles(app: tauri::AppHandle) -> Vec<BibleInfo> {
    store(&app).list()
}

#[tauri::command]
pub async fn bible_passage(app: tauri::AppHandle, reference: String, translation: String, parallel: Option<String>) -> Result<PassageView, String> {
    let bibles = store(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let found = bible::passage(&bibles, &reference, &translation, parallel.as_deref());
        PassageView {
            title: found.title,
            warning: found.warning,
            slides: found.slides.into_iter().map(|s| PassageSlide { text: s.text, parallel: s.parallel, caption: s.caption }).collect(),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_bible(app: tauri::AppHandle, query: String, translation: String) -> Result<Vec<SearchHit>, String> {
    let bibles = store(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let Some(found) = bibles.get(&translation) else { return Err(format!("{} is not on this computer", translation.to_uppercase())) };
        let language = found.info.language.clone();
        Ok(found
            .search(&query, SEARCH_LIMIT)
            .into_iter()
            .map(|v| {
                let name = bible::book(v.book).map(|b| b.name(&language).to_string()).unwrap_or_default();
                SearchHit { reference: format!("{name} {}:{}", v.chapter, v.verse), text: v.text.clone() }
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn import_bible(app: tauri::AppHandle) -> Result<Option<BibleInfo>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Import a Bible")
        .add_filter("Bible (OSIS, Zefania, USFM)", &["xml", "osis", "usfm", "sfm", "zip", "txt"])
        .pick_file(move |file| {
            let _ = sender.send(file);
        });
    let Some(file) = receiver.await.map_err(|e| e.to_string())? else { return Ok(None) };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let bibles = store(&app);
    let info = tauri::async_runtime::spawn_blocking(move || bibles.import(&path).map_err(|e| format!("{e:#}")))
        .await
        .map_err(|e| e.to_string())??;
    app.state::<AppState>().send(EngineCommand::RefreshService)?;
    Ok(Some(info))
}

#[tauri::command]
pub fn remove_bible(app: tauri::AppHandle, id: String) -> Result<(), String> {
    store(&app).remove(&id).map_err(|e| format!("{e:#}"))?;
    app.state::<AppState>().send(EngineCommand::RefreshService)
}
