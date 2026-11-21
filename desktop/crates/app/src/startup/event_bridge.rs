use tauri::Emitter;
use tokio::sync::broadcast::error::RecvError;

use super::Channels;

const ENGINE_EVENT: &str = "engine:event";

pub(super) fn forward_engine_events(app: tauri::AppHandle, channels: &Channels) {
    let mut events = channels.events.subscribe();
    let snapshot = channels.snapshot.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    let _ = app.emit(ENGINE_EVENT, event);
                }
                Err(RecvError::Lagged(missed)) => {
                    log::warn!("Operator UI missed {missed} engine events; resynchronizing");
                    let current = snapshot.read().expect("snapshot lock poisoned").clone();
                    for event in current.sync_events() {
                        let _ = app.emit(ENGINE_EVENT, event);
                    }
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}
