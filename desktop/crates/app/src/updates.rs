//! Updates from GitHub Releases (roadmap phase 11). A new version downloads
//! in the background and is verified against the public key in
//! `tauri.conf.json`; the operator installs it from Settings → System after
//! reading its notes, never while the outputs are shown. Installers are kept
//! in `updates/` (the newest few) so an earlier version can be reinstalled.
//!
//! Only the Windows build is published, so other platforms report updates as
//! unavailable.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::displays::OutputRoutes;

/// Where releases announce themselves; each release also carries its own
/// manifest, used to fetch the running version's installer for rollback.
const RELEASES: &str = "https://github.com/jabezdev/projection/releases";
pub const FOLDER: &str = "updates";
/// Installers kept for rollback.
const KEEP: usize = 3;
const FIRST_CHECK: Duration = Duration::from_secs(120);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate { checked_ms: i64 },
    Downloading { version: String, received: u64, total: Option<u64> },
    Ready { version: String, notes: String },
    Failed { message: String },
    Unsupported,
}

pub struct Updates {
    state: Mutex<UpdateState>,
    ready: Mutex<Option<tauri_plugin_updater::Update>>,
    dir: PathBuf,
}

#[derive(Serialize)]
pub struct UpdateStatus {
    current_version: &'static str,
    #[serde(flatten)]
    state: UpdateState,
    /// Saved installers of other versions, newest first.
    earlier: Vec<String>,
    /// False while the outputs are shown.
    can_install: bool,
}

fn installer_name(version: &str) -> String {
    format!("PALTO_{version}_x64-setup.exe")
}

/// `0.MINOR.BUILD` as numbers, for ordering saved installers.
fn version_key(version: &str) -> Vec<u64> {
    version.split('.').map(|part| part.parse().unwrap_or(0)).collect()
}

/// Versions with a saved installer, newest first.
fn saved_versions(dir: &Path) -> Vec<String> {
    let mut versions: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.strip_prefix("PALTO_")?.strip_suffix("_x64-setup.exe").map(str::to_string))
        .collect();
    versions.sort_by_key(|v| std::cmp::Reverse(version_key(v)));
    versions
}

/// Keeps the newest [`KEEP`] installers.
fn prune(dir: &Path) {
    for version in saved_versions(dir).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(dir.join(installer_name(&version)));
    }
}

impl Updates {
    pub fn new(data_dir: &Path) -> Self {
        let state = if cfg!(windows) { UpdateState::Idle } else { UpdateState::Unsupported };
        Self { state: Mutex::new(state), ready: Mutex::default(), dir: data_dir.join(FOLDER) }
    }

    fn set(&self, app: &tauri::AppHandle, state: UpdateState) {
        *self.state.lock().expect("update state poisoned") = state.clone();
        let _ = app.emit("update:state", state);
    }

    fn state(&self) -> UpdateState {
        self.state.lock().expect("update state poisoned").clone()
    }
}

fn outputs_shown(app: &tauri::AppHandle) -> bool {
    app.try_state::<OutputRoutes>().is_some_and(|routes| routes.shown.load(Ordering::Acquire))
}

fn updater(app: &tauri::AppHandle, manifest: &str) -> Result<tauri_plugin_updater::Updater, String> {
    let handle = app.clone();
    app.updater_builder()
        .endpoints(vec![manifest.parse().map_err(|e| format!("{e}"))?])
        .map_err(|e| e.to_string())?
        .timeout(Duration::from_secs(60))
        // Saves and the crash-recovery marker are written before the installer takes over.
        .on_before_exit(move || crate::shutdown_native(&handle))
        .build()
        .map_err(|e| e.to_string())
}

/// Checks for a newer release and downloads it. Does nothing while a check
/// or download is already running or an update is ready.
pub async fn check(app: tauri::AppHandle) -> Result<(), String> {
    let updates = app.state::<Updates>();
    match updates.state() {
        UpdateState::Unsupported => return Err("Updates are published for the Windows version only.".into()),
        UpdateState::Checking | UpdateState::Downloading { .. } | UpdateState::Ready { .. } => return Ok(()),
        _ => {}
    }
    updates.set(&app, UpdateState::Checking);
    let result = download_latest(&app).await;
    if let Err(message) = &result {
        log::warn!("Update check failed: {message}");
        updates.set(&app, UpdateState::Failed { message: message.clone() });
    }
    result
}

async fn download_latest(app: &tauri::AppHandle) -> Result<(), String> {
    let updates = app.state::<Updates>();
    let Some(update) = updater(app, &format!("{RELEASES}/latest/download/latest.json"))?.check().await.map_err(|e| e.to_string())? else {
        updates.set(app, UpdateState::UpToDate { checked_ms: crate::now_ms() });
        return Ok(());
    };
    let version = update.version.clone();
    log::info!("Downloading PALTO {version}");
    updates.set(app, UpdateState::Downloading { version: version.clone(), received: 0, total: None });
    let mut received = 0u64;
    let progress = app.clone();
    let bytes = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                // Coarse steps keep the event rate low.
                if received % (4 << 20) < chunk as u64 {
                    progress.state::<Updates>().set(&progress, UpdateState::Downloading { version: version.clone(), received, total });
                }
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&updates.dir).map_err(|e| e.to_string())?;
    crate::settings::write_bytes_atomic(&updates.dir.join(installer_name(&version)), &bytes).map_err(|e| e.to_string())?;
    keep_current_installer(app).await;
    prune(&updates.dir);
    log::info!("PALTO {version} is ready to install");
    *updates.ready.lock().expect("ready update poisoned") = Some(update.clone());
    updates.set(app, UpdateState::Ready { version, notes: update.body.clone().unwrap_or_default() });
    Ok(())
}

/// Saves the running version's installer (from its own release) so the
/// operator can return to it after updating. Best effort.
async fn keep_current_installer(app: &tauri::AppHandle) {
    let updates = app.state::<Updates>();
    let current = env!("CARGO_PKG_VERSION");
    let path = updates.dir.join(installer_name(current));
    if path.exists() {
        return;
    }
    let manifest = format!("{RELEASES}/download/v{current}/latest.json");
    let result = async {
        let updater = app
            .updater_builder()
            .endpoints(vec![manifest.parse().map_err(|e| format!("{e}"))?])
            .map_err(|e| e.to_string())?
            .version_comparator(|_, _| true)
            .build()
            .map_err(|e| e.to_string())?;
        let release = updater.check().await.map_err(|e| e.to_string())?.ok_or("no release")?;
        let bytes = release.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
        crate::settings::write_bytes_atomic(&path, &bytes).map_err(|e| e.to_string())
    }
    .await;
    if let Err(e) = result {
        log::info!("No installer of {current} kept for rollback: {e}");
    }
}

/// Checks shortly after start and then every few hours (Windows only).
pub fn start_background_checks(app: &tauri::AppHandle) {
    if !cfg!(windows) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            let _ = check(app.clone()).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

#[tauri::command]
pub fn get_update_status(app: tauri::AppHandle, updates: tauri::State<Updates>) -> UpdateStatus {
    let current = env!("CARGO_PKG_VERSION");
    UpdateStatus {
        current_version: current,
        state: updates.state(),
        earlier: saved_versions(&updates.dir).into_iter().filter(|v| version_key(v) < version_key(current)).collect(),
        can_install: !outputs_shown(&app),
    }
}

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<(), String> {
    check(app).await
}

/// Installs the downloaded update: PALTO closes, the installer runs and
/// starts the new version.
#[tauri::command]
pub fn install_update(app: tauri::AppHandle, updates: tauri::State<Updates>) -> Result<(), String> {
    if outputs_shown(&app) {
        return Err("Hide the outputs first: updates never install during a service.".into());
    }
    let update = updates.ready.lock().map_err(|e| e.to_string())?.clone().ok_or("No update is ready")?;
    let bytes = std::fs::read(updates.dir.join(installer_name(&update.version))).map_err(|e| format!("Reading the downloaded update: {e}"))?;
    log::info!("Installing PALTO {}", update.version);
    // Verifies the signature again, runs the installer and exits.
    update.install(bytes).map_err(|e| format!("Installing: {e}"))
}

/// Runs a saved installer of an earlier version; its own screens confirm
/// replacing the newer version.
#[tauri::command]
pub fn reinstall_version(app: tauri::AppHandle, updates: tauri::State<Updates>, version: String) -> Result<(), String> {
    if outputs_shown(&app) {
        return Err("Hide the outputs first: PALTO never reinstalls during a service.".into());
    }
    if !saved_versions(&updates.dir).contains(&version) {
        return Err(format!("No saved installer of {version}"));
    }
    let installer = updates.dir.join(installer_name(&version));
    log::info!("Reinstalling PALTO {version} from {}", installer.display());
    std::process::Command::new(&installer).spawn().map_err(|e| format!("Starting the installer: {e}"))?;
    crate::shutdown_native(&app);
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_installers_sort_by_version_and_only_the_newest_stay() {
        let dir = std::env::temp_dir().join(format!("magi-updates-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for version in ["0.1.9", "0.1.10", "0.2.0", "0.1.2"] {
            std::fs::write(dir.join(installer_name(version)), b"x").unwrap();
        }
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        assert_eq!(saved_versions(&dir), ["0.2.0", "0.1.10", "0.1.9", "0.1.2"]);
        prune(&dir);
        assert_eq!(saved_versions(&dir), ["0.2.0", "0.1.10", "0.1.9"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
