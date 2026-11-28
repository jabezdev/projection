use anyhow::{Context, Result};
use rusqlite::OptionalExtension;

use super::Repository;

const SCHEMA_VERSION: i64 = 5;

impl Repository {
    pub(super) fn init_schema(&self) -> Result<()> {
        self.conn.execute("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER NOT NULL)", []).context("create schema_migrations")?;
        let version: i64 = self
            .conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row.get::<_, Option<i64>>(0))
            .optional()
            .context("read schema version")?
            .flatten()
            .unwrap_or(0);
        if version >= SCHEMA_VERSION {
            return Ok(());
        }
        self.conn
            .execute_batch(&format!(
                "BEGIN;
                DROP TABLE IF EXISTS projects;
                DROP TABLE IF EXISTS songs;
                DROP TABLE IF EXISTS media;
                {TABLES}
                DELETE FROM schema_migrations;
                INSERT INTO schema_migrations(version) VALUES ({SCHEMA_VERSION});
                COMMIT;"
            ))
            .context("create tables")
    }
}

const TABLES: &str = "CREATE TABLE IF NOT EXISTS song (
        id TEXT PRIMARY KEY, title TEXT NOT NULL, archived INTEGER NOT NULL DEFAULT 0,
        credits TEXT NOT NULL, key TEXT, tags TEXT NOT NULL, current_version TEXT NOT NULL,
        updated_at_ms INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE IF NOT EXISTS song_version (
        id TEXT PRIMARY KEY, song_id TEXT NOT NULL, number INTEGER NOT NULL, body TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS song_version_by_song ON song_version(song_id, number);
    CREATE TABLE IF NOT EXISTS service (
        id TEXT PRIMARY KEY, scheduled_at_ms INTEGER NOT NULL, title TEXT NOT NULL, leader TEXT, notes TEXT,
        ended_at_ms INTEGER, frozen INTEGER NOT NULL DEFAULT 0, updated_at_ms INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS service_by_time ON service(scheduled_at_ms);
    CREATE TABLE IF NOT EXISTS service_item (
        id TEXT PRIMARY KEY, service_id TEXT NOT NULL, position INTEGER NOT NULL,
        song_id TEXT, version_id TEXT, body TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS service_item_by_service ON service_item(service_id, position);
    CREATE INDEX IF NOT EXISTS service_item_by_version ON service_item(version_id);
    CREATE TABLE IF NOT EXISTS media_file (
        id TEXT PRIMARY KEY, title TEXT NOT NULL, kind TEXT NOT NULL, hash TEXT
    );
    CREATE TABLE IF NOT EXISTS media_path (media_id TEXT PRIMARY KEY, path TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS device_binding (item_id TEXT PRIMARY KEY, body TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS edit_step (
        seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, record_id TEXT NOT NULL,
        at_ms INTEGER NOT NULL, stack TEXT NOT NULL, body TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS edit_step_by_record ON edit_step(record_id, stack, seq);
    CREATE TABLE IF NOT EXISTS edit_compaction (record_id TEXT PRIMARY KEY, through_seq INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS service_template (id TEXT PRIMARY KEY, name TEXT NOT NULL, body TEXT NOT NULL, updated_at_ms INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE IF NOT EXISTS app_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS sync_state (
        kind TEXT NOT NULL, id TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 0, pushed INTEGER NOT NULL DEFAULT 0, base TEXT,
        PRIMARY KEY (kind, id)
    );
    CREATE TABLE IF NOT EXISTS sync_deleted (kind TEXT NOT NULL, id TEXT NOT NULL, PRIMARY KEY (kind, id));
    CREATE TABLE IF NOT EXISTS sync_conflict (id TEXT PRIMARY KEY, record_id TEXT NOT NULL, at_ms INTEGER NOT NULL, body TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS media_remote (media_id TEXT PRIMARY KEY, url TEXT, uploaded INTEGER NOT NULL DEFAULT 0);";
