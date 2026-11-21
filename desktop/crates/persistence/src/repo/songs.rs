use anyhow::{anyhow, Context, Result};
use protocol::model::{new_id, Section, Song, SongVersion};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Repository;

#[derive(Serialize, Deserialize)]
struct VersionBody {
    sections: Vec<Section>,
    arrangement: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    background: Option<String>,
}

impl Repository {
    pub fn insert_song(&self, song: &Song, now_ms: i64) -> Result<()> {
        self.insert_version(&song.id, &song.current)?;
        self.conn
            .execute(
                "INSERT INTO song(id, title, archived, credits, key, tags, current_version, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    song.id,
                    song.title,
                    song.archived,
                    serde_json::to_string(&song.credits)?,
                    song.key,
                    serde_json::to_string(&song.tags)?,
                    song.current.id,
                    now_ms
                ],
            )
            .context("insert song")?;
        Ok(())
    }

    pub fn update_song_details(&self, song: &Song, now_ms: i64) -> Result<()> {
        let affected = self
            .conn
            .execute(
                "UPDATE song SET title = ?2, archived = ?3, credits = ?4, key = ?5, tags = ?6, updated_at_ms = ?7 WHERE id = ?1",
                params![song.id, song.title, song.archived, serde_json::to_string(&song.credits)?, song.key, serde_json::to_string(&song.tags)?, now_ms],
            )
            .context("update song")?;
        if affected == 0 {
            return Err(anyhow!("song {} not found", song.id));
        }
        Ok(())
    }

    pub fn edit_song_content(
        &self,
        song_id: &str,
        sections: Vec<Section>,
        arrangement: Vec<String>,
        background: Option<String>,
        now_ms: i64,
    ) -> Result<SongVersion> {
        let current = self.song(song_id)?.ok_or_else(|| anyhow!("song {song_id} not found"))?.current;
        let mut version = SongVersion { id: current.id.clone(), number: current.number, sections, arrangement, background };
        if self.version_is_pinned(&current.id)? {
            version.id = new_id();
            version.number = self.last_version_number(song_id)? + 1;
            self.insert_version(song_id, &version)?;
            self.conn.execute("UPDATE song SET current_version = ?2 WHERE id = ?1", params![song_id, version.id])?;
        } else {
            self.conn.execute("UPDATE song_version SET body = ?2 WHERE id = ?1", params![version.id, body_json(&version)?])?;
        }
        self.conn.execute("UPDATE song SET updated_at_ms = ?2 WHERE id = ?1", params![song_id, now_ms])?;
        Ok(version)
    }

    pub fn song(&self, id: &str) -> Result<Option<Song>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, title, archived, credits, key, tags, current_version FROM song WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, bool>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()
            .context("get song")?;
        let Some((id, title, archived, credits, key, tags, current)) = row else { return Ok(None) };
        let current = self.song_version(&current)?.ok_or_else(|| anyhow!("song {id} has no current version"))?;
        Ok(Some(Song { id, title, archived, credits: serde_json::from_str(&credits)?, key, tags: serde_json::from_str(&tags)?, current }))
    }

    pub fn list_library(&self, include_archived: bool) -> Result<Vec<Song>> {
        let mut statement = self.conn.prepare("SELECT id FROM song WHERE archived = 0 OR ?1 ORDER BY title COLLATE NOCASE, id")?;
        let ids = statement.query_map(params![include_archived], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().filter_map(|id| self.song(id).transpose()).collect()
    }

    pub fn song_version(&self, id: &str) -> Result<Option<SongVersion>> {
        let row = self
            .conn
            .query_row("SELECT id, number, body FROM song_version WHERE id = ?1", params![id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?, row.get::<_, String>(2)?))
            })
            .optional()
            .context("get song version")?;
        let Some((id, number, body)) = row else { return Ok(None) };
        let body: VersionBody = serde_json::from_str(&body).context("decode song version")?;
        Ok(Some(SongVersion { id, number, sections: body.sections, arrangement: body.arrangement, background: body.background }))
    }

    fn insert_version(&self, song_id: &str, version: &SongVersion) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO song_version(id, song_id, number, body) VALUES (?1, ?2, ?3, ?4)",
                params![version.id, song_id, version.number, body_json(version)?],
            )
            .context("insert song version")?;
        Ok(())
    }

    fn last_version_number(&self, song_id: &str) -> Result<u32> {
        Ok(self.conn.query_row("SELECT COALESCE(MAX(number), 0) FROM song_version WHERE song_id = ?1", params![song_id], |row| row.get(0))?)
    }

    fn version_is_pinned(&self, version_id: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT EXISTS(SELECT 1 FROM service_item WHERE version_id = ?1)", params![version_id], |row| row.get(0))?)
    }
}

fn body_json(version: &SongVersion) -> Result<String> {
    let body = VersionBody { sections: version.sections.clone(), arrangement: version.arrangement.clone(), background: version.background.clone() };
    Ok(serde_json::to_string(&body)?)
}
