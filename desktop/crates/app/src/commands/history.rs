use std::time::Duration;

use tauri::Manager;

use core_engine::EngineCommand;
use persistence::History;
use protocol::model::{Service, Song};

use crate::{now_ms, AppState, Storage};

const STEP_WAIT: Duration = Duration::from_secs(15);
const SAVES_WAIT: Duration = Duration::from_millis(500);

#[derive(serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Record {
    Service,
    Song,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stepped {
    #[serde(skip_serializing_if = "Option::is_none")]
    service: Option<Service>,
    #[serde(skip_serializing_if = "Option::is_none")]
    song: Option<Song>,
    history: History,
}

#[tauri::command]
pub fn edit_history(state: tauri::State<AppState>, store: tauri::State<Storage>, id: String) -> Result<History, String> {
    store.flush(SAVES_WAIT);
    state.db.lock().map_err(|e| e.to_string())?.history(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn undo_edit(app: tauri::AppHandle, record: Record, id: String) -> Result<Stepped, String> {
    step(app, record, id, false).await
}

#[tauri::command]
pub async fn redo_edit(app: tauri::AppHandle, record: Record, id: String) -> Result<Stepped, String> {
    step(app, record, id, true).await
}

async fn step(app: tauri::AppHandle, record: Record, id: String, redo: bool) -> Result<Stepped, String> {
    tauri::async_runtime::spawn_blocking(move || match record {
        Record::Service => step_service(&app, id, redo),
        Record::Song => step_song(&app, &id, redo),
    })
    .await
    .map_err(|e| e.to_string())?
}

fn step_service(app: &tauri::AppHandle, id: String, redo: bool) -> Result<Stepped, String> {
    let state = app.state::<AppState>();
    let store = app.state::<Storage>();
    let (request, reply) = store.expect_step();
    if let Err(e) = state.send(EngineCommand::StepServiceHistory { service_id: id.clone(), redo, request }) {
        store.forget_step(request);
        return Err(e);
    }
    let service = match reply.recv_timeout(STEP_WAIT) {
        Ok(result) => result?,
        Err(_) => {
            store.forget_step(request);
            return Err("The edit history did not answer in time.".into());
        }
    };
    let history = state.db.lock().map_err(|e| e.to_string())?.history(&id).map_err(|e| e.to_string())?;
    Ok(Stepped { service, song: None, history })
}

fn step_song(app: &tauri::AppHandle, id: &str, redo: bool) -> Result<Stepped, String> {
    let state = app.state::<AppState>();
    let (song, history) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let song = if redo { db.redo_song(id, now_ms()) } else { db.undo_song(id, now_ms()) }.map_err(|e| e.to_string())?;
        (song, db.history(id).map_err(|e| e.to_string())?)
    };
    if song.is_some() {
        state.send(EngineCommand::RefreshService)?;
    }
    Ok(Stepped { service: None, song, history })
}
