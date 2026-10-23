//! The log file. Release builds on Windows have no console, so everything is
//! written to `logs/pakit.log` in the app-data folder (and echoed to stderr),
//! rotated by size: `pakit.log` plus `pakit.1.log` … `pakit.4.log`, 5 MB each.
//! Panics are logged before the default hook runs, so a crash leaves a trace.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Tauri's bundle identifier: the app-data folder's name.
pub const IDENTIFIER: &str = "com.magi.projection";
const FILE_STEM: &str = "pakit";
const MAX_BYTES: u64 = 5 * 1024 * 1024;
/// The current file plus this many older ones.
const KEEP: usize = 4;
/// Chatty dependencies only report problems unless `RUST_LOG` says otherwise.
const DEFAULT_FILTER: &str = "info,wgpu_core=warn,wgpu_hal=warn,naga=warn,tao=warn,tauri=warn,cosmic_text=warn";

/// `%APPDATA%\com.magi.projection\logs` on Windows, the XDG data folder on Linux.
pub fn logs_dir() -> PathBuf {
    let data = std::env::var_os(crate::settings::DATA_DIR_ENV).filter(|dir| !dir.is_empty()).map(PathBuf::from);
    data.unwrap_or_else(|| dirs::data_dir().unwrap_or_else(std::env::temp_dir).join(IDENTIFIER)).join("logs")
}

/// Starts logging to stderr and the rotated file. A log folder that cannot be
/// created leaves stderr only, which is reported once.
pub fn init() {
    let dir = logs_dir();
    let file = RotatingFile::open(&dir, MAX_BYTES, KEEP);
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(DEFAULT_FILTER));
    let failure = match file {
        Ok(file) => {
            builder.target(env_logger::Target::Pipe(Box::new(file)));
            None
        }
        Err(e) => Some(e),
    };
    builder.init();
    if let Some(e) = failure {
        log::error!("Cannot write logs to {}: {e}", dir.display());
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log::error!("Panic on thread {}: {info}", thread.name().unwrap_or("unnamed"));
        default_hook(info);
    }));
}

/// Appends to `<stem>.log`; when a write would pass `max_bytes`, shifts
/// `<stem>.N.log` up by one (dropping the oldest) and starts a new file.
struct RotatingFile {
    dir: PathBuf,
    file: File,
    written: u64,
    max_bytes: u64,
    keep: usize,
}

impl RotatingFile {
    fn open(dir: &Path, max_bytes: u64, keep: usize) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = Self::path(dir, 0);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata()?.len();
        Ok(Self { dir: dir.to_path_buf(), file, written, max_bytes, keep })
    }

    fn path(dir: &Path, index: usize) -> PathBuf {
        match index {
            0 => dir.join(format!("{FILE_STEM}.log")),
            n => dir.join(format!("{FILE_STEM}.{n}.log")),
        }
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        let _ = std::fs::remove_file(Self::path(&self.dir, self.keep));
        for index in (0..self.keep).rev() {
            let from = Self::path(&self.dir, index);
            if from.exists() {
                std::fs::rename(&from, Self::path(&self.dir, index + 1))?;
            }
        }
        self.file = OpenOptions::new().create(true).append(true).open(Self::path(&self.dir, 0))?;
        self.written = 0;
        Ok(())
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if self.written > 0 && self.written + buf.len() as u64 > self.max_bytes {
            // A failed rotation keeps appending rather than losing the log.
            let _ = self.rotate();
        }
        self.file.write_all(buf)?;
        self.written += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_by_size_and_keeps_a_fixed_number_of_files() {
        let dir = std::env::temp_dir().join(format!("magi-logs-{}", uuid::Uuid::new_v4()));
        let mut log = RotatingFile::open(&dir, 100, 2).unwrap();
        for line in 0..10 {
            log.write_all(format!("{line:0>49}\n").as_bytes()).unwrap();
        }
        let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, ["pakit.1.log", "pakit.2.log", "pakit.log"]);
        // Two 50-byte lines per file; the newest lines are in the current file.
        assert_eq!(std::fs::read_to_string(dir.join("pakit.log")).unwrap().lines().last().unwrap(), format!("{:0>49}", 9));
        assert_eq!(std::fs::read_to_string(dir.join("pakit.2.log")).unwrap().lines().count(), 2);

        // Reopening appends to the current file instead of truncating it.
        drop(log);
        let log = RotatingFile::open(&dir, 100, 2).unwrap();
        assert_eq!(log.written, 100);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
