use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use core_engine::EngineCommand;
use media_engine::hwaccel::{self, DecoderChoice};
use tauri::Manager;

use crate::displays::OutputRoutes;
use crate::engine_host::EngineHandle;
use crate::input::InputState;
use crate::now_ms;
use crate::settings::{load_config, load_or_set_aside, AppConfig, ConfigStore, OutputState, CONFIG_FILENAME};

/// Loads output styles, display routes and input bindings. A file that cannot
/// be read is set aside and replaced by defaults; the returned warnings say so.
pub(super) fn load_output_and_input_settings(app: &tauri::App, dir: &Path) -> Vec<String> {
    let output_path = dir.join("output-settings.json");
    let output_config = load_or_set_aside(&output_path, now_ms());
    let routes_path = dir.join("output-displays.json");
    let saved_routes = load_or_set_aside(&routes_path, now_ms());
    let (input_state, input_aside) = InputState::load(dir, now_ms());

    let warnings = [output_config.set_aside.as_ref(), saved_routes.set_aside.as_ref(), input_aside.as_ref()]
        .into_iter()
        .flatten()
        .map(|aside| format!("A settings file could not be read, so defaults are in use. The original was kept as {}.", aside.display()))
        .collect();

    app.manage(input_state);
    app.manage(OutputState { config: Mutex::new(output_config.value), path: output_path });
    app.manage(OutputRoutes::new(saved_routes.value, routes_path));
    warnings
}

/// Loads `app-config.json` and applies the parts the engine and decoders need
/// before anything is cued.
pub(super) fn load_app_config(app: &tauri::App, dir: &Path, engine: &EngineHandle) -> (Arc<RwLock<AppConfig>>, PathBuf) {
    let path = dir.join(CONFIG_FILENAME);
    let config = load_config(&path);
    hwaccel::set_choice(DecoderChoice::parse(&config.video_decoder));
    if let Err(e) = engine.command(EngineCommand::SetAutoplay { enabled: config.media_autoplay }) {
        log::warn!("Applying media autoplay: {e}");
    }
    let config = Arc::new(RwLock::new(config));
    app.manage(ConfigStore { value: config.clone(), path: path.clone() });
    (config, path)
}
