use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use protocol::{EngineCommand, EngineSnapshot, MediaAction, ProjectModel, SessionItemKind};

use crate::settings::write_atomic;

pub const FILENAME: &str = "session-state.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    pub running: bool,
    pub project_id: Option<i64>,
    pub item_id: Option<i64>,
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
            state if state.project_id.is_some() => Previous::Restore(state.clone()),
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
        let cue = (snapshot.project.as_ref().map(|p| p.id), program.active_item_id, program.active_step_index);
        if cue == (current.project_id, current.item_id, current.step) {
            return;
        }
        self.write(SessionState { running: true, project_id: cue.0, item_id: cue.1, step: cue.2, recovered: false });
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

pub fn restore_commands(state: &SessionState, project: &ProjectModel) -> Vec<EngineCommand> {
    let mut commands = vec![EngineCommand::LoadProject { project_id: project.id }];
    let item = state.item_id.and_then(|id| project.session.items.iter().find(|item| item.id == id));
    if let Some(item) = item {
        commands.push(EngineCommand::Cue { item_id: item.id, index: state.step.unwrap_or(0) });
        if matches!(item.kind, SessionItemKind::Video { .. } | SessionItemKind::Audio { .. }) {
            commands.push(EngineCommand::Media { session_id: None, action: MediaAction::Pause });
        }
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{SessionItem, SessionModel, VideoOptions};

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("magi-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn snapshot(project_id: i64, item_id: i64, step: u32) -> EngineSnapshot {
        let mut snapshot = EngineSnapshot { project: Some(ProjectModel::empty(project_id, "Sunday")), ..Default::default() };
        snapshot.program_state.active_item_id = Some(item_id);
        snapshot.program_state.active_step_index = Some(step);
        snapshot
    }

    #[test]
    fn a_clean_exit_restores_nothing() {
        let dir = temp_dir();
        let (recovery, previous) = Recovery::start(&dir);
        assert!(matches!(previous, Previous::Clean));
        recovery.observe(&snapshot(3, 30, 2));
        recovery.clean_exit();
        assert!(matches!(Recovery::start(&dir).1, Previous::Clean));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_crash_restores_the_last_cue_once() {
        let dir = temp_dir();
        let (recovery, _) = Recovery::start(&dir);
        recovery.observe(&snapshot(3, 30, 2));
        drop(recovery);
        let (recovery, previous) = Recovery::start(&dir);
        let Previous::Restore(state) = previous else { panic!("expected a restore") };
        assert_eq!((state.project_id, state.item_id, state.step), (Some(3), Some(30), Some(2)));

        recovery.observe(&snapshot(3, 30, 2));
        drop(recovery);
        assert!(matches!(Recovery::start(&dir).1, Previous::RepeatedCrash));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn moving_on_after_a_restore_allows_the_next_recovery() {
        let dir = temp_dir();
        let (recovery, _) = Recovery::start(&dir);
        recovery.observe(&snapshot(3, 30, 2));
        drop(recovery);
        let (recovery, _) = Recovery::start(&dir);
        recovery.observe(&snapshot(3, 31, 0));
        drop(recovery);
        let Previous::Restore(state) = Recovery::start(&dir).1 else { panic!("expected a restore") };
        assert_eq!(state.item_id, Some(31));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restoring_media_pauses_it() {
        let video = SessionItem { id: 30, title: None, kind: SessionItemKind::Video { path: "a.mp4".into(), options: VideoOptions::default() }, song: None, background: None };
        let project = ProjectModel { id: 3, name: "Sunday".into(), session: SessionModel { items: vec![video] } };
        let state = SessionState { running: true, project_id: Some(3), item_id: Some(30), step: None, recovered: false };
        let commands = restore_commands(&state, &project);
        assert!(matches!(commands.as_slice(), [
            EngineCommand::LoadProject { project_id: 3 },
            EngineCommand::Cue { item_id: 30, index: 0 },
            EngineCommand::Media { session_id: None, action: MediaAction::Pause },
        ]));
        let gone = SessionState { item_id: Some(99), ..state };
        assert_eq!(restore_commands(&gone, &project).len(), 1);
    }
}
