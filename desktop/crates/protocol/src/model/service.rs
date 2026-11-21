use serde::{Deserialize, Serialize};

use super::{Section, SectionPatch, SongVersion};
use crate::{CaptureOptions, CaptureSource, VideoOptions};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    pub scheduled_at_ms: i64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at_ms: Option<i64>,
    #[serde(default)]
    pub items: Vec<ServiceItem>,
}

impl Service {
    pub fn is_past(&self, today_starts_at_ms: i64) -> bool {
        self.ended_at_ms.is_some() || self.scheduled_at_ms < today_starts_at_ms
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceItem {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    pub content: ItemContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemContent {
    Song(SongItem),
    Text { steps: Vec<String> },
    Media {
        #[serde(rename = "mediaId")]
        media_id: String,
        #[serde(default)]
        options: VideoOptions,
    },
    Presentation {
        #[serde(default, rename = "mediaId", skip_serializing_if = "Option::is_none")]
        media_id: Option<String>,
        #[serde(default)]
        slides: Vec<String>,
    },
    Capture {
        #[serde(default)]
        options: CaptureOptions,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<CaptureSource>,
    },
    Header,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SongItem {
    pub song_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrangement: Option<Vec<Entry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    #[serde(default, skip_serializing_if = "SectionPatch::is_empty")]
    pub variation: SectionPatch,
}

impl ItemContent {
    pub fn without_device_fields(&self) -> ItemContent {
        match self {
            ItemContent::Presentation { media_id, .. } => ItemContent::Presentation { media_id: media_id.clone(), slides: Vec::new() },
            ItemContent::Capture { options, .. } => ItemContent::Capture { options: options.clone(), source: None },
            other => other.clone(),
        }
    }
}

impl SongItem {
    pub fn resolve(&self, version: &SongVersion) -> Vec<Section> {
        match &self.arrangement {
            None => version.arrangement.iter().filter_map(|id| version.section(id).cloned()).collect(),
            Some(entries) => entries
                .iter()
                .filter_map(|entry| {
                    let base = entry.section_id.as_deref().and_then(|id| version.section(id));
                    if entry.section_id.is_some() && base.is_none() && entry.variation.is_empty() {
                        return None;
                    }
                    Some(entry.variation.apply(&entry.id, base))
                })
                .collect(),
        }
    }
}
