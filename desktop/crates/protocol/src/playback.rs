use serde::{Deserialize, Serialize};


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
