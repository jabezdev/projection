use serde::{Deserialize, Serialize};

use crate::model::Service;
use crate::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum EngineCommand {
    Media {
        #[serde(default)]
        session_id: Option<SessionId>,
        action: MediaAction,
    },

    Play,
    Pause,
    Stop,

    Next,
    Prev,

    NextItem,
    PrevItem,
    Cue { item_id: String, index: u32 },
    GoToItem {
        item_id: String,
    },
    NextStep,
    PrevStep,
    GoToStep {
        index: u32,
    },

    Seek {
        time_ms: i64,
    },

    SetActiveText {
        text: String,
    },
    SetAutoplay { enabled: bool },
    SetDisplay {
        display: DisplayMode,
    },

    SetBackground {
        path: String,
    },

    LoadService {
        service_id: String,
    },
    SaveService {
        service: Service,
    },
    StepServiceHistory {
        service_id: String,
        redo: bool,
        #[serde(default)]
        request: u64,
    },
    RefreshService,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandDomain {
    Editor,
    Live,
}

impl EngineCommand {
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
            EngineCommand::LoadService { .. } | EngineCommand::SaveService { .. } | EngineCommand::StepServiceHistory { .. } | EngineCommand::RefreshService => {
                CommandDomain::Editor
            }

            _ => CommandDomain::Live,
        }
    }
}
