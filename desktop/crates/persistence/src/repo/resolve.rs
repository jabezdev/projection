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
        let mut fallback_title = None;
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
                        let (title, credits) = library.map(|s| (s.title, s.credits)).unwrap_or_default();
                        song = Some(LiveSong { song_id: song_item.song_id.clone(), version_id: version.id.clone(), sections, title, credits });
                        SessionItemKind::text(steps)
                    }
                    None => SessionItemKind::text(Vec::new()),
                }
            }
            ItemContent::Text { steps } => SessionItemKind::text(steps.clone()),
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
                None => SessionItemKind::text(Vec::new()),
            },
            ItemContent::Scripture { reference, translation, parallel } => {
                let found = bible::passage(&self.bibles, reference, translation, parallel.as_deref());
                fallback_title = Some(match &found.warning {
                    Some(warning) => format!("{} — {warning}", found.title),
                    None => found.title.clone(),
                });
                let (steps, parallel, captions) = found.slides.into_iter().map(|s| (s.text, s.parallel, s.caption)).fold((Vec::new(), Vec::new(), Vec::new()), |mut acc, (t, p, c)| {
                    acc.0.push(t);
                    acc.1.push(p);
                    acc.2.push(c);
                    acc
                });
                let parallel = if parallel.iter().all(String::is_empty) { Vec::new() } else { parallel };
                SessionItemKind::Text { steps, parallel, captions }
            }
            ItemContent::Header => SessionItemKind::Section {},
        };
        background = match item.background.as_ref().or(background.as_ref()) {
            Some(shader) if shader.starts_with(SHADER_PREFIX) => Some(shader.clone()),
            Some(id) => self.media_file(id)?.and_then(|m| m.path),
            None => None,
        };
        let title = item.title.clone().filter(|t| !t.trim().is_empty()).or(fallback_title).or_else(|| song.as_ref().and_then(|s: &LiveSong| self.song(&s.song_id).ok().flatten().map(|s| s.title)));
        Ok(SessionItem { id: item.id.clone(), title, kind, song, background })
    }
}
