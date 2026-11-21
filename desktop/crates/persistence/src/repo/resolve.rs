use anyhow::Result;
use protocol::model::{split_slides, ItemContent, MediaKind, Service, ServiceItem};
use protocol::{LiveService, LiveSong, SessionItem, SessionItemKind, SHADER_PREFIX};

use super::Repository;

impl Repository {
    pub fn resolve_service(&self, service: &Service) -> Result<LiveService> {
        let items = service.items.iter().map(|item| self.resolve_item(item)).collect::<Result<_>>()?;
        Ok(LiveService { service: service.clone(), items })
    }

    fn resolve_item(&self, item: &ServiceItem) -> Result<SessionItem> {
        let mut background = None;
        let mut song = None;
        let kind = match &item.content {
            ItemContent::Song(song_item) => {
                let library = self.song(&song_item.song_id)?;
                let version = match &song_item.version_id {
                    Some(id) => self.song_version(id)?,
                    None => library.as_ref().map(|s| s.current.clone()),
                };
                match version {
                    Some(version) => {
                        let sections = song_item.resolve(&version);
                        let steps = sections.iter().flat_map(|s| split_slides(&s.lyrics)).collect();
                        background = version.background.clone();
                        song = Some(LiveSong { song_id: song_item.song_id.clone(), version_id: version.id.clone(), sections });
                        SessionItemKind::Text { steps }
                    }
                    None => SessionItemKind::Text { steps: Vec::new() },
                }
            }
            ItemContent::Text { steps } => SessionItemKind::Text { steps: steps.clone() },
            ItemContent::Media { media_id, options } => {
                let media = self.media_file(media_id)?;
                let path = media.as_ref().and_then(|m| m.path.clone()).unwrap_or_default();
                match media.map(|m| m.kind) {
                    Some(MediaKind::Video) => SessionItemKind::Video { path, options: options.clone() },
                    Some(MediaKind::Audio) => SessionItemKind::Audio { path, options: options.clone() },
                    _ => SessionItemKind::Image { path },
                }
            }
            ItemContent::Presentation { media_id, slides } => {
                let source = match media_id {
                    Some(id) => self.media_file(id)?.and_then(|m| m.path),
                    None => None,
                };
                let path = slides.first().cloned().or(source).unwrap_or_default();
                SessionItemKind::Presentation { path, slides: slides.clone() }
            }
            ItemContent::Capture { options, source } => match source {
                Some(source) => SessionItemKind::Capture { source: source.clone(), options: options.clone() },
                None => SessionItemKind::Text { steps: Vec::new() },
            },
            ItemContent::Header => SessionItemKind::Section {},
        };
        background = match item.background.as_ref().or(background.as_ref()) {
            Some(shader) if shader.starts_with(SHADER_PREFIX) => Some(shader.clone()),
            Some(id) => self.media_file(id)?.and_then(|m| m.path),
            None => None,
        };
        let title = item.title.clone().or_else(|| song.as_ref().and_then(|s: &LiveSong| self.song(&s.song_id).ok().flatten().map(|s| s.title)));
        Ok(SessionItem { id: item.id.clone(), title, kind, song, background })
    }
}
