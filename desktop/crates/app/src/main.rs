// Release builds are GUI apps on Windows: no console window beside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod awake;
mod backup;
mod commands;
mod displays;
mod engine_host;
mod input;
mod library;
mod logging;
mod midi;
mod native_media;
mod native_output;
mod native_scene;
mod recovery;
mod settings;
mod storage;
mod streamdeck;
use log::{error, info, warn};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::thread;
use native_output::NativeOutputs;
use tauri::{Emitter, Manager};

use commands::*;
use core_engine::{EngineCommand, EngineEvent, EngineSnapshot, Files};
use displays::*;
use engine_host::{EngineHandle, EngineLoop};
use native_media::{NativeMedia, NativeMediaBackend};
use network_server::WsServer;
use persistence::Repository;
use protocol::ProjectModel;
use settings::*;
use tokio::sync::{broadcast, watch};

pub(crate) fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// The single database connection, shared by IPC commands and the engine.
#[derive(Clone)]
pub(crate) struct Db(Arc<Mutex<Repository>>);

impl Db {
    pub(crate) fn lock(&self) -> anyhow::Result<MutexGuard<'_, Repository>> {
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))
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

/// Saves still queued at exit get this long to reach the disk.
const EXIT_FLUSH: std::time::Duration = std::time::Duration::from_secs(5);

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
fn main() -> anyhow::Result<()> {
    logging::init();
    info!("Starting PALTO {}", env!("CARGO_PKG_VERSION"));

    native_output::prepare_platform();

    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let (engine, engine_rx) = EngineHandle::channel();

    // Engine events (broadcast to UI + WS clients)
    let (event_tx, _) = broadcast::channel::<EngineEvent>(1024);

    // Authoritative state snapshot for initial sync (UI + WS server)
    let snapshot: Arc<RwLock<EngineSnapshot>> = Arc::new(RwLock::new(EngineSnapshot::default()));

    let snapshot_provider: Arc<dyn Fn() -> EngineSnapshot + Send + Sync> = {
        let snapshot = snapshot.clone();
        Arc::new(move || snapshot.read().expect("snapshot lock poisoned").clone())
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_output_sizes,
            get_output_preview,
            preview_output_style,
            get_output_displays,
            set_output_display,
            set_outputs_shown,
            get_startup_warnings,
            open_logs_folder,
            get_app_info,
            get_video_decoding,
            set_video_decoder,
            export_backup,
            open_backups_folder,
            list_missing_media,
            relink_media,
            import_openlyrics,
            input::get_input_bindings,
            input::set_input_bindings,
            input::reset_input_bindings,
            input::trigger_action,
            input::learn_input,
            input::get_input_devices,
            get_output_state,
            set_output_state,
            resolve_media_path,
            video_thumbnail,
            choose_presentation_images,
            play,
            pause,
            stop,
            nav_next,
            nav_prev,
            cue,
            engine_command,
            go_to_item,
            go_to_step,
            seek,
            set_active_text,
            set_background,
            set_display,
            get_engine_snapshot,
            list_projects,
            create_project,
            get_project,
            load_project,
            save_project,
            list_songs,
            create_song,
            get_song,
            update_song,
            list_media,
            create_media,
            choose_media_file
        ])
        .setup(move |app| {
            let app_data_dir = data_dir(app.path().app_data_dir()?);
            std::fs::create_dir_all(&app_data_dir)?;
            let output_path = app_data_dir.join("output-settings.json");
            let output_config = settings::load_or_set_aside(&output_path, now_ms());
            let routes_path = app_data_dir.join("output-displays.json");
            let saved_routes = settings::load_or_set_aside(&routes_path, now_ms());
            let (input_state, input_aside) = input::InputState::load(&app_data_dir, now_ms());
            app.manage(input_state);
            let mut warnings: Vec<String> = [output_config.set_aside.as_ref(), saved_routes.set_aside.as_ref(), input_aside.as_ref()]
                .into_iter()
                .flatten()
                .map(|aside| format!("A settings file could not be read, so defaults are in use. The original was kept as {}.", aside.display()))
                .collect();
            app.manage(OutputState { config: Mutex::new(output_config.value), path: output_path });
            app.manage(OutputRoutes { values: Mutex::new(saved_routes.value), path: routes_path, shown: AtomicBool::new(false) });
            let config_path = app_data_dir.join(CONFIG_FILENAME);
            let config = Arc::new(RwLock::new(load_config(&config_path)));
            media_engine::hwaccel::set_choice(media_engine::hwaccel::DecoderChoice::parse(&config.read().expect("config lock poisoned").video_decoder));
            app.manage(ConfigStore { value: config.clone(), path: config_path.clone() });
            let db = Db(Arc::new(Mutex::new(Repository::new(
                app_data_dir.join(DB_FILENAME),
            )?)));

            // After a crash, reopen the last service at the last cue; the
            // engine applies these once its thread starts.
            let (recovery, previous) = recovery::Recovery::start(&app_data_dir);
            match previous {
                recovery::Previous::Clean => {}
                recovery::Previous::Restore(state) => {
                    let project = state.project_id.map(|id| db.lock().and_then(|db| db.get_project(id)));
                    match project {
                        Some(Ok(project)) => {
                            warn!("Previous run ended unexpectedly; reopening {} at its last cue", project.name);
                            for command in recovery::restore_commands(&state, &project) {
                                let _ = engine.command(command);
                            }
                            warnings.push(format!("PALTO did not close normally. \"{}\" was reopened at its last cue with media paused. Outputs stay hidden until you show them.", project.name));
                        }
                        _ => warnings.push("PALTO did not close normally, and the last service could not be reopened.".into()),
                    }
                }
                recovery::Previous::RepeatedCrash => {
                    warn!("Previous run crashed right after a recovery; not restoring again");
                    warnings.push("PALTO closed unexpectedly twice in a row, so the last service was not reopened automatically. Open it from the service list. The log folder (Settings) has details.".into());
                }
                recovery::Previous::Crashed => warnings.push("PALTO did not close normally last time.".into()),
            }
            let recovery = Arc::new(recovery);
            app.manage(recovery.clone());
            {
                let mut rx = event_tx.subscribe();
                let snapshot = snapshot.clone();
                thread::Builder::new().name("recovery".into()).spawn(move || loop {
                    match rx.blocking_recv() {
                        Ok(EngineEvent::ProjectChanged { .. } | EngineEvent::ProgramChanged { .. }) | Err(broadcast::error::RecvError::Lagged(_)) => {
                            let current = snapshot.read().expect("snapshot lock poisoned").clone();
                            recovery.observe(&current);
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                })?;
            }
            app.manage(StartupWarnings(warnings));
            app.manage(DataDir(app_data_dir.clone()));
            // A weekly automatic backup, checked shortly after start and then
            // every few hours, so a long-running app still makes one.
            {
                let (db, dir, handle) = (db.clone(), app_data_dir.clone(), app.handle().clone());
                thread::Builder::new().name("backup".into()).spawn(move || loop {
                    thread::sleep(std::time::Duration::from_secs(30));
                    let logo = handle.state::<OutputState>().config.lock().map(|c| c.logo_path.clone()).unwrap_or_default();
                    match db.lock().and_then(|db| backup::automatic_if_due(&db, &dir, &logo, std::time::SystemTime::now())) {
                        Ok(Some(path)) => info!("Automatic backup written to {}", path.display()),
                        Ok(None) => {}
                        Err(e) => error!("Automatic backup failed: {e:#}"),
                    }
                    thread::sleep(std::time::Duration::from_secs(6 * 60 * 60));
                })?;
            }
            app.manage(awake::KeepAwake::start()?);
            // Outside controllers share the input layer with the keyboard.
            midi::start(app.handle().clone())?;
            streamdeck::start(app.handle().clone())?;

            app.manage(AppState {
                engine: engine.clone(),
                snapshot: snapshot.clone(),
                db: db.clone(),
            });
            let store = Storage::start(db.clone(), event_tx.clone())?;
            app.manage(store.clone());

            let main_window = app.get_webview_window("main").unwrap();

            // Graceful shutdown + persist config
            {
                let shutdown_flag = shutdown_flag.clone();
                let shutdown_tx = shutdown_tx.clone();
                let config = config.clone();
                let handle = app.handle().clone();
                main_window.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                        shutdown_flag.store(true, Ordering::Release);
                        let _ = shutdown_tx.send(true);

                        if let Ok(cfg) = config.read() {
                            save_config(&config_path, &cfg);
                        }
                        // Outputs cannot be closed individually, so the operator
                        // window ends the application.
                        shutdown_native(&handle);
                        handle.exit(0);
                    }
                });
            }

            // Start WS server on Tauri runtime
            {
                let ws_addr: SocketAddr = {
                    let cfg = config.read().expect("config lock poisoned");
                    format!("{}:{}", cfg.ws_bind_host, cfg.ws_port).parse()?
                };

                let remote = engine.clone();
                let sink: network_server::CommandSink = Arc::new(move |command| remote.command(command));
                let server = WsServer::new(ws_addr, sink, event_tx.clone())
                    .with_snapshot_provider(snapshot_provider.clone())
                    .with_shutdown(shutdown_rx.clone());
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = server.run().await {
                        error!("Network server failed: {}", e);
                    }
                });
            }

            // Bridge EngineEvent -> Frontend (single stream)
            {
                let app_handle = app.handle().clone();
                let mut rx = event_tx.subscribe();
                let snapshot = snapshot.clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        match rx.recv().await {
                            Ok(evt) => {
                                let _ = app_handle.emit("engine:event", evt);
                            }
                            // Events carry whole domains, so the current snapshot
                            // replaces everything that was skipped.
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                                warn!("Operator UI missed {missed} engine events; resynchronizing");
                                let current = snapshot.read().expect("snapshot lock poisoned").clone();
                                for evt in current.sync_events() {
                                    let _ = app_handle.emit("engine:event", evt);
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                });
            }

            // Outputs start hidden; the operator shows them from the
            // operator window.
            let (width, height) = config
                .read()
                .expect("config lock poisoned")
                .projection_size
                .unwrap_or(WINDOWED_SIZE);
            let projection_window = native_output::build_output_window(app, OUTPUTS[0].0, OUTPUTS[0].1, (width, height))?;
            let confidence_window = native_output::build_output_window(app, OUTPUTS[1].0, OUTPUTS[1].1, WINDOWED_SIZE)?;
            #[cfg(target_os = "linux")]
            for window in [&projection_window, &confidence_window] {
                let (handle, label) = (app.handle().clone(), window.label().to_string());
                native_output::refullscreen_on_map(window, move || {
                    handle.state::<OutputRoutes>().wants_fullscreen(&label)
                })?;
            }
            for window in [&projection_window, &confidence_window] {
                let handle = app.handle().clone();
                let output = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Resized(size) = event {
                        let _ = handle.emit_to("main", "output:resized", content_size(&output, *size));
                    }
                });
            }

            // One decoder per session and one audio output, shared by both
            // output windows.
            let media = NativeMedia::start(engine.clone())?;
            app.manage(media.clone());
            native_output::start(app, [&projection_window, &confidence_window], media.frames())?;
            // Scenes follow every engine change and output style edit.
            let feed = {
                let handle = app.handle().clone();
                let apply_handle = app.handle().clone();
                let ambient = media.clone();
                native_scene::SceneFeed::start(
                    snapshot.clone(),
                    move || handle.state::<OutputState>().config.lock().map(|c| c.clone()).unwrap_or_default(),
                    move |scenes, plan| {
                        ambient.set_ambient(&plan.ambient, &plan.prepared_ambient);
                        ambient.prepare(plan.foreground.as_deref());
                        if let Some(outputs) = apply_handle.try_state::<NativeOutputs>() {
                            for (role, scene) in scenes {
                                outputs.set_scene(role, scene);
                            }
                            outputs.preload(&plan.images);
                        }
                    },
                )?
            };
            {
                let feed = feed.clone();
                let mut rx = event_tx.subscribe();
                tauri::async_runtime::spawn(async move {
                    loop {
                        match rx.recv().await {
                            Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => feed.refresh(),
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                });
            }
            app.manage(feed);
            show_outputs_if_requested(app.handle());

            // Remember the projection size for the next start.
            {
                let config = config.clone();
                projection_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Resized(size) = event {
                        if let Ok(mut cfg) = config.write() {
                            cfg.projection_size = Some((size.width, size.height));
                        }
                    }
                });
            }

            let inputs = engine_rx.clone();
            let snapshot = snapshot.clone();
            let events = event_tx.clone();
            let shutdown = shutdown_flag.clone();
            thread::Builder::new().name("engine".into()).spawn(move || {
                let media = NativeMediaBackend { media };
                EngineLoop { inputs, assets: Files, media, store, snapshot, events, shutdown }.run()
            })?;

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Release native surfaces before Tauri destroys their windows.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                shutdown_native(app);
            }
        });

    Ok(())
}

/// Writes pending project saves, then stops native presentation (and its
/// frame polling) before media decoders. Safe to call more than once.
fn shutdown_native<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(store) = app.try_state::<Storage>() {
        if !store.flush(EXIT_FLUSH) {
            error!("Exiting with project saves still pending");
        }
    }
    if let Some(outputs) = app.try_state::<NativeOutputs>() {
        outputs.shutdown();
    }
    if let Some(awake) = app.try_state::<awake::KeepAwake>() {
        awake.set(false);
    }
    if let Some(media) = app.try_state::<Arc<NativeMedia>>() {
        media.shutdown();
    }
    if let Some(recovery) = app.try_state::<Arc<recovery::Recovery>>() {
        recovery.clean_exit();
    }
}
