use serde::{Deserialize, Serialize};

use crate::*;

/// Facts emitted by the engine.
///
/// Each state event carries the *complete* state of one domain (mirroring
/// `CommandDomain`), so clients only ever replace state, never merge deltas.
/// Initial sync comes from the snapshot (`get_engine_snapshot`, or
/// `EngineSnapshot::sync_events` on WebSocket connect).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum EngineEvent {
    /// The loaded project changed (load/save). Editor domain.
    ProjectChanged {
        project: Option<ProjectModel>,
        version: u64,
    },

    /// Live program state changed (transport, selection, text, background).
    ProgramChanged {
        program_state: ProgramState,
        version: u64,
    },

    /// Foreground media transport changed (cue, transport, report).
    PlaybackChanged {
        playback: Option<PlaybackState>,
        revision: u64,
    },

    /// A command could not be applied. State is unchanged, so there is no version.
    Error {
        source: String,
        message: String,
        recoverable: bool,
    },
}

impl EngineSnapshot {
    /// The events a freshly connected client needs to reach this state.
    pub fn sync_events(&self) -> [EngineEvent; 3] {
        [
            EngineEvent::ProjectChanged {
                project: self.project.clone(),
                version: self.version,
            },
            EngineEvent::ProgramChanged {
                program_state: self.program_state.clone(),
                version: self.version,
            },
            EngineEvent::PlaybackChanged {
                playback: self.playback.clone(),
                revision: self.playback_revision,
            },
        ]
    }
}
