use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

pub use history::History;
pub use remote::Applied;
pub use sync::DirtyRecord;

mod history;
mod media;
mod remote;
mod resolve;
mod schema;
mod services;
mod songs;
mod sync;
mod templates;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod sync_tests;

pub struct Repository {
    conn: Connection,
    bibles: std::sync::Arc<bible::Bibles>,
}

impl Repository {
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let conn = Connection::open(db_path).context("open sqlite db")?;
        let repo = Self { conn, bibles: std::sync::Arc::default() };
        repo.init_schema()?;
        Ok(repo)
    }

    pub fn set_bibles(&mut self, bibles: std::sync::Arc<bible::Bibles>) {
        self.bibles = bibles;
    }

    fn atomic<T>(&self, work: impl FnOnce() -> Result<T>) -> Result<T> {
        self.conn.execute_batch("SAVEPOINT atomic")?;
        match work() {
            Ok(value) => {
                self.conn.execute_batch("RELEASE atomic")?;
                Ok(value)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK TO atomic; RELEASE atomic");
                Err(e)
            }
        }
    }

    pub fn backup_to(&self, path: &Path) -> Result<()> {
        let path = path.to_str().ok_or_else(|| anyhow!("backup path is not valid UTF-8"))?;
        self.conn.execute("VACUUM INTO ?1", params![path]).context("copy database")?;
        Ok(())
    }
}
