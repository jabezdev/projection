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
            .query_row(
                "SELECT version FROM schema_migrations LIMIT 1",
                [],
                |row| row.get(0),
            )
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
        Ok(())
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectHeader>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, updated_at_ms FROM projects ORDER BY updated_at_ms DESC, id DESC")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ProjectHeader {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    updated_at_ms: row.get(2)?,
                })
            })
            .context("list projects")?;

        rows.collect::<rusqlite::Result<_>>().context("list projects")
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
            let mut model: ProjectModel = serde_json::from_str(&json).context("decode project model_json")?;
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
}
