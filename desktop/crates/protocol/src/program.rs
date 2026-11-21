use serde::{Deserialize, Serialize};

use crate::*;

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
