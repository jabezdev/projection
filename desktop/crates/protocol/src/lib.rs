use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectModel {
    pub id: i64,
    pub name: String,
    pub session: SessionModel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectHeader {
    pub id: i64,
    pub name: String,
    pub updated_at_ms: i64,
}

impl ProjectModel {
    pub fn empty(id: i64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            session: SessionModel::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SessionModel {
    pub items: Vec<SessionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionItem {
    /// Stable item identifier within a project.
    pub id: i64,
    pub title: Option<String>,
    pub kind: SessionItemKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SessionItemKind {
    Text { steps: Vec<String> },
    Image { path: String },
    Video { path: String },
    Presentation { path: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Manual,
    Automated,
    External,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundKind {
    Image,
    Video,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Background {
    pub kind: BackgroundKind,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgramState {
    pub mode: Mode,
    pub transport: Transport,
    /// Authoritative timeline time. In Manual mode this can remain 0.
    pub time_ms: i64,

    /// Active item within a session playlist (mixed media).
    pub active_item_id: Option<i64>,
    pub next_item_id: Option<i64>,

    /// Optional internal position within the active item (line/slide/page/etc.).
    pub active_step_index: Option<u32>,

    /// Resolved final text being rendered.
    pub active_text: String,

    pub background: Option<Background>,
}

impl Default for ProgramState {
    fn default() -> Self {
        Self {
            mode: Mode::Manual,
            transport: Transport::Stopped,
            time_ms: 0,
            active_item_id: None,
            next_item_id: None,
            active_step_index: None,
            active_text: String::new(),
            background: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineSnapshot {
    pub program_state: ProgramState,
    pub version: u64,

    /// Loaded project (Phase 2). When absent, navigation commands are no-ops.
    pub project: Option<ProjectModel>,
}

impl Default for EngineSnapshot {
    fn default() -> Self {
        Self {
            program_state: ProgramState::default(),
            version: 0,
            project: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum EngineCommand {
    // Transport
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
    GoToItem { item_id: i64 },
    NextStep,
    PrevStep,
    GoToStep { index: u32 },

    // Time
    Seek { time_ms: i64 },

    // Live overrides
    SetActiveText { text: String },
    SetOpacity { layer: usize, value: f32 },

    // Background
    SetBackground { path: String },

    // Windowing
    Resize { width: u32, height: u32 },

    // Project
    LoadProject { project_id: i64 },
    SaveProject { project: ProjectModel },
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
    ProgramChanged { program_state: ProgramState, version: u64 },

    /// A command could not be applied. State is unchanged, so there is no version.
    Error {
        source: String,
        message: String,
        recoverable: bool,
    },
}

impl EngineSnapshot {
    /// The events a freshly connected client needs to reach this state.
    pub fn sync_events(&self) -> [EngineEvent; 2] {
        [
            EngineEvent::ProjectChanged {
                project: self.project.clone(),
                version: self.version,
            },
            EngineEvent::ProgramChanged {
                program_state: self.program_state.clone(),
                version: self.version,
            },
        ]
    }
}
