use serde::{Deserialize, Serialize};

pub type SessionId = u64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackKind {
    Video,
    Audio,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaybackState {
    pub session_id: SessionId,
    pub generation: u64,
    pub item_id: String,
    pub path: String,
    pub kind: PlaybackKind,
    pub paused: bool,
    pub suspended: bool,
    pub phase: PlaybackPhase,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub position_at_ms: i64,
    pub volume: u8,
    pub muted: bool,
    pub rate_percent: u16,
    pub looping: bool,
    pub start_ms: u64,
    pub end_ms: Option<u64>,
    #[serde(default)]
    pub confidence: bool,
    pub error: Option<String>,
}

impl PlaybackState {
    pub fn running(&self) -> bool {
        !self.paused && !self.suspended
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum MediaAction {
    Play,
    Pause,
    Stop,
    Seek { position_ms: u64 },
    SetMuted { muted: bool },
    SetVolume { volume: u8 },
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MediaDirective {
    Open { playback: PlaybackState },
    Play { session_id: SessionId },
    Pause { session_id: SessionId },
    Seek { session_id: SessionId, generation: u64, position_ms: u64 },
    Configure { playback: PlaybackState },
    Close { session_id: SessionId },
}
