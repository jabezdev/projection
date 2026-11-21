use std::path::Path;
use std::sync::Arc;
use std::thread;

use core_engine::EngineEvent;
use tauri::Manager;
use tokio::sync::broadcast::error::RecvError;

use super::Channels;
use crate::system::recovery::{restore_commands, Previous, Recovery};
use crate::Db;

/// After a crash, queues the commands that reopen the last service at its last
/// cue, then keeps recording what is on air for the next start. Returns what
/// the operator should be told.
pub(super) fn restore_last_service(app: &tauri::App, dir: &Path, db: &Db, channels: &Channels) -> std::io::Result<Vec<String>> {
    let (recovery, previous) = Recovery::start(dir);
    let warning = match previous {
        Previous::Clean => None,
        Previous::Restore(state) => match state.project_id.map(|id| db.lock().and_then(|db| db.get_project(id))) {
            Some(Ok(project)) => {
                log::warn!("Previous run ended unexpectedly; reopening {} at its last cue", project.name);
                for command in restore_commands(&state, &project) {
                    let _ = channels.engine.command(command);
                }
                Some(format!("PALTO did not close normally. \"{}\" was reopened at its last cue with media paused. Outputs stay hidden until you show them.", project.name))
            }
            _ => Some("PALTO did not close normally, and the last service could not be reopened.".into()),
        },
        Previous::RepeatedCrash => {
            log::warn!("Previous run crashed right after a recovery; not restoring again");
            Some("PALTO closed unexpectedly twice in a row, so the last service was not reopened automatically. Open it from the service list. The log folder (Settings) has details.".into())
        }
        Previous::Crashed => Some("PALTO did not close normally last time.".into()),
    };

    let recovery = Arc::new(recovery);
    app.manage(recovery.clone());
    record_on_air_state(recovery, channels)?;
    Ok(warning.into_iter().collect())
}

fn record_on_air_state(recovery: Arc<Recovery>, channels: &Channels) -> std::io::Result<()> {
    let mut events = channels.events.subscribe();
    let snapshot = channels.snapshot.clone();
    thread::Builder::new().name("recovery".into()).spawn(move || loop {
        match events.blocking_recv() {
            Ok(EngineEvent::ProjectChanged { .. } | EngineEvent::ProgramChanged { .. }) | Err(RecvError::Lagged(_)) => {
                let current = snapshot.read().expect("snapshot lock poisoned").clone();
                recovery.observe(&current);
            }
            Ok(_) => {}
            Err(RecvError::Closed) => break,
        }
    })?;
    Ok(())
}
