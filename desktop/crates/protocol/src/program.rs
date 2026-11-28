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
    Shader,
}

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

    pub active_item_id: Option<String>,
    pub next_item_id: Option<String>,

    pub active_step_index: Option<u32>,

    pub active_text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_parallel: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_caption: String,

    pub background: Option<Background>,
    #[serde(default)]
    pub stage: StageState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StageState {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub timer: Option<StageTimer>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageTimer {
    pub duration_ms: u64,
    pub remaining_ms: i64,
    pub running_since_ms: Option<i64>,
}

impl StageTimer {
    pub fn remaining_at(&self, now_ms: i64) -> i64 {
        match self.running_since_ms {
            Some(since) => self.remaining_ms - (now_ms - since).max(0),
            None => self.remaining_ms,
        }
    }
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
            active_parallel: String::new(),
            active_caption: String::new(),
            background: None,
            stage: StageState::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EngineSnapshot {
    pub program_state: ProgramState,
    pub version: u64,

    pub service: Option<LiveService>,

    #[serde(default)]
    pub playback: Option<PlaybackState>,
    #[serde(default)]
    pub playback_revision: u64,
}
