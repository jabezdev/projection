use serde::{Deserialize, Serialize};

use super::Credits;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub credits: Credits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub current: SongVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SongVersion {
    pub id: String,
    pub number: u32,
    pub sections: Vec<Section>,
    pub arrangement: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
}

impl SongVersion {
    pub fn same_content(&self, other: &SongVersion) -> bool {
        self.sections == other.sections && self.arrangement == other.arrangement && self.background == other.background
    }

    pub fn section(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Verse,
    PreChorus,
    Chorus,
    Bridge,
    Intro,
    Outro,
    Tag,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: String,
    pub kind: SectionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub lyrics: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SectionPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<SectionKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<String>,
}

impl SectionPatch {
    pub fn is_empty(&self) -> bool {
        *self == SectionPatch::default()
    }

    pub fn apply(&self, id: &str, base: Option<&Section>) -> Section {
        Section {
            id: id.to_string(),
            kind: self.kind.or(base.map(|b| b.kind)).unwrap_or(SectionKind::Other),
            number: self.number.or(base.and_then(|b| b.number)),
            name: self.name.clone().or(base.and_then(|b| b.name.clone())),
            color: self.color.clone().or(base.and_then(|b| b.color.clone())),
            lyrics: self.lyrics.clone().or(base.map(|b| b.lyrics.clone())).unwrap_or_default(),
        }
    }
}
