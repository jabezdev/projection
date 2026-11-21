use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

mod media;
mod projects;
mod songs;

#[cfg(test)]
mod tests;

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

}

impl Repository {
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        let path = path.to_str().ok_or_else(|| anyhow!("backup path is not valid UTF-8"))?;
        self.conn.execute("VACUUM INTO ?1", params![path]).context("copy database")?;
        Ok(())
    }
}
