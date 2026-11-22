use serde::{Deserialize, Serialize};

use crate::model::{AuthorRole, Section, SectionKind};
use crate::{DisplayMode, EngineSnapshot, SessionItem, SessionItemKind};

pub const FEED_VERSION: u32 = 1;
pub const FEED_PATH: &str = "/feed/v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FeedDisplay {
    #[default]
    Live,
    Clear,
    Black,
    Logo,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeedItemKind {
    Song,
    Text,
    Image,
    Video,
    Audio,
    Presentation,
    Capture,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeedItem {
    pub kind: FeedItemKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeedAuthor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct FeedSong {
    pub title: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<FeedAuthor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copyright: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccli: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct FeedState {
    pub v: u32,
    pub seq: u64,
    pub display: FeedDisplay,
    pub text: String,
    pub lines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<FeedItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub song: Option<FeedSong>,
}

impl FeedState {
    pub fn from_snapshot(snapshot: &EngineSnapshot) -> Self {
        let program = &snapshot.program_state;
        let item = program.active_item_id.as_deref().and_then(|id| snapshot.service.as_ref()?.item(id));
        let step = program.active_step_index.map(|s| s as usize);
        let text = program.active_text.clone();
        Self {
            v: FEED_VERSION,
            seq: 0,
            display: match program.display {
                DisplayMode::Live => FeedDisplay::Live,
                DisplayMode::Clear => FeedDisplay::Clear,
                DisplayMode::Black => FeedDisplay::Black,
                DisplayMode::Logo => FeedDisplay::Logo,
            },
            lines: text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect(),
            text,
            next_text: item.zip(step).and_then(|(item, step)| next_step(item, step)),
            item: item.map(feed_item),
            section: item.zip(step).and_then(|(item, step)| section_at(item, step)),
            song: item.and_then(feed_song),
        }
    }

    pub fn same_content(&self, other: &FeedState) -> bool {
        Self { seq: 0, ..self.clone() } == Self { seq: 0, ..other.clone() }
    }
}

fn next_step(item: &SessionItem, step: usize) -> Option<String> {
    match &item.kind {
        SessionItemKind::Text { steps } => steps.get(step + 1).cloned(),
        _ => None,
    }
}

fn feed_item(item: &SessionItem) -> FeedItem {
    let kind = match &item.kind {
        _ if item.song.is_some() => FeedItemKind::Song,
        SessionItemKind::Text { .. } => FeedItemKind::Text,
        SessionItemKind::Image { .. } => FeedItemKind::Image,
        SessionItemKind::Video { .. } => FeedItemKind::Video,
        SessionItemKind::Audio { .. } => FeedItemKind::Audio,
        SessionItemKind::Presentation { .. } => FeedItemKind::Presentation,
        SessionItemKind::Capture { .. } => FeedItemKind::Capture,
        SessionItemKind::Section {} => FeedItemKind::Other,
    };
    let title = item.title.clone().or_else(|| item.song.as_ref().map(|s| s.title.clone())).filter(|t| !t.is_empty());
    FeedItem { kind, title }
}

fn section_at(item: &SessionItem, step: usize) -> Option<String> {
    let mut first = 0;
    for section in &item.song.as_ref()?.sections {
        first += section.slides().len();
        if step < first {
            return Some(section.label()).filter(|l| !l.is_empty());
        }
    }
    None
}

fn feed_song(item: &SessionItem) -> Option<FeedSong> {
    let song = item.song.as_ref()?;
    let credits = &song.credits;
    let role = |role: AuthorRole| match role {
        AuthorRole::Words => "words",
        AuthorRole::Music => "music",
        AuthorRole::Translation => "translation",
    };
    Some(FeedSong {
        title: song.title.clone(),
        authors: credits.authors.iter().map(|a| FeedAuthor { name: a.name.clone(), role: a.role.map(|r| role(r).to_string()) }).collect(),
        copyright: credits.copyright.clone(),
        ccli: credits.ccli.clone(),
        publisher: credits.publisher.clone(),
        year: credits.year,
    })
}

impl Section {
    pub fn label(&self) -> String {
        if let Some(name) = self.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            return name.to_string();
        }
        let kind = match self.kind {
            SectionKind::Verse => "Verse",
            SectionKind::PreChorus => "Pre-chorus",
            SectionKind::Chorus => "Chorus",
            SectionKind::Bridge => "Bridge",
            SectionKind::Intro => "Intro",
            SectionKind::Outro => "Outro",
            SectionKind::Tag => "Tag",
            SectionKind::Other => "",
        };
        match self.number {
            Some(n) if !kind.is_empty() => format!("{kind} {n}"),
            _ => kind.to_string(),
        }
    }
}
