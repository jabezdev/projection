use anyhow::Result;
use protocol::model::{ItemContent, MediaFile, Service, ServiceTemplate, SongVersion};
use protocol::sync::{Deleted, PullPage, RecordKind as Kind, RemoteMedia, RemoteService, RemoteSong};
use rusqlite::params;

use super::Repository;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Applied {
    pub changed: Vec<String>,
    pub skipped: usize,
}

impl Repository {
    pub fn apply_pull(&self, page: &PullPage, now_ms: i64) -> Result<Applied> {
        self.atomic(|| {
            let mut applied = Applied::default();
            for media in &page.media {
                self.apply_remote_media(media, &mut applied)?;
            }
            for song in &page.songs {
                self.apply_remote_song(song, now_ms, &mut applied)?;
            }
            for pinned in &page.pinned {
                self.keep_version(&pinned.song_id, &pinned.version)?;
            }
            for template in &page.templates {
                self.apply_remote_template(template, now_ms, &mut applied)?;
            }
            for service in &page.services {
                self.apply_remote_service(service, now_ms, &mut applied)?;
            }
            for deleted in &page.deleted {
                self.apply_remote_delete(deleted, &mut applied)?;
            }
            Ok(applied)
        })
    }

    fn apply_remote_song(&self, remote: &RemoteSong, now_ms: i64, applied: &mut Applied) -> Result<()> {
        let song = &remote.song;
        for version in &remote.versions {
            self.keep_version(&song.id, version)?;
        }
        if self.is_dirty(Kind::Song, &song.id)? {
            applied.skipped += 1;
            return Ok(());
        }
        let before = self.song(&song.id)?;
        self.keep_version(&song.id, &song.current)?;
        self.conn.execute(
            "INSERT INTO song(id, title, archived, credits, key, tags, current_version, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET title = ?2, archived = ?3, credits = ?4, key = ?5, tags = ?6, current_version = ?7, updated_at_ms = ?8",
            params![song.id, song.title, song.archived, serde_json::to_string(&song.credits)?, song.key, serde_json::to_string(&song.tags)?, song.current.id, now_ms],
        )?;
        if before.as_ref() != Some(song) {
            applied.changed.push(song.id.clone());
        }
        self.synced(Kind::Song, &song.id, &serde_json::to_value(song)?)
    }

    fn keep_version(&self, song_id: &str, version: &SongVersion) -> Result<()> {
        match self.song_version(&version.id)? {
            None => self.insert_version(song_id, version),
            Some(local) if local.same_content(version) => Ok(()),
            Some(_) if self.version_is_pinned(&version.id)? => Ok(()),
            Some(_) => {
                self.conn.execute("UPDATE song_version SET body = ?2, number = ?3 WHERE id = ?1", params![version.id, super::songs::body_json(version)?, version.number])?;
                Ok(())
            }
        }
    }

    fn apply_remote_service(&self, remote: &RemoteService, now_ms: i64, applied: &mut Applied) -> Result<()> {
        let id = &remote.service.id;
        if self.is_dirty(Kind::Service, id)? {
            applied.skipped += 1;
            return Ok(());
        }
        let local = self.service(id)?;
        let service = with_device_fields(&remote.service, local.as_ref());
        if local.as_ref() != Some(&service) {
            self.write_service(&service, local.as_ref(), now_ms)?;
            applied.changed.push(id.clone());
        }
        if remote.frozen {
            self.conn.execute("UPDATE service SET frozen = 1 WHERE id = ?1", params![id])?;
            self.drop_history(id)?;
        }
        self.synced(Kind::Service, id, &serde_json::to_value(&remote.service)?)
    }

    fn apply_remote_template(&self, template: &ServiceTemplate, now_ms: i64, applied: &mut Applied) -> Result<()> {
        if self.is_dirty(Kind::Template, &template.id)? {
            applied.skipped += 1;
            return Ok(());
        }
        if self.wire_template(&template.id)?.as_ref() != Some(template) {
            self.write_template(template, now_ms)?;
            applied.changed.push(template.id.clone());
        }
        self.synced(Kind::Template, &template.id, &serde_json::to_value(template)?)
    }

    fn apply_remote_media(&self, remote: &RemoteMedia, applied: &mut Applied) -> Result<()> {
        let media = &remote.media;
        self.set_media_remote(&media.id, remote.url.as_deref(), remote.url.is_some())?;
        if self.is_dirty(Kind::Media, &media.id)? {
            applied.skipped += 1;
            return Ok(());
        }
        let local = self.media_file(&media.id)?;
        let path = local.as_ref().and_then(|m| m.path.clone()).or(match &media.hash {
            Some(hash) => self.path_for_hash(hash)?,
            None => None,
        });
        let merged = MediaFile { path, ..media.clone() };
        if local.as_ref() != Some(&merged) {
            self.write_media_file(&merged)?;
            applied.changed.push(media.id.clone());
        }
        self.synced(Kind::Media, &media.id, &serde_json::to_value(media)?)
    }

    fn apply_remote_delete(&self, deleted: &Deleted, applied: &mut Applied) -> Result<()> {
        let id = deleted.id.as_str();
        if self.is_dirty(deleted.kind, id)? {
            self.conn.execute("UPDATE sync_state SET base = NULL WHERE kind = ?1 AND id = ?2", params![deleted.kind.name(), id])?;
            applied.skipped += 1;
            return Ok(());
        }
        let removed = match deleted.kind {
            Kind::Service => self.remove_service(id)?,
            Kind::Template => self.conn.execute("DELETE FROM service_template WHERE id = ?1", params![id])? > 0,
            Kind::Media => self.remove_media_file(id)?,
            Kind::Song => self.conn.execute("UPDATE song SET archived = 1 WHERE id = ?1", params![id])? > 0,
        };
        if removed {
            applied.changed.push(id.to_string());
        }
        self.conn.execute("DELETE FROM sync_state WHERE kind = ?1 AND id = ?2", params![deleted.kind.name(), id])?;
        Ok(())
    }

    fn path_for_hash(&self, hash: &str) -> Result<Option<String>> {
        let mut statement = self.conn.prepare("SELECT p.path FROM media_file m JOIN media_path p ON p.media_id = m.id WHERE m.hash = ?1 LIMIT 1")?;
        let mut rows = statement.query_map(params![hash], |row| row.get::<_, String>(0))?;
        Ok(rows.next().transpose()?)
    }
}

fn with_device_fields(remote: &Service, local: Option<&Service>) -> Service {
    let mut service = remote.clone();
    let Some(local) = local else { return service };
    for item in &mut service.items {
        let Some(old) = local.items.iter().find(|i| i.id == item.id) else { continue };
        match (&mut item.content, &old.content) {
            (ItemContent::Presentation { slides, .. }, ItemContent::Presentation { slides: kept, .. }) => *slides = kept.clone(),
            (ItemContent::Capture { source, .. }, ItemContent::Capture { source: kept, .. }) => *source = kept.clone(),
            _ => {}
        }
    }
    service
}
