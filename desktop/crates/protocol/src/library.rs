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
    /// A screen or window of this computer, captured live.
    Capture { source: CaptureSource, #[serde(default)] options: CaptureOptions },
    /// A heading that groups the items after it (its name is the item's
    /// title). Never on air: navigation passes over it.
    Section {},
}

impl SessionItemKind {
    /// Whether the item can be cued (everything but section headings).
    pub fn is_cueable(&self) -> bool {
        !matches!(self, SessionItemKind::Section {})
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    Screen,
    Window,
}

/// What to capture. `id` lets the capture method find it again (a display
/// name, a window's program and title, or a screen-sharing restore token);
/// `name` is shown to the operator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CaptureSource {
    pub kind: CaptureKind,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaptureFit {
    /// The whole capture, letterboxed.
    #[default]
    Fit,
    /// Fills the screen, cropping the edges.
    Fill,
}

/// Which screens show a capture, and how it fills each.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CaptureOptions {
    pub projection: bool,
    pub confidence: bool,
    pub projection_fit: CaptureFit,
    pub confidence_fit: CaptureFit,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self { projection: true, confidence: false, projection_fit: CaptureFit::Fit, confidence_fit: CaptureFit::Fit }
    }
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
