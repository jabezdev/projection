use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use protocol::{EngineCommand, EngineSnapshot, LiveService, MediaAction, SessionItemKind};

use crate::settings::write_atomic;

pub const FILENAME: &str = "session-state.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    pub running: bool,
    pub service_id: Option<String>,
    pub item_id: Option<String>,
    pub step: Option<u32>,
    pub recovered: bool,
}

pub enum Previous {
    Clean,
    Restore(SessionState),
    RepeatedCrash,
    Crashed,
}

pub struct Recovery {
    path: PathBuf,
    state: Mutex<SessionState>,
}

impl Recovery {
    pub fn start(dir: &Path) -> (Self, Previous) {
        let path = dir.join(FILENAME);
        let previous: SessionState = std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        let outcome = match &previous {
            state if !state.running => Previous::Clean,
            state if state.recovered => Previous::RepeatedCrash,
            state if state.service_id.is_some() => Previous::Restore(state.clone()),
            _ => Previous::Crashed,
        };
        let state = match &outcome {
            Previous::Restore(state) => SessionState { running: true, recovered: true, ..state.clone() },
            _ => SessionState { running: true, ..SessionState::default() },
        };
        let recovery = Self { path, state: Mutex::new(SessionState::default()) };
        recovery.write(state);
        (recovery, outcome)
    }

    pub fn observe(&self, snapshot: &EngineSnapshot) {
        let program = &snapshot.program_state;
        let Ok(current) = self.state.lock().map(|s| s.clone()) else { return };
        let cue = (snapshot.service.as_ref().map(|s| s.id().to_string()), program.active_item_id.clone(), program.active_step_index);
        if cue == (current.service_id.clone(), current.item_id.clone(), current.step) {
            return;
        }
        self.write(SessionState { running: true, service_id: cue.0, item_id: cue.1, step: cue.2, recovered: false });
    }

    pub fn clean_exit(&self) {
        let Ok(current) = self.state.lock().map(|s| s.clone()) else { return };
        self.write(SessionState { running: false, recovered: false, ..current });
    }

    fn write(&self, state: SessionState) {
        let Ok(mut current) = self.state.lock() else { return };
        if let Err(e) = write_atomic(&self.path, &state) {
            log::warn!("Saving {}: {e}", self.path.display());
        }
        *current = state;
    }
}

pub fn restore_commands(state: &SessionState, service: &LiveService) -> Vec<EngineCommand> {
    let mut commands = vec![EngineCommand::LoadService { service_id: service.id().to_string() }];
    let item = state.item_id.as_deref().and_then(|id| service.item(id));
    if let Some(item) = item {
        commands.push(EngineCommand::Cue { item_id: item.id.clone(), index: state.step.unwrap_or(0) });
        if matches!(item.kind, SessionItemKind::Video { .. } | SessionItemKind::Audio { .. }) {
            commands.push(EngineCommand::Media { session_id: None, action: MediaAction::Pause });
        }
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::model::Service;
    use protocol::{SessionItem, VideoOptions};

    fn live(id: &str, items: Vec<SessionItem>) -> LiveService {
        LiveService { service: Service { id: id.into(), scheduled_at_ms: 0, title: "Sunday".into(), leader: None, notes: None, ended_at_ms: None, items: Vec::new() }, items }
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("magi-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn snapshot(service_id: &str, item_id: &str, step: u32) -> EngineSnapshot {
        let mut snapshot = EngineSnapshot { service: Some(live(service_id, Vec::new())), ..Default::default() };
        snapshot.program_state.active_item_id = Some(item_id.into());
        snapshot.program_state.active_step_index = Some(step);
        snapshot
    }

    #[test]
    fn a_clean_exit_restores_nothing() {
        let dir = temp_dir();
        let (recovery, previous) = Recovery::start(&dir);
        assert!(matches!(previous, Previous::Clean));
        recovery.observe(&snapshot("3", "30", 2));
        recovery.clean_exit();
        assert!(matches!(Recovery::start(&dir).1, Previous::Clean));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_crash_restores_the_last_cue_once() {
        let dir = temp_dir();
        let (recovery, _) = Recovery::start(&dir);
        recovery.observe(&snapshot("3", "30", 2));
        drop(recovery);
        let (recovery, previous) = Recovery::start(&dir);
        let Previous::Restore(state) = previous else { panic!("expected a restore") };
        assert_eq!((state.service_id.as_deref(), state.item_id.as_deref(), state.step), (Some("3"), Some("30"), Some(2)));

        recovery.observe(&snapshot("3", "30", 2));
        drop(recovery);
        assert!(matches!(Recovery::start(&dir).1, Previous::RepeatedCrash));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn moving_on_after_a_restore_allows_the_next_recovery() {
        let dir = temp_dir();
        let (recovery, _) = Recovery::start(&dir);
        recovery.observe(&snapshot("3", "30", 2));
        drop(recovery);
        let (recovery, _) = Recovery::start(&dir);
        recovery.observe(&snapshot("3", "31", 0));
        drop(recovery);
        let Previous::Restore(state) = Recovery::start(&dir).1 else { panic!("expected a restore") };
        assert_eq!(state.item_id.as_deref(), Some("31"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restoring_media_pauses_it() {
        let video = SessionItem { id: "30".into(), title: None, kind: SessionItemKind::Video { path: "a.mp4".into(), options: VideoOptions::default() }, song: None, background: None };
        let service = live("3", vec![video]);
        let state = SessionState { running: true, service_id: Some("3".into()), item_id: Some("30".into()), step: None, recovered: false };
        let commands = restore_commands(&state, &service);
        assert!(matches!(commands.as_slice(), [
            EngineCommand::LoadService { service_id },
            EngineCommand::Cue { item_id, index: 0 },
            EngineCommand::Media { session_id: None, action: MediaAction::Pause },
        ] if service_id == "3" && item_id == "30"));
        let gone = SessionState { item_id: Some("99".into()), ..state };
        assert_eq!(restore_commands(&gone, &service).len(), 1);
    }
}
