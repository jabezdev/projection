use std::path::Path;
use std::sync::{Arc, RwLock};

use core_engine::EngineSnapshot;
use network_server::osc::OscSink;
use network_server::pairing::{self, Pairing};
use network_server::{mdns, CommandSink, RemoteServer};
use tauri::Manager;
use tokio::sync::watch;

use super::Channels;
use crate::settings::AppConfig;
use crate::{input, remote};

pub(super) fn start(app: &tauri::App, channels: &Channels, config: &RwLock<AppConfig>, dir: &Path) {
    let saved = config.read().expect("config lock poisoned").clone();
    let pairing = Arc::new(Pairing::load(dir.join(pairing::FILENAME)));
    let (listen, listen_rx) = watch::channel(remote::listen(&saved));
    let (osc, osc_rx) = watch::channel(remote::osc_listen(&saved));

    let engine = channels.engine.clone();
    let sink: CommandSink = Arc::new(move |command| engine.command(command));
    let snapshot = channels.snapshot.clone();
    let snapshot_provider: Arc<dyn Fn() -> EngineSnapshot + Send + Sync> = Arc::new(move || snapshot.read().expect("snapshot lock poisoned").clone());
    let server = RemoteServer::new(listen_rx, sink, channels.events.clone(), snapshot_provider, pairing.clone(), channels.shutdown_rx.clone());

    let state = remote::RemoteState {
        pairing,
        listen,
        server: server.status(),
        osc,
        osc_status: Arc::default(),
        advertiser: mdns::Advertiser::default(),
    };
    let osc_status = state.osc_status.clone();
    state.apply(&saved);
    app.manage(state);

    tauri::async_runtime::spawn(async move {
        if let Err(e) = server.run().await {
            log::error!("Network server failed: {}", e);
        }
    });

    let handle = app.handle().clone();
    let osc_sink: OscSink = Arc::new(move |address, args| match remote::osc_action(address, args) {
        Some(action) => {
            if let Err(e) = input::perform(&handle, action) {
                log::warn!("OSC {address}: {e}");
            }
        }
        None => log::debug!("Unhandled OSC {address}"),
    });
    tauri::async_runtime::spawn(network_server::osc::run(osc_rx, osc_sink, osc_status, channels.shutdown_rx.clone()));
}
