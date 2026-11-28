use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Change, MediaFile, Service, ServiceTemplate, Song, SongVersion};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Song,
    Service,
    Template,
    Media,
}

impl RecordKind {
    pub const ALL: [RecordKind; 4] = [RecordKind::Song, RecordKind::Service, RecordKind::Template, RecordKind::Media];

    pub fn name(self) -> &'static str {
        match self {
            RecordKind::Song => "song",
            RecordKind::Service => "service",
            RecordKind::Template => "template",
            RecordKind::Media => "media",
        }
    }

    pub fn from_name(name: &str) -> Option<RecordKind> {
        RecordKind::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullPage {
    pub cursor: f64,
    pub more: bool,
    #[serde(default)]
    pub songs: Vec<RemoteSong>,
    #[serde(default)]
    pub services: Vec<RemoteService>,
    #[serde(default)]
    pub templates: Vec<ServiceTemplate>,
    #[serde(default)]
    pub media: Vec<RemoteMedia>,
    #[serde(default)]
    pub deleted: Vec<Deleted>,
    #[serde(default)]
    pub pinned: Vec<PinnedVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PinnedVersion {
    pub song_id: String,
    pub version: SongVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSong {
    pub song: Song,
    #[serde(default)]
    pub versions: Vec<SongVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteService {
    pub service: Service,
    #[serde(default)]
    pub frozen: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMedia {
    pub media: MediaFile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Deleted {
    pub kind: RecordKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Push {
    Create { kind: RecordKind, record: Value },
    Change {
        change: Change,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        versions: Vec<SongVersion>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current: Option<String>,
    },
    Replace { kind: RecordKind, base: Value, record: Value },
    Delete { kind: RecordKind, id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushResult {
    pub kind: RecordKind,
    pub id: String,
    #[serde(default)]
    pub conflicts: Vec<RemoteConflict>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConflict {
    pub label: String,
    pub mine: Value,
    pub theirs: Value,
    pub keep_mine: Push,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub id: String,
    pub kind: RecordKind,
    pub record_id: String,
    pub title: String,
    pub label: String,
    pub mine: Value,
    pub theirs: Value,
    pub keep_mine: Push,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub paired: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    pub offline: bool,
    pub state: SyncState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_synced_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub pending: usize,
    pub conflicts: usize,
    pub uploads: usize,
    pub downloads: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_service: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    #[default]
    Off,
    Idle,
    Syncing,
    Offline,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NowShowing {
    pub service_id: String,
    pub service_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(default)]
    pub blank: bool,
}
