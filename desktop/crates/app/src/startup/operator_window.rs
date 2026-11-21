use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

use tauri::Manager;

use super::Channels;
use crate::settings::{save_config, AppConfig};
use crate::shutdown_native;

pub(super) fn exit_app_on_close(app: &tauri::App, channels: &Channels, config: Arc<RwLock<AppConfig>>, config_path: PathBuf) {
    let Some(window) = app.get_webview_window("main") else {
        log::error!("The operator window is missing");
        return;
    };
    let shutting_down = channels.shutting_down.clone();
    let shutdown_tx = channels.shutdown_tx.clone();
    let handle = app.handle().clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
            shutting_down.store(true, Ordering::Release);
            let _ = shutdown_tx.send(true);
            if let Ok(config) = config.read() {
                save_config(&config_path, &config);
            }
            shutdown_native(&handle);
            handle.exit(0);
        }
    });
}
