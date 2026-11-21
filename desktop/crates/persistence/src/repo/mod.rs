use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

mod media;
mod resolve;
mod schema;
mod services;
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

    pub fn backup_to(&self, path: &Path) -> Result<()> {
        let path = path.to_str().ok_or_else(|| anyhow!("backup path is not valid UTF-8"))?;
        self.conn.execute("VACUUM INTO ?1", params![path]).context("copy database")?;
        Ok(())
    }
}
