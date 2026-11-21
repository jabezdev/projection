use anyhow::{Context, Result};
use rusqlite::params;

use super::*;

impl Repository {
    pub fn list_songs(&self) -> Result<Vec<protocol::SongHeader>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title FROM songs ORDER BY title ASC")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(protocol::SongHeader {
                    id: row.get(0)?,
                    title: row.get(1)?,
                })
            })
            .context("list songs")?;
        rows.collect::<rusqlite::Result<_>>().context("list songs")
    }

    pub fn create_song(
        &self,
        title: impl Into<String>,
        lyrics: impl Into<String>,
    ) -> Result<protocol::SongModel> {
        let title = title.into();
        let lyrics = lyrics.into();
        self.conn
            .execute(
                "INSERT INTO songs(title, lyrics) VALUES (?1, ?2)",
                params![title, lyrics],
            )
            .context("insert song")?;

        let id = self.conn.last_insert_rowid();
        Ok(protocol::SongModel { id, title, lyrics })
    }

    pub fn get_song(&self, id: i64) -> Result<protocol::SongModel> {
        self.conn
            .query_row(
                "SELECT id, title, lyrics FROM songs WHERE id = ?1",
                params![id],
                |row| {
                    Ok(protocol::SongModel {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        lyrics: row.get(2)?,
                    })
                },
            )
            .context("get song")
    }

    pub fn update_song(&self, song: &protocol::SongModel) -> Result<()> {
        let affected = self
            .conn
            .execute(
                "UPDATE songs SET title = ?2, lyrics = ?3 WHERE id = ?1",
                params![song.id, song.title, song.lyrics],
            )
            .context("update song")?;

        if affected == 0 {
            return Err(anyhow::anyhow!("song {} not found", song.id));
        }
        Ok(())
    }
    /// Deletes a library song. Services keep their own copies.
    pub fn delete_song(&self, id: i64) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM songs WHERE id = ?1", params![id]).context("delete song")? > 0)
    }
}
