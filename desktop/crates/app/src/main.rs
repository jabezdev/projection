#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod commands;
mod engine_host;
mod input;
mod library;
mod output;
mod remote;
mod settings;
mod shutdown;
mod startup;
mod system;

use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use core_engine::{EngineCommand, EngineSnapshot};
use engine_host::EngineHandle;
use persistence::Repository;

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

impl library::storage::ServiceWriter for Db {
    fn load(&self, service_id: &str) -> anyhow::Result<protocol::model::Service> {
        self.lock()?.service(service_id)?.ok_or_else(|| anyhow::anyhow!("service {service_id} not found"))
    }

    fn save(&self, service: &protocol::model::Service) -> anyhow::Result<()> {
        self.lock()?.save_service(service, now_ms())
    }

    fn resolve(&self, service: &protocol::model::Service) -> anyhow::Result<protocol::LiveService> {
        self.lock()?.resolve_service(service)
    }

    fn step(&self, service_id: &str, redo: bool) -> anyhow::Result<Option<protocol::model::Service>> {
        let db = self.lock()?;
        if redo { db.redo_service(service_id, now_ms()) } else { db.undo_service(service_id, now_ms()) }
    }
}

type Storage = library::storage::StorageWorker<Db>;

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
    system::logging::init();
    log::info!("Starting PALTO {}", env!("CARGO_PKG_VERSION"));
    output::prepare_platform();
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
