mod engine_host;
mod native_media;
mod native_output;
mod native_scene;
mod settings;
mod storage;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::thread;
use native_output::NativeOutputs;
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

use core_engine::{EngineCommand, EngineEvent, EngineSnapshot, Projector};
use engine_host::{EngineHandle, EngineLoop};
use native_media::{NativeMedia, NativeMediaBackend};
use network_server::WsServer;
use persistence::Repository;
use protocol::{ProjectHeader, ProjectModel, SongHeader, SongModel};
use tokio::sync::{broadcast, watch};

const CONFIG_FILENAME: &str = "app-config.json";
const DB_FILENAME: &str = "app.db";
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

/// All runtime state lives in the OS app-data dir, never the workspace. An
/// existing MVP library under the pre-release identifier stays in use.
fn data_dir(new_data_dir: std::path::PathBuf) -> std::path::PathBuf {
    let legacy_data_dir = new_data_dir.with_file_name("com.tauri.dev");
    if !new_data_dir.join(DB_FILENAME).exists() && legacy_data_dir.join(DB_FILENAME).exists() {
        legacy_data_dir
    } else {
        new_data_dir
    }
}

fn load_config(path: &Path) -> AppConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_config(path: &Path, config: &AppConfig) {
    if let Err(e) = settings::write_atomic(path, config) {
        error!("Saving {}: {e}", path.display());
    }
}

pub(crate) fn now_ms() -> i64 {
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

struct AppState {
    engine: EngineHandle,
    snapshot: Arc<RwLock<EngineSnapshot>>,
    db: Db,
}

impl AppState {
    fn send(&self, cmd: EngineCommand) -> Result<(), String> {
        self.engine.command(cmd)
    }
}

/// Problems found while starting, shown to the operator once the UI loads.
struct StartupWarnings(Vec<String>);

#[tauri::command]
fn get_startup_warnings(warnings: tauri::State<StartupWarnings>) -> Vec<String> {
    warnings.0.clone()
}

struct OutputState {
    config: Mutex<protocol::OutputConfig>,
    path: std::path::PathBuf,
}

#[tauri::command]
fn get_output_state(state: tauri::State<OutputState>, domain: String) -> Result<protocol::OutputConfig, String> {
    if domain != "config" { return Err("Unknown output domain".into()); }
    Ok(state.config.lock().map_err(|e| e.to_string())?.clone())
}

/// Validates before saving, so the outputs never receive a style they cannot render.
#[tauri::command]
fn set_output_state(state: tauri::State<OutputState>, app: tauri::AppHandle, domain: String, value: protocol::OutputConfig) -> Result<(), String> {
    if domain != "config" { return Err("Unknown output domain".into()); }
    value.validate()?;
    let mut guard = state.config.lock().map_err(|e| e.to_string())?;
    settings::write_atomic(&state.path, &value).map_err(|e| format!("Saving output settings: {e}"))?;
    *guard = value.clone();
    drop(guard);
    if let Some(feed) = app.try_state::<native_scene::SceneFeed>() {
        feed.refresh();
    }
    let event = serde_json::json!({ "domain": domain, "value": value });
    app.emit("output:changed", event).map_err(|e| e.to_string())
}

fn monitor_label(index: usize, monitor: &tauri::Monitor) -> String {
    format!("{}: {} ({}×{})", index + 1, monitor.name().map(String::as_str).unwrap_or("Display"), monitor.size().width, monitor.size().height)
}

#[tauri::command]
fn list_output_monitors(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let window = app.get_webview_window("main").ok_or("Control window unavailable")?;
    Ok(window.available_monitors().map_err(|e| e.to_string())?.iter().enumerate().map(|(i, m)| monitor_label(i, m)).collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OutputRoute {
    name: Option<String>, x: i32, y: i32, width: u32, height: u32, fullscreen: bool,
}
struct OutputRoutes {
    values: Mutex<std::collections::BTreeMap<String, OutputRoute>>,
    path: std::path::PathBuf,
}
fn matching_route(route: &OutputRoute, candidates: &[OutputRoute]) -> Option<usize> {
    candidates.iter().position(|m| m.name == route.name && m.x == route.x && m.y == route.y)
        .or_else(|| {
            let matches: Vec<_> = candidates.iter().enumerate().filter(|(_, m)| route.name.is_some() && m.name == route.name).map(|(i, _)| i).collect();
            if matches.len() == 1 { Some(matches[0]) } else {
                let matches: Vec<_> = candidates.iter().enumerate().filter(|(_, m)| m.name == route.name && m.width == route.width && m.height == route.height).map(|(i, _)| i).collect();
                if matches.len() == 1 { Some(matches[0]) } else { None }
            }
        })
}
fn matching_monitor(route: &OutputRoute, monitors: &[tauri::Monitor]) -> Option<usize> {
    let candidates: Vec<_> = monitors.iter().map(|m| OutputRoute { name: m.name().cloned(), x: m.position().x, y: m.position().y, width: m.size().width, height: m.size().height, fullscreen: false }).collect();
    matching_route(route, &candidates)
}
fn apply_output_route(window: &tauri::Window, target: &tauri::Monitor, fullscreen: bool) -> Result<(), String> {
    window.set_fullscreen(false).map_err(|e| e.to_string())?;
    window.set_position(*target.position()).map_err(|e| e.to_string())?;
    window.set_decorations(!fullscreen).map_err(|e| e.to_string())?;
    window.set_fullscreen(fullscreen).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())
}
#[tauri::command]
fn get_output_routes(app: tauri::AppHandle, routes: tauri::State<OutputRoutes>) -> Result<serde_json::Value, String> {
    let window = app.get_webview_window("main").ok_or("Control window unavailable")?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let saved = routes.values.lock().map_err(|e| e.to_string())?;
    let mut result = serde_json::Map::new();
    for (output, route) in saved.iter() {
        result.insert(output.clone(), serde_json::json!({ "monitor": matching_monitor(route, &monitors), "fullscreen": route.fullscreen, "name": route.name }));
    }
    Ok(serde_json::Value::Object(result))
}
#[tauri::command]
fn place_output(app: tauri::AppHandle, routes: tauri::State<OutputRoutes>, output: String, monitor: usize, label: Option<String>, fullscreen: bool) -> Result<(), String> {
    if output != "projection" && output != "confidence" { return Err("Unknown output".into()); }
    let window = app.get_window(&output).ok_or("Output window unavailable")?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let target = monitors.get(monitor).ok_or("Display is no longer connected")?;
    // The list may have changed since the operator chose from it.
    if label.is_some_and(|label| label != monitor_label(monitor, target)) {
        return Err("The connected displays changed. Close and reopen settings to choose again.".into());
    }
    apply_output_route(&window, target, fullscreen)?;
    let mut saved = routes.values.lock().map_err(|e| e.to_string())?;
    saved.insert(output, OutputRoute { name: target.name().cloned(), x: target.position().x, y: target.position().y, width: target.size().width, height: target.size().height, fullscreen });
    settings::write_atomic(&routes.path, &*saved).map_err(|e| e.to_string())
}

/// The latest native preview still of an output ("projection", "confidence")
/// or of the style sample ("style"), as JPEG bytes.
#[tauri::command]
fn get_output_preview(previews: tauri::State<native_output::PreviewStore>, target: String) -> Result<tauri::ipc::Response, String> {
    previews.jpeg(&target).map(tauri::ipc::Response::new).ok_or_else(|| format!("No {target} preview yet"))
}

/// Renders the settings dialog's sample lyrics in `config` natively.
#[tauri::command]
fn preview_output_style(outputs: tauri::State<NativeOutputs>, config: protocol::OutputConfig) -> Result<(), String> {
    config.validate()?;
    outputs.preview_style(render_engine::Scene::Main(native_scene::style_sample_scene(&config)));
    Ok(())
}

#[tauri::command]
fn resolve_media_path(app: tauri::AppHandle, path: String) -> Result<String, String> {
    let file = std::fs::canonicalize(&path).map_err(|e| format!("Cannot open media: {e}"))?;
    if !file.is_file() { return Err("Media must be a file".into()); }
    app.asset_protocol_scope().allow_file(&file).map_err(|e| e.to_string())?;
    Ok(file.to_string_lossy().into_owned())
}

#[tauri::command]
async fn video_thumbnail(app: tauri::AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::hash::{Hash, Hasher};
        let file = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        file.hash(&mut hash); metadata.len().hash(&mut hash); metadata.modified().ok().hash(&mut hash);
        let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("video-thumbnails");
        std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        let poster = cache.join(format!("{:x}.jpg", hash.finish()));
        if !poster.exists() {
            // Decoded in-process: packaged builds ship FFmpeg libraries, not its CLI.
            let frame = media_engine::poster(&file.to_string_lossy(), 640).map_err(|e| format!("Thumbnail: {e:#}"))?;
            let temporary = cache.join(format!("{}.jpg", uuid::Uuid::new_v4()));
            image::save_buffer_with_format(&temporary, &frame.rgb, frame.width, frame.height, image::ColorType::Rgb8, image::ImageFormat::Jpeg).map_err(|e| e.to_string())?;
            std::fs::rename(&temporary, &poster).map_err(|e| e.to_string())?;
        }
        app.asset_protocol_scope().allow_file(&poster).map_err(|e| e.to_string())?;
        Ok(poster.to_string_lossy().into_owned())
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn choose_presentation_images(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Choose presentation images").add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp"]).pick_files(move |files| { let _ = sender.send(files); });
    let files = receiver.await.map_err(|e| e.to_string())?.unwrap_or_default();
    files.into_iter().map(|file| {
        let path = file.into_path().map_err(|e| e.to_string())?;
        app.asset_protocol_scope().allow_file(&path).map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    }).collect()
}

#[tauri::command]
fn play(state: tauri::State<AppState>) -> Result<(), String> {
    state.send(EngineCommand::Play)
}

#[tauri::command]
fn pause(state: tauri::State<AppState>) -> Result<(), String> {
    state.send(EngineCommand::Pause)
}

#[tauri::command]
fn stop(state: tauri::State<AppState>) -> Result<(), String> {
    state.send(EngineCommand::Stop)
}

#[tauri::command]
fn set_active_text(state: tauri::State<AppState>, text: String) -> Result<(), String> {
    state.send(EngineCommand::SetActiveText { text })
}

#[tauri::command]
fn set_display(state: tauri::State<AppState>, display: protocol::DisplayMode) -> Result<(), String> {
    state.send(EngineCommand::SetDisplay { display })
}

#[tauri::command]
fn set_background(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    state.send(EngineCommand::SetBackground { path })
}

#[tauri::command]
fn nav_next(state: tauri::State<AppState>) -> Result<(), String> {
    state.send(EngineCommand::Next)
}

#[tauri::command]
fn nav_prev(state: tauri::State<AppState>) -> Result<(), String> {
    state.send(EngineCommand::Prev)
}

#[tauri::command]
fn cue(state: tauri::State<AppState>, item_id: i64, index: u32) -> Result<(), String> {
    state.send(EngineCommand::Cue { item_id, index })
}

/// Any typed engine command; local controls use the same vocabulary as
/// WebSocket clients.
#[tauri::command]
fn engine_command(state: tauri::State<AppState>, command: EngineCommand) -> Result<(), String> {
    state.send(command)
}

#[tauri::command]
fn go_to_item(state: tauri::State<AppState>, item_id: i64) -> Result<(), String> {
    state.send(EngineCommand::GoToItem { item_id })
}

#[tauri::command]
fn go_to_step(state: tauri::State<AppState>, index: u32) -> Result<(), String> {
    state.send(EngineCommand::GoToStep { index })
}

#[tauri::command]
fn seek(state: tauri::State<AppState>, time_ms: i64) -> Result<(), String> {
    state.send(EngineCommand::Seek { time_ms })
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
fn load_project(state: tauri::State<AppState>, project_id: i64) -> Result<(), String> {
    state.send(EngineCommand::LoadProject { project_id })
}

#[tauri::command]
fn save_project(state: tauri::State<AppState>, project: ProjectModel) -> Result<(), String> {
    state.send(EngineCommand::SaveProject { project })
}

#[tauri::command]
fn list_songs(state: tauri::State<AppState>) -> Result<Vec<SongHeader>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_songs().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_song(state: tauri::State<AppState>, title: String, lyrics: String) -> Result<SongModel, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.create_song(title, lyrics).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_song(state: tauri::State<AppState>, id: i64) -> Result<SongModel, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_song(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_song(state: tauri::State<AppState>, song: SongModel) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_song(&song).map_err(|e| e.to_string())
}

#[tauri::command]
async fn choose_media_file(app: tauri::AppHandle, kind: String) -> Result<Option<String>, String> {
    let extensions: &[&str] = match kind.as_str() {
        "image" => &["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        "audio" => &["mp3", "wav", "ogg", "flac", "m4a", "aac"],
        "video" => &["mp4", "mov", "mkv", "webm", "avi"],
        "logo" => &["png", "jpg", "jpeg", "webp", "gif", "bmp", "mp4", "m4v", "mov", "mkv", "webm", "avi"],
        _ => &["png", "jpg", "jpeg", "webp", "mp4", "mov", "mkv", "webm"],
    };
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Choose media").add_filter("Media files", extensions).pick_file(move |file| { let _ = sender.send(file); });
    receiver.await.map_err(|e| e.to_string())?.map(|file| file.into_path().map(|path| path.to_string_lossy().into_owned()).map_err(|e| e.to_string())).transpose()
}

#[tauri::command]
fn list_media(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<Vec<protocol::MediaModel>, String> {
    let media = state.db.lock().map_err(|e| e.to_string())?.list_media().map_err(|e| e.to_string())?;
    for item in &media { app.asset_protocol_scope().allow_file(&item.path).map_err(|e| e.to_string())?; }
    Ok(media)
}

#[tauri::command]
fn create_media(state: tauri::State<AppState>, app: tauri::AppHandle, title: String, kind: protocol::MediaKind, path: String) -> Result<protocol::MediaModel, String> {
    let file = std::fs::canonicalize(path.trim()).map_err(|e| format!("Cannot open media file: {e}"))?;
    if !file.is_file() { return Err("Choose a media file, not a directory.".into()); }
    if title.trim().is_empty() { return Err("Enter a media title.".into()); }
    app.asset_protocol_scope().allow_file(&file).map_err(|e| e.to_string())?;
    state.db.lock().map_err(|e| e.to_string())?.create_media(title.trim().into(), kind, file.to_string_lossy().into()).map_err(|e| e.to_string())
}

/// Current output dimensions for matching operator preview aspect ratios.
#[tauri::command]
fn get_output_sizes(app: tauri::AppHandle) -> serde_json::Value {
    let mut sizes = serde_json::Map::new();
    for label in ["projection", "confidence"] {
        if let Some(window) = app.get_window(label) {
            if let Ok(size) = window.inner_size() { sizes.insert(label.into(), serde_json::json!({ "width": size.width, "height": size.height })); }
        }
    }
    serde_json::Value::Object(sizes)
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    info!("Starting projection-rust Tauri Edition...");

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
            list_output_monitors,
            place_output,
            get_output_routes,
            get_startup_warnings,
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
            let warnings = [output_config.set_aside.as_ref(), saved_routes.set_aside.as_ref()]
                .into_iter()
                .flatten()
                .map(|aside| format!("A settings file could not be read, so defaults are in use. The original was kept as {}.", aside.display()))
                .collect();
            app.manage(StartupWarnings(warnings));
            app.manage(OutputState { config: Mutex::new(output_config.value), path: output_path });
            app.manage(OutputRoutes { values: Mutex::new(saved_routes.value), path: routes_path });
            let config_path = app_data_dir.join(CONFIG_FILENAME);
            let config = Arc::new(RwLock::new(load_config(&config_path)));
            let db = Db(Arc::new(Mutex::new(Repository::new(
                app_data_dir.join(DB_FILENAME),
            )?)));

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

            // A disconnected saved display leaves an ordinary window rather than
            // unexpectedly taking over the operator's current monitor.
            let has_saved_projection = app.state::<OutputRoutes>().values.lock().expect("output route lock poisoned").contains_key("projection");
            // Create Projection Window
            let (width, height) = config
                .read()
                .expect("config lock poisoned")
                .projection_size
                .unwrap_or((1280, 720));
            let projection_window = native_output::build_projection_window(app, has_saved_projection, (width, height))?;
            let confidence_window = native_output::build_confidence_window(app)?;

            {
                let routes = app.state::<OutputRoutes>();
                let saved = routes.values.lock().expect("output route lock poisoned");
                for window in [&projection_window, &confidence_window] {
                    if let Some(route) = saved.get(window.label()) {
                        if let Ok(monitors) = window.available_monitors() {
                            if let Some(index) = matching_monitor(route, &monitors) {
                                if let Err(error) = apply_output_route(window, &monitors[index], route.fullscreen) { error!("Restoring {} display: {}", window.label(), error); }
                            }
                        }
                    }
                }
            }
            for window in [&projection_window, &confidence_window] {
                let handle = app.handle().clone();
                let label = window.label().to_string();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Resized(size) = event {
                        let _ = handle.emit_to("main", "output:resized", serde_json::json!({ "label": label, "width": size.width, "height": size.height }));
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
                    move |scenes, videos| {
                        ambient.set_ambient(&videos);
                        if let Some(outputs) = apply_handle.try_state::<NativeOutputs>() {
                            for (role, scene) in scenes {
                                outputs.set_scene(role, scene);
                            }
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

            // Listen for Resizes
            {
                let engine = engine.clone();
                let config = config.clone();
                projection_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Resized(size) = event {
                        let _ = engine.command(EngineCommand::Resize {
                            width: size.width,
                            height: size.height,
                        });

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
                EngineLoop { inputs, output: Projector, media, store, snapshot, events, shutdown }.run()
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
    if let Some(media) = app.try_state::<Arc<NativeMedia>>() {
        media.shutdown();
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn app_configs_saved_with_an_output_backend_still_load() {
        let config: AppConfig = serde_json::from_str(r#"{"ws_bind_host":"0.0.0.0","ws_port":9000,"projection_size":null,"output_backend":"webview"}"#).unwrap();
        assert_eq!(config.ws_port, 9000);
    }
}

#[cfg(test)]
mod output_route_tests {
    use super::*;
    fn display(name: &str, x: i32) -> OutputRoute { OutputRoute { name: Some(name.into()), x, y: 0, width: 1920, height: 1080, fullscreen: true } }
    #[test]
    fn restores_named_display_after_enumeration_or_position_changes() {
        let audience = display("HDMI-A-1", 1920);
        assert_eq!(matching_route(&audience, &[audience.clone(), display("eDP-1", 0)]), Some(0));
        assert_eq!(matching_route(&audience, &[display("eDP-1", 0), display("HDMI-A-1", 0)]), Some(1));
    }
    #[test]
    fn missing_or_ambiguous_display_does_not_redirect_output() {
        let audience = display("HDMI-A-1", 1920);
        assert_eq!(matching_route(&audience, &[display("eDP-1", 0)]), None);
        assert_eq!(matching_route(&audience, &[display("HDMI-A-1", 0), display("HDMI-A-1", 3840)]), None);
    }
    #[test]
    fn saved_routes_preserve_separate_fullscreen_choices() {
        let audience = display("HDMI-A-1", 1920);
        let mut confidence = display("DP-1", -1920);
        confidence.fullscreen = false;
        let routes = std::collections::BTreeMap::from([("projection", audience), ("confidence", confidence)]);
        let loaded: std::collections::BTreeMap<String, OutputRoute> = serde_json::from_slice(&serde_json::to_vec(&routes).unwrap()).unwrap();
        assert!(loaded["projection"].fullscreen);
        assert!(!loaded["confidence"].fullscreen);
        assert_eq!(loaded["confidence"].name.as_deref(), Some("DP-1"));
    }
}
