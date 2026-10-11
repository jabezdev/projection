use anyhow::{anyhow, Context, Result};
use protocol::{ProjectHeader, ProjectModel, SessionModel};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct Repository {
    conn: Connection,
}

impl Repository {
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let conn = Connection::open(db_path).context("open sqlite db")?;
        let repo = Self { conn };
        repo.init_schema()?;
        Ok(repo)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER NOT NULL
                )",
                [],
            )
            .context("create schema_migrations")?;

        let version: i64 = self
            .conn
            .query_row("SELECT version FROM schema_migrations LIMIT 1", [], |row| {
                row.get(0)
            })
            .optional()
            .context("read schema version")?
            .unwrap_or(0);

        if version < 1 {
            self.apply_migration_v1()?;
            self.conn.execute("DELETE FROM schema_migrations", [])?;
            self.conn
                .execute("INSERT INTO schema_migrations(version) VALUES (1)", [])
                .context("write schema version")?;
        }

        if version < 2 {
            self.conn.execute_batch("BEGIN;
                CREATE TABLE IF NOT EXISTS songs (id INTEGER PRIMARY KEY, title TEXT NOT NULL, lyrics TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS media (
                    id INTEGER PRIMARY KEY, title TEXT NOT NULL, kind TEXT NOT NULL, path TEXT NOT NULL
                );
                DELETE FROM schema_migrations;
                INSERT INTO schema_migrations(version) VALUES (2);
                COMMIT;")?;
        }
        Ok(())
    }

    fn apply_migration_v1(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL DEFAULT 0,
                updated_at_ms INTEGER NOT NULL DEFAULT 0,
                model_json TEXT
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS songs (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                lyrics TEXT NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectHeader>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, updated_at_ms FROM projects ORDER BY updated_at_ms DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ProjectHeader {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    updated_at_ms: row.get(2)?,
                })
            })
            .context("list projects")?;

        rows.collect::<rusqlite::Result<_>>()
            .context("list projects")
    }

    pub fn create_project(&self, name: impl Into<String>, now_ms: i64) -> Result<ProjectModel> {
        let name = name.into();
        self.conn
            .execute(
                "INSERT INTO projects(name, created_at_ms, updated_at_ms, model_json) VALUES (?1, ?2, ?2, NULL)",
                params![name, now_ms],
            )
            .context("insert project")?;
        let id = self.conn.last_insert_rowid();

        let mut model = ProjectModel {
            id,
            name: name.clone(),
            session: SessionModel::default(),
        };
        self.save_project(&mut model, now_ms)?;
        Ok(model)
    }

    pub fn get_project(&self, project_id: i64) -> Result<ProjectModel> {
        let row = self
            .conn
            .query_row(
                "SELECT id, name, model_json FROM projects WHERE id = ?1",
                params![project_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
            .context("select project")?;

        let Some((id, name, model_json)) = row else {
            return Err(anyhow!("project {project_id} not found"));
        };

        if let Some(json) = model_json {
            let mut model: ProjectModel =
                serde_json::from_str(&json).context("decode project model_json")?;
            // Keep columns authoritative for basic metadata.
            model.id = id;
            model.name = name;
            Ok(model)
        } else {
            Ok(ProjectModel {
                id,
                name,
                session: SessionModel::default(),
            })
        }
    }

    pub fn save_project(&self, project: &mut ProjectModel, now_ms: i64) -> Result<()> {
        if project.id <= 0 {
            return Err(anyhow!("save_project requires project.id > 0"));
        }
        let json = serde_json::to_string(project).context("encode project")?;
        let affected = self
            .conn
            .execute(
                "UPDATE projects SET name = ?2, updated_at_ms = ?3, model_json = ?4 WHERE id = ?1",
                params![project.id, project.name, now_ms, json],
            )
            .context("update project")?;
        if affected == 0 {
            return Err(anyhow!("project {} not found", project.id));
        }
        Ok(())
    }

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::MediaKind;

    #[test]
    fn media_catalog_retains_each_kind_and_original_path() {
        let repo = Repository::new(":memory:").unwrap();
        for (kind, path) in [
            (MediaKind::Video, "/media/welcome.mp4"),
            (MediaKind::Image, "/media/still.png"),
            (MediaKind::Audio, "/media/song.wav"),
        ] {
            let created = repo
                .create_media("A title".into(), kind.clone(), path.into())
                .unwrap();
            assert_eq!(created.kind, kind);
            assert!(repo
                .list_media()
                .unwrap()
                .iter()
                .any(|item| item == &created));
        }
        assert_eq!(repo.list_media().unwrap().len(), 3);
    }

    #[test]
    fn upgrading_version_one_adds_media_without_losing_services() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE schema_migrations(version INTEGER NOT NULL); INSERT INTO schema_migrations VALUES (1);
            CREATE TABLE projects(id INTEGER PRIMARY KEY, name TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL, model_json TEXT);
            INSERT INTO projects VALUES (1, 'Existing service', 0, 0, NULL);").unwrap();
        let repo = Repository { conn };
        repo.init_schema().unwrap();
        assert_eq!(repo.get_project(1).unwrap().name, "Existing service");
        assert!(repo.list_media().unwrap().is_empty());
        repo.create_song("Song", "First\nsecond\n\nThird").unwrap();
        repo.create_media("Video".into(), MediaKind::Video, "/media/a.mp4".into())
            .unwrap();
        repo.init_schema().unwrap();
        assert_eq!(repo.list_media().unwrap().len(), 1);
        assert_eq!(repo.list_songs().unwrap().len(), 1);
    }

    #[test]
    fn service_schedule_preserves_media_options_and_song_document_metadata() {
        let repo = Repository::new(":memory:").unwrap();
        let mut service = repo.create_project("Service", 1).unwrap();
        service.session.items.push(protocol::SessionItem {
            id: 10,
            title: Some("Audio".into()),
            kind: protocol::SessionItemKind::Audio {
                path: "/audio/test.wav".into(),
                options: protocol::VideoOptions { looping: true, volume: 40, ..Default::default() },
            },
            song: None,
            background: None,
        });
        service.session.items.push(protocol::SessionItem {
            id: 11,
            title: Some("Song".into()),
            kind: protocol::SessionItemKind::Text {
                steps: vec!["A\nB".into(), "C".into()],
            },
            song: Some("{\"slideSeparator\":\"blank-line\"}".into()),
            background: Some("/media/bg.png".into()),
        });
        service.session.items.push(protocol::SessionItem {
            id: 12,
            title: Some("Video".into()),
            kind: protocol::SessionItemKind::Video {
                path: "/video/clip.mp4".into(),
                options: protocol::VideoOptions {
                    confidence: true, looping: true, start_ms: 1500,
                    end_ms: Some(8000), volume: 75, rate_percent: 125,
                },
            },
            song: None, background: None,
        });
        repo.save_project(&mut service, 2).unwrap();
        assert_eq!(repo.get_project(service.id).unwrap(), service);
    }
}
