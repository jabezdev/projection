use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum EngineCommand {
    /// Media transport. `session_id: None` targets the current foreground
    /// session; a stale id is rejected.
    Media {
        #[serde(default)]
        session_id: Option<SessionId>,
        action: MediaAction,
    },

    // Shorthand for `Media { session_id: None, .. }`, kept for simple clients.
    Play,
    Pause,
    Stop,

    // Navigation (session playlist)
    /// "Smart next": advance within item if possible, else advance item.
    Next,
    /// "Smart prev": reverse within item if possible, else go to previous item.
    Prev,

    // Explicit navigation
    NextItem,
    PrevItem,
    Cue { item_id: i64, index: u32 },
    GoToItem {
        item_id: i64,
    },
    NextStep,
    PrevStep,
    GoToStep {
        index: u32,
    },

    /// Shorthand for `Media { session_id: None, action: Seek }`.
    Seek {
        time_ms: i64,
    },

    // Live overrides
    SetActiveText {
        text: String,
    },
    /// Whether cued video and audio start playing at once (the default) or
    /// wait paused on their first frame.
    SetAutoplay { enabled: bool },
    SetDisplay {
        display: DisplayMode,
    },

    // Background
    SetBackground {
        path: String,
    },

    // Project
    LoadProject {
        project_id: i64,
    },
    SaveProject {
        project: ProjectModel,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandDomain {
    /// Persistent/undoable edits to a project model.
    Editor,
    /// Transient live operations affecting current output.
    Live,
}

impl EngineCommand {
    /// Expands transport shorthands so the engine handles one media vocabulary.
    pub fn normalize(self) -> Self {
        let action = match self {
            EngineCommand::Play => MediaAction::Play,
            EngineCommand::Pause => MediaAction::Pause,
            EngineCommand::Stop => MediaAction::Stop,
            EngineCommand::Seek { time_ms } => MediaAction::Seek { position_ms: time_ms.max(0) as u64 },
            other => return other,
        };
        EngineCommand::Media { session_id: None, action }
    }

    pub fn domain(&self) -> CommandDomain {
        match self {
            EngineCommand::LoadProject { .. } | EngineCommand::SaveProject { .. } => {
                CommandDomain::Editor
            }

            // Everything else is a live op.
            _ => CommandDomain::Live,
        }
    }
}
