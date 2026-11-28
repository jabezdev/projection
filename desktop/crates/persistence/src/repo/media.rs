use anyhow::{Context, Result};
use protocol::model::{MediaFile, MediaKind};
use rusqlite::{params, OptionalExtension, Row};

use protocol::sync::RecordKind as Kind;

use super::Repository;

impl Repository {
    pub fn save_media_file(&self, media: &MediaFile) -> Result<()> {
        self.write_media_file(media)?;
        self.mark_dirty(Kind::Media, &media.id)
    }

    pub(super) fn write_media_file(&self, media: &MediaFile) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO media_file(id, title, kind, hash) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET title = ?2, kind = ?3, hash = ?4",
                params![media.id, media.title, kind_name(media.kind), media.hash],
            )
            .context("save media file")?;
        match &media.path {
            Some(path) => self.conn.execute(
                "INSERT INTO media_path(media_id, path) VALUES (?1, ?2) ON CONFLICT(media_id) DO UPDATE SET path = ?2",
                params![media.id, path],
            ),
            None => self.conn.execute("DELETE FROM media_path WHERE media_id = ?1", params![media.id]),
        }
        .context("save media path")?;
        Ok(())
    }

    pub fn list_media_files(&self) -> Result<Vec<MediaFile>> {
        let mut statement = self.conn.prepare(&format!("{SELECT} ORDER BY m.title COLLATE NOCASE, m.id"))?;
        let rows = statement.query_map([], media_row)?;
        rows.collect::<rusqlite::Result<_>>().context("list media files")
    }

    pub fn media_file(&self, id: &str) -> Result<Option<MediaFile>> {
        self.conn.query_row(&format!("{SELECT} WHERE m.id = ?1"), params![id], media_row).optional().context("get media file")
    }

    pub fn delete_media_file(&self, id: &str) -> Result<bool> {
        let removed = self.remove_media_file(id)?;
        self.mark_deleted(Kind::Media, id)?;
        Ok(removed)
    }

    pub(super) fn remove_media_file(&self, id: &str) -> Result<bool> {
        self.conn.execute("DELETE FROM media_remote WHERE media_id = ?1", params![id])?;
        self.conn.execute("DELETE FROM media_path WHERE media_id = ?1", params![id])?;
        Ok(self.conn.execute("DELETE FROM media_file WHERE id = ?1", params![id]).context("delete media file")? > 0)
    }

    pub fn relink_media_path(&self, old: &str, new: &str) -> Result<usize> {
        self.conn.execute("UPDATE media_path SET path = ?2 WHERE path = ?1", params![old, new]).context("relink media")
    }

    pub fn media_file_at(&self, path: &str) -> Result<Option<MediaFile>> {
        self.conn.query_row(&format!("{SELECT} WHERE p.path = ?1"), params![path], media_row).optional().context("find media file")
    }
}

const SELECT: &str = "SELECT m.id, m.title, m.kind, m.hash, p.path FROM media_file m LEFT JOIN media_path p ON p.media_id = m.id";

fn media_row(row: &Row) -> rusqlite::Result<MediaFile> {
    let kind: String = row.get(2)?;
    Ok(MediaFile { id: row.get(0)?, title: row.get(1)?, kind: kind_of_name(&kind), hash: row.get(3)?, path: row.get(4)? })
}

fn kind_name(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Image => "image",
        MediaKind::Video => "video",
        MediaKind::Audio => "audio",
        MediaKind::Presentation => "presentation",
    }
}

fn kind_of_name(name: &str) -> MediaKind {
    match name {
        "video" => MediaKind::Video,
        "audio" => MediaKind::Audio,
        "presentation" => MediaKind::Presentation,
        _ => MediaKind::Image,
    }
}
