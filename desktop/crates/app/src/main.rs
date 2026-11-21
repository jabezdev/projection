#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod awake;
mod backup;
mod capture;
mod commands;
mod display_names;
mod displays;
mod engine_host;
mod input;
mod library;
mod logging;
mod midi;
mod monitor_window;
mod native_media;
mod native_output;
mod native_scene;
mod recovery;
mod remote;
mod settings;
mod shutdown;
mod startup;
mod storage;
mod streamdeck;
mod updates;

use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use core_engine::{EngineCommand, EngineSnapshot};
use engine_host::EngineHandle;
use persistence::Repository;
use protocol::ProjectModel;

pub(crate) use shutdown::shutdown_native;

pub(crate) fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

#[derive(Clone)]
pub(crate) struct Db(Arc<Mutex<Repository>>);

impl Db {
    pub(crate) fn lock(&self) -> anyhow::Result<MutexGuard<'_, Repository>> {
        self.0.lock().map_err(|_| anyhow::anyhow!("database lock poisoned"))
    }
}

impl storage::ProjectWriter for Db {
    fn load(&self, project_id: i64) -> anyhow::Result<ProjectModel> {
        self.lock()?.get_project(project_id)
    }

    fn save(&self, project: &mut ProjectModel) -> anyhow::Result<()> {
        self.lock()?.save_project(project, now_ms())
    }
}

type Storage = storage::StorageWorker<Db>;

pub(crate) struct AppState {
    engine: EngineHandle,
    snapshot: Arc<RwLock<EngineSnapshot>>,
    db: Db,
}

impl AppState {
    pub(crate) fn send(&self, cmd: EngineCommand) -> Result<(), String> {
        self.engine.command(cmd)
    }
}

fn main() {
    logging::init();
    log::info!("Starting PALTO {}", env!("CARGO_PKG_VERSION"));
    native_output::prepare_platform();
    let channels = startup::Channels::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(commands::handler())
        .setup(move |app| startup::run(app, channels))
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                shutdown_native(app);
            }
        });
}
