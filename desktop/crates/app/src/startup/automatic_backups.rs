use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime};

use tauri::Manager;

use crate::backup;
use crate::settings::OutputState;
use crate::Db;

const FIRST_CHECK_AFTER: Duration = Duration::from_secs(30);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// Writes the weekly backup when it is due. Checking every few hours means an
/// app left running for days still makes one.
pub(super) fn start(app: tauri::AppHandle, db: Db, dir: PathBuf) -> std::io::Result<()> {
    thread::Builder::new().name("backup".into()).spawn(move || loop {
        thread::sleep(FIRST_CHECK_AFTER);
        let logo = app.state::<OutputState>().config.lock().map(|c| c.logo_path.clone()).unwrap_or_default();
        match db.lock().and_then(|db| backup::automatic_if_due(&db, &dir, &logo, SystemTime::now())) {
            Ok(Some(path)) => log::info!("Automatic backup written to {}", path.display()),
            Ok(None) => {}
            Err(e) => log::error!("Automatic backup failed: {e:#}"),
        }
        thread::sleep(CHECK_EVERY);
    })?;
    Ok(())
}
