use std::path::{Path, PathBuf};
use std::sync::Mutex;

use log::error;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tauri::{Emitter, Manager};
use crate::output::scene;

pub fn write_atomic(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temporary, bytes)?;
    if let Err(e) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(e.into());
    }
    Ok(())
}

pub fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

pub struct Loaded<T> {
    pub value: T,
    pub set_aside: Option<PathBuf>,
}

pub fn load_or_set_aside<T: DeserializeOwned + Default>(path: &Path, now_ms: i64) -> Loaded<T> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Loaded { value: T::default(), set_aside: None },
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => Loaded { value, set_aside: None },
        Err(e) => {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("settings");
            let aside = path.with_file_name(format!("{stem}.invalid-{now_ms}.json"));
            match std::fs::rename(path, &aside) {
                Ok(()) => log::error!("{} is unreadable ({e}); kept as {} and using defaults", path.display(), aside.display()),
                Err(rename) => log::error!("{} is unreadable ({e}) and could not be set aside ({rename})", path.display()),
            }
            Loaded { value: T::default(), set_aside: Some(aside) }
        }
    }
}

pub const CONFIG_FILENAME: &str = "app-config.json";
pub const DB_FILENAME: &str = "app.db";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub ws_port: u16,
    pub projection_size: Option<(u32, u32)>,
    pub video_decoder: String,
    pub lan_enabled: bool,
    pub feed_enabled: bool,
    pub osc_enabled: bool,
    pub osc_port: u16,
    pub osc_allowed: Vec<String>,
    pub capture_method: String,
    pub media_autoplay: bool,
    pub decoder_chosen: bool,
    pub system_check_version: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ws_port: 8080,
            projection_size: None,
            video_decoder: "software".to_string(),
            lan_enabled: false,
            feed_enabled: false,
            osc_enabled: false,
            osc_port: 9000,
            osc_allowed: Vec::new(),
            capture_method: String::new(),
            media_autoplay: true,
            decoder_chosen: false,
            system_check_version: String::new(),
        }
    }
}

pub struct ConfigStore {
    pub value: std::sync::Arc<std::sync::RwLock<AppConfig>>,
    pub path: PathBuf,
}

impl ConfigStore {
    pub fn update(&self, change: impl FnOnce(&mut AppConfig)) -> Result<AppConfig, String> {
        let mut config = self.value.write().map_err(|e| e.to_string())?;
        change(&mut config);
        write_atomic(&self.path, &*config).map_err(|e| format!("Saving {}: {e}", self.path.display()))?;
        Ok(config.clone())
    }

    pub fn get(&self) -> AppConfig {
        self.value.read().map(|c| c.clone()).unwrap_or_default()
    }
}

#[derive(Serialize)]
pub struct DecoderOption {
    name: String,
    label: String,
}

#[derive(Serialize)]
pub struct VideoDecoding {
    choice: String,
    available: Vec<DecoderOption>,
}

#[tauri::command]
pub async fn get_video_decoding(config: tauri::State<'_, ConfigStore>) -> Result<VideoDecoding, String> {
    let choice = config.get().video_decoder;
    let available = tauri::async_runtime::spawn_blocking(media_engine::hwaccel::available).await.map_err(|e| e.to_string())?;
    Ok(VideoDecoding {
        choice,
        available: available.into_iter().map(|(name, label)| DecoderOption { name: name.into(), label: label.into() }).collect(),
    })
}

#[tauri::command]
pub fn set_video_decoder(config: tauri::State<ConfigStore>, choice: String) -> Result<(), String> {
    let parsed = media_engine::hwaccel::DecoderChoice::parse(&choice);
    if parsed == media_engine::hwaccel::DecoderChoice::Software && choice != "software" {
        return Err(format!("Unknown video decoder {choice}"));
    }
    config.update(|c| {
        c.video_decoder = choice.clone();
        c.decoder_chosen = true;
    })?;
    log::info!("Video decoder set to {choice}");
    media_engine::hwaccel::set_choice(parsed);
    Ok(())
}

#[tauri::command]
pub fn get_media_autoplay(config: tauri::State<ConfigStore>) -> bool {
    config.get().media_autoplay
}

#[tauri::command]
pub fn set_media_autoplay(config: tauri::State<ConfigStore>, state: tauri::State<crate::AppState>, enabled: bool) -> Result<(), String> {
    config.update(|c| c.media_autoplay = enabled)?;
    state.send(core_engine::EngineCommand::SetAutoplay { enabled })
}

pub const DATA_DIR_ENV: &str = "PALTO_DATA_DIR";

pub fn data_dir(new_data_dir: std::path::PathBuf) -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV).filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir);
    }
    let legacy_data_dir = new_data_dir.with_file_name("com.tauri.dev");
    if !new_data_dir.join(DB_FILENAME).exists() && legacy_data_dir.join(DB_FILENAME).exists() {
        legacy_data_dir
    } else {
        new_data_dir
    }
}

pub fn load_config(path: &Path) -> AppConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_config(path: &Path, config: &AppConfig) {
    if let Err(e) = write_atomic(path, config) {
        error!("Saving {}: {e}", path.display());
    }
}

pub struct OutputState {
    pub config: Mutex<protocol::OutputConfig>,
    pub path: std::path::PathBuf,
}

#[tauri::command]
pub fn get_output_state(state: tauri::State<OutputState>, domain: String) -> Result<protocol::OutputConfig, String> {
    if domain != "config" { return Err("Unknown output domain".into()); }
    Ok(state.config.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
pub fn set_output_state(state: tauri::State<OutputState>, app: tauri::AppHandle, domain: String, value: protocol::OutputConfig) -> Result<(), String> {
    if domain != "config" { return Err("Unknown output domain".into()); }
    value.validate()?;
    let mut guard = state.config.lock().map_err(|e| e.to_string())?;
    write_atomic(&state.path, &value).map_err(|e| format!("Saving output settings: {e}"))?;
    *guard = value.clone();
    drop(guard);
    if let Some(feed) = app.try_state::<scene::SceneFeed>() {
        feed.refresh();
    }
    if let Some(state) = app.try_state::<crate::AppState>() {
        if state.db.fitter.set_style(scene::lyric_style(&value)) {
            let _ = state.send(protocol::EngineCommand::RefreshService);
        }
    }
    let event = serde_json::json!({ "domain": domain, "value": value });
    app.emit("output:changed", event).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::OutputConfig;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("magi-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn existing_settings_load_and_save_atomically() {
        let dir = temp_dir();
        let path = dir.join("output-settings.json");
        std::fs::write(&path, r#"{"font":"Inter","size":4.5,"lineHeight":0.9}"#).unwrap();
        let loaded: Loaded<OutputConfig> = load_or_set_aside(&path, 1);
        assert_eq!((loaded.value.font.as_str(), loaded.value.size, loaded.value.line_height), ("Inter", 4.5, 0.9));
        assert!(loaded.set_aside.is_none());

        write_atomic(&path, &OutputConfig { size: 7.0, ..loaded.value }).unwrap();
        let reloaded: Loaded<OutputConfig> = load_or_set_aside(&path, 2);
        assert_eq!((reloaded.value.font.as_str(), reloaded.value.size), ("Inter", 7.0));
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name()).collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("output-settings.json")], "no temporary files remain");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn malformed_settings_are_kept_aside_not_overwritten() {
        let dir = temp_dir();
        let path = dir.join("output-settings.json");
        std::fs::write(&path, "{ this is not json").unwrap();
        let loaded: Loaded<OutputConfig> = load_or_set_aside(&path, 1234);
        assert_eq!(loaded.value, OutputConfig::default());
        let aside = loaded.set_aside.unwrap();
        assert_eq!(aside.file_name().unwrap(), "output-settings.invalid-1234.json");
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "{ this is not json");
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_file_uses_defaults() {
        let dir = temp_dir();
        let loaded: Loaded<OutputConfig> = load_or_set_aside(&dir.join("absent.json"), 1);
        assert_eq!(loaded.value, OutputConfig::default());
        assert!(loaded.set_aside.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn app_configs_saved_with_an_output_backend_still_load() {
        let config: AppConfig = serde_json::from_str(r#"{"ws_bind_host":"0.0.0.0","ws_port":9000,"projection_size":null,"output_backend":"webview"}"#).unwrap();
        assert_eq!(config.ws_port, 9000);
        assert!(!config.lan_enabled, "an old all-interfaces host no longer opens the remote to the network");
        assert_eq!(config.video_decoder, "software");
    }
}
