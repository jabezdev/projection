use protocol::sync::{Conflict, SyncStatus};
use serde::Deserialize;
use serde_json::json;

use super::client::{normalize, Convex};
use super::{Credentials, SyncHandle};
use crate::AppState;

#[derive(Deserialize)]
struct Claimed {
    token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hello {
    user_name: String,
}

#[tauri::command]
pub fn get_sync_status(sync: tauri::State<'_, SyncHandle>) -> SyncStatus {
    let mut status = sync.0.status.read().map(|s| s.clone()).unwrap_or_default();
    let credentials = sync.0.credentials();
    status.paired = credentials.paired().is_some();
    status.deployment = credentials.deployment;
    status.device_name = credentials.device_name;
    status.offline = credentials.offline;
    status
}

#[tauri::command]
pub async fn pair_sync(sync: tauri::State<'_, SyncHandle>, state: tauri::State<'_, AppState>, deployment: String, code: String, device_name: String) -> Result<String, String> {
    let deployment = normalize(&deployment).map_err(|e| e.to_string())?;
    let convex = Convex::new(&deployment).map_err(|e| e.to_string())?;
    let device_name = Some(device_name.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Projection computer".into());
    let claimed: Claimed = convex.mutation("devices:claimPairingCode", json!({ "code": code, "deviceName": device_name })).await.map_err(|e| e.to_string())?;
    let hello: Hello = convex.query("sync:hello", json!({ "token": claimed.token })).await.map_err(|e| e.to_string())?;
    let previous = sync.0.credentials();
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        if previous.deployment.as_deref() != Some(deployment.as_str()) {
            db.forget_sync().map_err(|e| e.to_string())?;
        }
        db.mark_everything_changed().map_err(|e| e.to_string())?;
    }
    let next = Credentials { deployment: Some(deployment), token: Some(claimed.token), device_name: Some(device_name), offline: false };
    save(&sync, next)?;
    sync.0.poke();
    Ok(hello.user_name)
}

#[tauri::command]
pub fn unpair_sync(sync: tauri::State<'_, SyncHandle>, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.db.lock().map_err(|e| e.to_string())?.forget_sync().map_err(|e| e.to_string())?;
    save(&sync, Credentials::default())?;
    sync.0.poke();
    Ok(())
}

#[tauri::command]
pub fn set_sync_offline(sync: tauri::State<'_, SyncHandle>, offline: bool) -> Result<(), String> {
    let next = Credentials { offline, ..sync.0.credentials() };
    save(&sync, next)?;
    sync.0.poke();
    Ok(())
}

#[tauri::command]
pub fn sync_now(sync: tauri::State<'_, SyncHandle>) {
    sync.0.poke();
}

#[tauri::command]
pub fn list_sync_conflicts(state: tauri::State<'_, AppState>) -> Result<Vec<Conflict>, String> {
    state.db.lock().map_err(|e| e.to_string())?.conflicts().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resolve_sync_conflict(sync: tauri::State<'_, SyncHandle>, state: tauri::State<'_, AppState>, conflict_id: String, keep_mine: bool) -> Result<(), String> {
    if keep_mine {
        sync.0.keep_mine.lock().map_err(|e| e.to_string())?.push(conflict_id);
        sync.0.poke();
        return Ok(());
    }
    state.db.lock().map_err(|e| e.to_string())?.remove_conflict(&conflict_id).map_err(|e| e.to_string())?;
    sync.0.poke();
    Ok(())
}

fn save(sync: &SyncHandle, next: Credentials) -> Result<(), String> {
    next.save(&sync.0.dir).map_err(|e| format!("save sync settings: {e}"))?;
    *sync.0.credentials.lock().map_err(|e| e.to_string())? = next;
    Ok(())
}
