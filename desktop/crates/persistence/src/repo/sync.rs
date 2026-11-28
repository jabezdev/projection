use anyhow::{Context, Result};
use protocol::model::{ItemContent, MediaFile, Service, ServiceItem, SongVersion};
use protocol::sync::{Conflict, RecordKind as Kind};
use rusqlite::{params, OptionalExtension};
use serde_json::Value;

use super::Repository;

#[derive(Debug, Clone, PartialEq)]
pub struct DirtyRecord {
    pub kind: Kind,
    pub id: String,
    pub version: i64,
    pub base: Option<Value>,
}

impl Repository {
    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM app_meta WHERE key = ?1", params![key], |row| row.get(0)).optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT INTO app_meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", params![key, value])?;
        Ok(())
    }

    pub fn clear_meta(&self, key: &str) -> Result<()> {
        self.conn.execute("DELETE FROM app_meta WHERE key = ?1", params![key])?;
        Ok(())
    }

    pub(super) fn mark_dirty(&self, kind: Kind, id: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO sync_state(kind, id, version, pushed) VALUES (?1, ?2, 1, 0)
                 ON CONFLICT(kind, id) DO UPDATE SET version = version + 1",
                params![kind.name(), id],
            )
            .context("mark record changed")?;
        Ok(())
    }

    pub(super) fn mark_deleted(&self, kind: Kind, id: &str) -> Result<()> {
        if self.base(kind, id)?.is_some() {
            self.conn.execute("INSERT OR IGNORE INTO sync_deleted(kind, id) VALUES (?1, ?2)", params![kind.name(), id])?;
        }
        self.conn.execute("DELETE FROM sync_state WHERE kind = ?1 AND id = ?2", params![kind.name(), id])?;
        Ok(())
    }

    pub fn mark_everything_changed(&self) -> Result<usize> {
        let mut count = 0;
        for (kind, table) in [(Kind::Media, "media_file"), (Kind::Song, "song"), (Kind::Template, "service_template"), (Kind::Service, "service")] {
            let mut statement = self.conn.prepare(&format!("SELECT id FROM {table}"))?;
            let ids = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            for id in ids {
                self.mark_dirty(kind, &id)?;
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn forget_sync(&self) -> Result<()> {
        self.conn.execute_batch("DELETE FROM sync_state; DELETE FROM sync_deleted; DELETE FROM sync_conflict; DELETE FROM media_remote;")?;
        for key in ["sync_cursor", "sync_last_ms"] {
            self.clear_meta(key)?;
        }
        Ok(())
    }

    pub fn dirty_records(&self) -> Result<Vec<DirtyRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT kind, id, version, base FROM sync_state WHERE version > pushed
             ORDER BY CASE kind WHEN 'media' THEN 0 WHEN 'song' THEN 1 WHEN 'template' THEN 2 ELSE 3 END, id",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, Option<String>>(3)?)))?;
        let mut records = Vec::new();
        for row in rows {
            let (kind, id, version, base) = row?;
            let Some(kind) = Kind::from_name(&kind) else { continue };
            records.push(DirtyRecord { kind, id, version, base: base.map(|b| serde_json::from_str(&b)).transpose()? });
        }
        Ok(records)
    }

    pub fn deleted_records(&self) -> Result<Vec<(Kind, String)>> {
        let mut statement = self.conn.prepare("SELECT kind, id FROM sync_deleted ORDER BY kind, id")?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        Ok(rows.filter_map(|row| row.ok().and_then(|(kind, id)| Kind::from_name(&kind).map(|kind| (kind, id)))).collect())
    }

    pub fn delete_pushed(&self, kind: Kind, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM sync_deleted WHERE kind = ?1 AND id = ?2", params![kind.name(), id])?;
        Ok(())
    }

    pub fn pushed(&self, kind: Kind, id: &str, version: i64, base: &Value) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_state(kind, id, version, pushed, base) VALUES (?1, ?2, ?3, ?3, ?4)
             ON CONFLICT(kind, id) DO UPDATE SET pushed = MAX(pushed, ?3), base = ?4",
            params![kind.name(), id, version, serde_json::to_string(base)?],
        )?;
        Ok(())
    }

    pub(super) fn synced(&self, kind: Kind, id: &str, base: &Value) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_state(kind, id, version, pushed, base) VALUES (?1, ?2, 0, 0, ?3)
             ON CONFLICT(kind, id) DO UPDATE SET base = ?3, pushed = version",
            params![kind.name(), id, serde_json::to_string(base)?],
        )?;
        Ok(())
    }

    pub fn forget_base(&self, kind: Kind, id: &str) -> Result<()> {
        self.conn.execute("UPDATE sync_state SET base = NULL WHERE kind = ?1 AND id = ?2", params![kind.name(), id])?;
        Ok(())
    }

    pub fn base(&self, kind: Kind, id: &str) -> Result<Option<Value>> {
        let base: Option<Option<String>> = self.conn.query_row("SELECT base FROM sync_state WHERE kind = ?1 AND id = ?2", params![kind.name(), id], |row| row.get(0)).optional()?;
        base.flatten().map(|b| serde_json::from_str(&b).context("decode sync base")).transpose()
    }

    pub fn is_dirty(&self, kind: Kind, id: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT EXISTS(SELECT 1 FROM sync_state WHERE kind = ?1 AND id = ?2 AND version > pushed)", params![kind.name(), id], |row| row.get(0))?)
    }

    pub fn pending_count(&self) -> Result<usize> {
        let dirty: i64 = self.conn.query_row("SELECT COUNT(*) FROM sync_state WHERE version > pushed", [], |row| row.get(0))?;
        let deleted: i64 = self.conn.query_row("SELECT COUNT(*) FROM sync_deleted", [], |row| row.get(0))?;
        Ok((dirty + deleted) as usize)
    }

    pub fn add_conflict(&self, conflict: &Conflict) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_conflict(id, record_id, at_ms, body) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(id) DO UPDATE SET body = ?4",
            params![conflict.id, conflict.record_id, conflict.at_ms, serde_json::to_string(conflict)?],
        )?;
        Ok(())
    }

    pub fn conflicts(&self) -> Result<Vec<Conflict>> {
        let mut statement = self.conn.prepare("SELECT body FROM sync_conflict ORDER BY at_ms DESC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|body| Ok(serde_json::from_str(&body?)?)).collect()
    }

    pub fn conflict(&self, id: &str) -> Result<Option<Conflict>> {
        let body = self.conn.query_row("SELECT body FROM sync_conflict WHERE id = ?1", params![id], |row| row.get::<_, String>(0)).optional()?;
        body.map(|b| Ok(serde_json::from_str(&b)?)).transpose()
    }

    pub fn remove_conflict(&self, id: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM sync_conflict WHERE id = ?1", params![id])? > 0)
    }

    pub fn wire_service(&self, id: &str) -> Result<Option<Service>> {
        Ok(self.service(id)?.map(|service| Service { items: service.items.iter().map(wire_item).collect(), ..service }))
    }

    pub fn wire_media(&self, id: &str) -> Result<Option<MediaFile>> {
        Ok(self.media_file(id)?.map(|media| MediaFile { path: None, ..media }))
    }

    pub fn pinned_versions(&self, song_id: &str) -> Result<Vec<SongVersion>> {
        let mut statement = self.conn.prepare("SELECT DISTINCT v.id FROM song_version v JOIN service_item i ON i.version_id = v.id WHERE v.song_id = ?1 ORDER BY v.number")?;
        let ids = statement.query_map(params![song_id], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().filter_map(|id| self.song_version(id).transpose()).collect()
    }

    pub fn is_frozen_service(&self, id: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT EXISTS(SELECT 1 FROM service WHERE id = ?1 AND frozen = 1)", params![id], |row| row.get(0))?)
    }

    pub fn media_downloads(&self) -> Result<Vec<(MediaFile, String)>> {
        let mut statement = self.conn.prepare(
            "SELECT m.id, r.url FROM media_remote r JOIN media_file m ON m.id = r.media_id LEFT JOIN media_path p ON p.media_id = m.id WHERE p.path IS NULL AND r.url IS NOT NULL",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().filter_map(|(id, url)| self.media_file(&id).transpose().map(|m| m.map(|m| (m, url)))).collect()
    }

    pub fn media_uploads(&self) -> Result<Vec<MediaFile>> {
        let mut statement = self.conn.prepare(
            "SELECT m.id FROM media_file m JOIN media_path p ON p.media_id = m.id LEFT JOIN media_remote r ON r.media_id = m.id WHERE r.uploaded IS NULL OR r.uploaded = 0",
        )?;
        let ids = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().filter_map(|id| self.media_file(id).transpose()).collect()
    }

    pub fn set_media_remote(&self, media_id: &str, url: Option<&str>, uploaded: bool) -> Result<()> {
        self.conn.execute(
            "INSERT INTO media_remote(media_id, url, uploaded) VALUES (?1, ?2, ?3) ON CONFLICT(media_id) DO UPDATE SET url = COALESCE(?2, url), uploaded = MAX(uploaded, ?3)",
            params![media_id, url, uploaded],
        )?;
        Ok(())
    }

    pub fn set_media_hash(&self, media_id: &str, hash: &str) -> Result<()> {
        self.conn.execute("UPDATE media_file SET hash = ?2 WHERE id = ?1", params![media_id, hash])?;
        self.mark_dirty(Kind::Media, media_id)
    }

    pub fn set_media_path(&self, media_id: &str, path: &str) -> Result<()> {
        self.conn.execute("INSERT INTO media_path(media_id, path) VALUES (?1, ?2) ON CONFLICT(media_id) DO UPDATE SET path = ?2", params![media_id, path])?;
        Ok(())
    }
}

fn wire_item(item: &ServiceItem) -> ServiceItem {
    let content = match &item.content {
        ItemContent::Capture { .. } | ItemContent::Presentation { .. } => item.content.without_device_fields(),
        other => other.clone(),
    };
    ServiceItem { content, ..item.clone() }
}
