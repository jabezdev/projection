use serde::{Deserialize, Serialize};

mod input;
mod output_config;
pub use input::{InputAction, InputBinding, InputBindings, InputTrigger, MidiMessage, INPUT_BINDINGS_VERSION};
pub use output_config::{BackgroundFit, Easing, OutputConfig, TextAlign, TransitionDirection, TransitionKind, VerticalAlign, OUTPUT_CONFIG_VERSION, SHADER_PRESETS};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SongModel {
    pub id: i64,
    pub title: String,
    pub lyrics: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SongHeader {
    pub id: i64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Video,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaModel {
    pub id: i64,
    pub title: String,
    pub kind: MediaKind,
    pub path: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub song: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SessionItemKind {
    Text { steps: Vec<String> },
    Image { path: String },
    Video { path: String, #[serde(default)] options: VideoOptions },
    /// Standalone audio shares the video transport options; `confidence` is ignored.
    Audio { path: String, #[serde(default)] options: VideoOptions },
    Presentation { path: String, #[serde(default)] slides: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct VideoOptions {
    pub confidence: bool,
    pub looping: bool,
    pub start_ms: u64,
    pub end_ms: Option<u64>,
    pub volume: u8,
    pub rate_percent: u16,
}
impl Default for VideoOptions {
    fn default() -> Self { Self { confidence: false, looping: false, start_ms: 0, end_ms: None, volume: 100, rate_percent: 100 } }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Manual,
    Automated,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundKind {
    Image,
    Video,
    /// A built-in animated background: the path is `shader:<preset>`.
    Shader,
}

/// The `shader:` prefix of shader background paths.
pub const SHADER_PREFIX: &str = "shader:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Background {
    pub kind: BackgroundKind,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    Logo,
    Black,
    Clear,
    #[default]
    Live,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgramState {
    #[serde(default)]
    pub display: DisplayMode,
    pub mode: Mode,

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
            display: DisplayMode::Live,
            mode: Mode::Manual,
            active_item_id: None,
            next_item_id: None,
            active_step_index: None,
            active_text: String::new(),
            background: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EngineSnapshot {
    pub program_state: ProgramState,
    pub version: u64,

    /// Loaded project (Phase 2). When absent, navigation commands are no-ops.
    pub project: Option<ProjectModel>,

    /// Foreground media transport. Revised independently of `version` so
    /// position reports never resend the project or program.
    #[serde(default)]
    pub playback: Option<PlaybackState>,
    #[serde(default)]
    pub playback_revision: u64,
}

/// Identifies one playback of one cued media item. Recueing the same file
/// creates a new session, so commands for the old one are rejected.
pub type SessionId = u64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackKind {
    Video,
    Audio,
}

/// What the media backend observes. Intent (play/pause) lives in `paused`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackPhase {
    #[default]
    Preparing,
    Ready,
    Playing,
    Paused,
    Ended,
    Failed,
}

/// Authoritative transport of the foreground media session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaybackState {
    pub session_id: SessionId,
    /// Bumped by every seek/stop; reports from older generations are stale.
    pub generation: u64,
    pub item_id: i64,
    pub path: String,
    pub kind: PlaybackKind,
    /// Requested transport: true until the operator asks to play.
    pub paused: bool,
    /// Held by the Logo display; playback resumes when the display returns.
    pub suspended: bool,
    pub phase: PlaybackPhase,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    /// Wall-clock time of `position_ms`, for extrapolating between reports.
    pub position_at_ms: i64,
    pub volume: u8,
    pub muted: bool,
    pub rate_percent: u16,
    pub looping: bool,
    /// Clip bounds `[start_ms, end_ms)`; `None` plays to the end.
    pub start_ms: u64,
    pub end_ms: Option<u64>,
    /// Also shown fullscreen on the confidence output.
    #[serde(default)]
    pub confidence: bool,
    pub error: Option<String>,
}

impl PlaybackState {
    /// Whether media should currently be advancing.
    pub fn running(&self) -> bool {
        !self.paused && !self.suspended
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum MediaAction {
    Play,
    Pause,
    /// Pause and return to the clip start.
    Stop,
    Seek { position_ms: u64 },
    SetMuted { muted: bool },
    /// Transient volume (0–100); saved volume comes from `VideoOptions`.
    SetVolume { volume: u8 },
}

/// What a media backend reports about a session it is playing. Backends send
/// these; clients never do, so they are not an `EngineCommand`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaybackReport {
    pub session_id: SessionId,
    pub generation: u64,
    pub phase: PlaybackPhase,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Instructions from the engine to whichever backend plays media.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MediaDirective {
    /// A new foreground session; the backend prepares it without audio until `Play`.
    Open { playback: PlaybackState },
    Play { session_id: SessionId },
    Pause { session_id: SessionId },
    /// Move to `position_ms`; subsequent reports must carry `generation`.
    Seek { session_id: SessionId, generation: u64, position_ms: u64 },
    /// Volume, mute, rate, loop or clip bounds changed.
    Configure { playback: PlaybackState },
    /// Stop output (audio included) and release the session exactly once.
    Close { session_id: SessionId },
}

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

#[cfg(test)]
mod video_tests {
    use super::*;
    #[test]
    fn existing_video_services_load_with_playback_defaults() {
        let video: SessionItemKind = serde_json::from_str(r#"{"kind":"video","data":{"path":"clip.mp4"}}"#).unwrap();
        assert_eq!(video, SessionItemKind::Video { path: "clip.mp4".into(), options: VideoOptions::default() });
        let audio: SessionItemKind = serde_json::from_str(r#"{"kind":"audio","data":{"path":"song.mp3"}}"#).unwrap();
        assert_eq!(audio, SessionItemKind::Audio { path: "song.mp3".into(), options: VideoOptions::default() });
    }

    #[test]
    fn remote_clients_keep_shorthand_transport_commands() {
        let play: EngineCommand = serde_json::from_str(r#"{"type":"play"}"#).unwrap();
        assert!(matches!(play.normalize(), EngineCommand::Media { session_id: None, action: MediaAction::Play }));
        let seek: EngineCommand = serde_json::from_str(r#"{"type":"seek","payload":{"time_ms":-5}}"#).unwrap();
        assert!(matches!(seek.normalize(), EngineCommand::Media { action: MediaAction::Seek { position_ms: 0 }, .. }));
    }

    #[test]
    fn addressed_media_commands_have_a_stable_wire_format() {
        let json = r#"{"type":"media","payload":{"session_id":42,"action":{"type":"seek","payload":{"position_ms":1500}}}}"#;
        let command: EngineCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(command, EngineCommand::Media { session_id: Some(42), action: MediaAction::Seek { position_ms: 1500 } }));
        let directive = MediaDirective::Seek { session_id: 42, generation: 3, position_ms: 1500 };
        assert_eq!(
            serde_json::to_value(&directive).unwrap(),
            serde_json::json!({"type": "seek", "session_id": 42, "generation": 3, "position_ms": 1500})
        );
    }

    #[test]
    fn snapshots_from_before_playback_existed_still_load() {
        let old = r#"{"program_state":{"mode":"manual","transport":"stopped","time_ms":0,"active_item_id":null,"next_item_id":null,"active_step_index":null,"active_text":"","background":null},"version":3,"project":null}"#;
        let snapshot: EngineSnapshot = serde_json::from_str(old).unwrap();
        assert_eq!((snapshot.version, snapshot.playback, snapshot.playback_revision), (3, None, 0));
    }
}
