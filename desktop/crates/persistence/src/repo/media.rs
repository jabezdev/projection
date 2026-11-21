use anyhow::{anyhow, Context, Result};
use rusqlite::params;

use super::*;

impl Repository {
    pub fn list_media(&self) -> Result<Vec<protocol::MediaModel>> {
        let mut statement = self
            .conn
            .prepare("SELECT id, title, kind, path FROM media ORDER BY title COLLATE NOCASE, id")?;
        let rows = statement.query_map([], |row| {
            let kind: String = row.get(2)?;
            Ok(protocol::MediaModel {
                id: row.get(0)?,
                title: row.get(1)?,
                kind: match kind.as_str() {
                    "video" => protocol::MediaKind::Video,
                    "audio" => protocol::MediaKind::Audio,
                    _ => protocol::MediaKind::Image,
                },
                path: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn create_media(
        &self,
        title: String,
        kind: protocol::MediaKind,
        path: String,
    ) -> Result<protocol::MediaModel> {
        let kind_value = match kind {
            protocol::MediaKind::Image => "image",
            protocol::MediaKind::Video => "video",
            protocol::MediaKind::Audio => "audio",
        };
        self.conn.execute(
            "INSERT INTO media(title, kind, path) VALUES (?1, ?2, ?3)",
            params![title, kind_value, path],
        )?;
        Ok(protocol::MediaModel {
            id: self.conn.last_insert_rowid(),
            title,
            kind,
            path,
        })
    }

    pub fn delete_media(&self, id: i64) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM media WHERE id = ?1", params![id]).context("delete media")? > 0)
    }

    pub fn rename_media(&self, id: i64, title: &str) -> Result<()> {
        let affected = self.conn.execute("UPDATE media SET title = ?2 WHERE id = ?1", params![id, title]).context("rename media")?;
        if affected == 0 {
            return Err(anyhow!("media {id} not found"));
        }
        Ok(())
    }

    pub fn relink_media(&self, old: &str, new: &str) -> Result<usize> {
        Ok(self.conn.execute("UPDATE media SET path = ?2 WHERE path = ?1", params![old, new]).context("relink media")?)
    }
}
