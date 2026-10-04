use crossbeam::channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::thread;
use std::time::Duration;
use tauri::{Emitter, Manager};

use core_engine::{
    AudioEngine, Engine, EngineCommand, EngineEvent, EngineSnapshot, ProjectStore, Projector,
};
use network_server::WsServer;
use persistence::Repository;
use protocol::{ProjectHeader, ProjectModel};
use tokio::sync::{broadcast, watch};

const CONFIG_FILENAME: &str = "app-config.json";
const DB_FILENAME: &str = "app.db";
const INITIAL_BACKGROUND: &str = "assets/bg1.png";

/// Frame interval while something is moving (video).
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
/// Redraw interval while static. Repaints the surface in case the compositor
/// or the overlaying webview damaged it; content changes render immediately.
const IDLE_REDRAW_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppConfig {
    ws_bind_host: String,
    ws_port: u16,
    projection_size: Option<(u32, u32)>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ws_bind_host: "127.0.0.1".to_string(),
            ws_port: 8080,
            projection_size: None,
        }
    }
}

fn load_config(path: &Path) -> AppConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_config(path: &Path, config: &AppConfig) {
    if let Ok(text) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(path, text);
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// The single database connection, shared by IPC commands and the engine.
#[derive(Clone)]
struct Db(Arc<Mutex<Repository>>);

impl Db {
    fn lock(&self) -> anyhow::Result<MutexGuard<'_, Repository>> {
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))
    }
}

impl ProjectStore for Db {
    fn load_project(&self, project_id: i64) -> anyhow::Result<ProjectModel> {
        self.lock()?.get_project(project_id)
    }

    fn save_project(&self, project: &mut ProjectModel) -> anyhow::Result<()> {
        self.lock()?.save_project(project, now_ms())
    }
}

struct AppState {
    engine_tx: Sender<EngineCommand>,
    snapshot: Arc<RwLock<EngineSnapshot>>,
    db: Db,
}

impl AppState {
    fn send(&self, cmd: EngineCommand) {
        let _ = self.engine_tx.send(cmd);
    }
}

#[tauri::command]
fn play(state: tauri::State<AppState>) {
    state.send(EngineCommand::Play);
}

#[tauri::command]
fn pause(state: tauri::State<AppState>) {
    state.send(EngineCommand::Pause);
}

#[tauri::command]
fn stop(state: tauri::State<AppState>) {
    state.send(EngineCommand::Stop);
}

#[tauri::command]
fn set_active_text(state: tauri::State<AppState>, text: String) {
    state.send(EngineCommand::SetActiveText { text });
}

#[tauri::command]
fn set_background(state: tauri::State<AppState>, path: String) {
    state.send(EngineCommand::SetBackground { path });
}

#[tauri::command]
fn nav_next(state: tauri::State<AppState>) {
    state.send(EngineCommand::Next);
}

#[tauri::command]
fn nav_prev(state: tauri::State<AppState>) {
    state.send(EngineCommand::Prev);
}

#[tauri::command]
fn go_to_item(state: tauri::State<AppState>, item_id: i64) {
    state.send(EngineCommand::GoToItem { item_id });
}

#[tauri::command]
fn go_to_step(state: tauri::State<AppState>, index: u32) {
    state.send(EngineCommand::GoToStep { index });
}

#[tauri::command]
fn seek(state: tauri::State<AppState>, time_ms: i64) {
    state.send(EngineCommand::Seek { time_ms });
}

#[tauri::command]
fn get_engine_snapshot(state: tauri::State<AppState>) -> EngineSnapshot {
    state.snapshot.read().expect("snapshot lock poisoned").clone()
}

#[tauri::command]
fn list_projects(state: tauri::State<AppState>) -> Result<Vec<ProjectHeader>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_projects().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_project(state: tauri::State<AppState>, name: String) -> Result<ProjectModel, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.create_project(name, now_ms()).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_project(state: tauri::State<AppState>, project_id: i64) -> Result<ProjectModel, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_project(project_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_project(state: tauri::State<AppState>, project_id: i64) {
    state.send(EngineCommand::LoadProject { project_id });
}

#[tauri::command]
fn save_project(state: tauri::State<AppState>, project: ProjectModel) {
    state.send(EngineCommand::SaveProject { project });
}

/// Runs the engine until shutdown: apply commands, publish state, render.
///
/// Renders immediately after a state change, every frame while a video plays,
/// and otherwise only at the idle redraw interval.
fn run_engine(
    commands: Receiver<EngineCommand>,
    mut projector: Projector<'static>,
    db: Db,
    snapshot: Arc<RwLock<EngineSnapshot>>,
    events: broadcast::Sender<EngineEvent>,
    shutdown: Arc<AtomicBool>,
) {
    let mut engine = Engine::new();

    while !shutdown.load(Ordering::Acquire) {
        let wait = if projector.is_animating() {
            FRAME_INTERVAL
        } else {
            IDLE_REDRAW_INTERVAL
        };

        match commands.recv_timeout(wait) {
            Ok(cmd) => {
                // Apply the whole burst before rendering once.
                let mut emitted = engine.handle(cmd, &mut projector, &db);
                while let Ok(cmd) = commands.try_recv() {
                    emitted.extend(engine.handle(cmd, &mut projector, &db));
                }

                if !emitted.is_empty() {
                    *snapshot.write().expect("snapshot lock poisoned") = engine.snapshot().clone();
                    for evt in emitted {
                        let _ = events.send(evt);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        projector.render();
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    info!("Starting projection-rust Tauri Edition...");

    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let (engine_tx, engine_rx) = unbounded::<EngineCommand>();

    // Engine events (broadcast to UI + WS clients)
    let (event_tx, _) = broadcast::channel::<EngineEvent>(1024);

    // Authoritative state snapshot for initial sync (UI + WS server)
    let snapshot: Arc<RwLock<EngineSnapshot>> = Arc::new(RwLock::new(EngineSnapshot::default()));

    // Audio Engine (not wired up yet; reserved for playback + sync)
    let _audio_engine = AudioEngine::new()?;

    let snapshot_provider: Arc<dyn Fn() -> EngineSnapshot + Send + Sync> = {
        let snapshot = snapshot.clone();
        Arc::new(move || snapshot.read().expect("snapshot lock poisoned").clone())
    };

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            play,
            pause,
            stop,
            nav_next,
            nav_prev,
            go_to_item,
            go_to_step,
            seek,
            set_active_text,
            set_background,
            get_engine_snapshot,
            list_projects,
            create_project,
            get_project,
            load_project,
            save_project
        ])
        .setup(move |app| {
            // All runtime state lives in the OS app-data dir, never the workspace.
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let config_path = app_data_dir.join(CONFIG_FILENAME);
            let config = Arc::new(RwLock::new(load_config(&config_path)));
            let db = Db(Arc::new(Mutex::new(Repository::new(
                app_data_dir.join(DB_FILENAME),
            )?)));

            app.manage(AppState {
                engine_tx: engine_tx.clone(),
                snapshot: snapshot.clone(),
                db: db.clone(),
            });

            let main_window = app.get_webview_window("main").unwrap();

            // Graceful shutdown + persist config
            {
                let shutdown_flag = shutdown_flag.clone();
                let shutdown_tx = shutdown_tx.clone();
                let config = config.clone();
                main_window.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                        shutdown_flag.store(true, Ordering::Release);
                        let _ = shutdown_tx.send(true);

                        if let Ok(cfg) = config.read() {
                            save_config(&config_path, &cfg);
                        }
                    }
                });
            }

            // Start WS server on Tauri runtime
            {
                let ws_addr: SocketAddr = {
                    let cfg = config.read().expect("config lock poisoned");
                    format!("{}:{}", cfg.ws_bind_host, cfg.ws_port).parse()?
                };

                let server = WsServer::new(ws_addr, engine_tx.clone(), event_tx.clone())
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
                tauri::async_runtime::spawn(async move {
                    while let Ok(evt) = rx.recv().await {
                        let _ = app_handle.emit("engine:event", evt);
                    }
                });
            }

            // Create Projection Window
            let (width, height) = config
                .read()
                .expect("config lock poisoned")
                .projection_size
                .unwrap_or((1280, 720));
            let projection_window = tauri::WebviewWindowBuilder::new(
                app,
                "projection",
                tauri::WebviewUrl::App("projection.html".into()),
            )
            .title("Projection Output")
            .decorations(false) // Remove title bar and borders
            .transparent(true) // Help prevent flickering with native rendering
            .shadow(false)
            .fullscreen(true)
            .inner_size(width as f64, height as f64)
            .build()?;

            // Create Confidence Window (basic)
            let _confidence_window = tauri::WebviewWindowBuilder::new(
                app,
                "confidence",
                tauri::WebviewUrl::App("index.html?mode=confidence".into()),
            )
            .title("Confidence")
            .inner_size(540.0, 360.0)
            .resizable(true)
            .build()?;

            // Listen for Resizes
            {
                let engine_tx = engine_tx.clone();
                let config = config.clone();
                projection_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Resized(size) = event {
                        let _ = engine_tx.send(EngineCommand::Resize {
                            width: size.width,
                            height: size.height,
                        });

                        if let Ok(mut cfg) = config.write() {
                            cfg.projection_size = Some((size.width, size.height));
                        }
                    }
                });
            }

            let size = projection_window.inner_size()?;
            let projector = pollster::block_on(Projector::new(
                Arc::new(projection_window),
                size.width,
                size.height,
            ))
            .expect("Failed to init projection output");

            let _ = engine_tx.send(EngineCommand::SetBackground {
                path: INITIAL_BACKGROUND.to_string(),
            });

            let engine_rx = engine_rx.clone();
            let snapshot = snapshot.clone();
            let event_tx = event_tx.clone();
            let shutdown_flag = shutdown_flag.clone();
            thread::spawn(move || {
                run_engine(engine_rx, projector, db, snapshot, event_tx, shutdown_flag)
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    Ok(())
}
