mod automatic_backups;
mod crash_recovery;
mod event_bridge;
mod operator_window;
mod outputs;
mod remote_server;
mod settings_files;

use std::error::Error;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

use core_engine::{EngineEvent, EngineInput, EngineSnapshot, Files};
use crossbeam::channel::Receiver;
use persistence::Repository;
use tauri::Manager;
use tokio::sync::{broadcast, watch};

use crate::commands::{ChosenPdf, DataDir, StartupWarnings};
use crate::engine_host::{EngineHandle, EngineLoop};
use crate::output::media::{NativeMedia, NativeMediaBackend};
use crate::settings::{data_dir, DB_FILENAME};
use crate::{AppState, Db, Storage};
use crate::input::midi;
use crate::input::streamdeck;
use crate::system::awake;
use crate::system::updates;

const EVENT_BACKLOG: usize = 1024;

pub(crate) struct Channels {
    engine: EngineHandle,
    engine_inputs: Receiver<EngineInput>,
    events: broadcast::Sender<EngineEvent>,
    snapshot: Arc<RwLock<EngineSnapshot>>,
    shutting_down: Arc<AtomicBool>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl Channels {
    pub(crate) fn new() -> Self {
        let (engine, engine_inputs) = EngineHandle::channel();
        let (events, _) = broadcast::channel(EVENT_BACKLOG);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            engine,
            engine_inputs,
            events,
            snapshot: Arc::default(),
            shutting_down: Arc::default(),
            shutdown_tx,
            shutdown_rx,
        }
    }
}

pub(crate) fn run(app: &mut tauri::App, channels: Channels) -> Result<(), Box<dyn Error>> {
    let dir = data_dir(app.path().app_data_dir()?);
    std::fs::create_dir_all(&dir)?;

    let mut warnings = settings_files::load_output_and_input_settings(app, &dir);
    let (config, config_path) = settings_files::load_app_config(app, &dir, &channels.engine);
    let db = Db(Arc::new(Mutex::new(Repository::new(dir.join(DB_FILENAME))?)));
    warnings.extend(crash_recovery::restore_last_service(app, &dir, &db, &channels)?);

    app.manage(StartupWarnings(warnings));
    app.manage(ChosenPdf::default());
    app.manage(updates::Updates::new(&dir));
    updates::start_background_checks(app.handle());
    app.manage(DataDir(dir.clone()));
    automatic_backups::start(app.handle().clone(), db.clone(), dir.clone())?;
    app.manage(awake::KeepAwake::start()?);
    midi::start(app.handle().clone())?;
    streamdeck::start(app.handle().clone())?;

    app.manage(AppState { engine: channels.engine.clone(), snapshot: channels.snapshot.clone(), db: db.clone() });
    let store = Storage::start(db, channels.events.clone())?;
    app.manage(store.clone());

    operator_window::exit_app_on_close(app, &channels, config.clone(), config_path);
    remote_server::start(app, &channels, &config, &dir);
    event_bridge::forward_engine_events(app.handle().clone(), &channels);
    let media = outputs::start(app, &channels, &config, &dir)?;
    start_engine_thread(channels, media, store)?;
    crate::system::check::start_if_due(app.handle(), &dir);
    Ok(())
}

fn start_engine_thread(channels: Channels, media: Arc<NativeMedia>, store: Storage) -> std::io::Result<()> {
    let Channels { engine_inputs, events, snapshot, shutting_down, .. } = channels;
    thread::Builder::new().name("engine".into()).spawn(move || {
        let media = NativeMediaBackend { media };
        EngineLoop { inputs: engine_inputs, assets: Files, media, store, snapshot, events, shutdown: shutting_down }.run()
    })?;
    Ok(())
}
