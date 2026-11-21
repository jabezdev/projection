use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum EngineEvent {
    ProjectChanged {
        project: Option<ProjectModel>,
        version: u64,
    },

    ProgramChanged {
        program_state: ProgramState,
        version: u64,
    },

    PlaybackChanged {
        playback: Option<PlaybackState>,
        revision: u64,
    },

    Error {
        source: String,
        message: String,
        recoverable: bool,
    },
}

impl EngineSnapshot {
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
