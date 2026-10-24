//! Remote access settings and status: LAN access, the pairing code and
//! paired devices, the `palto.local` name, and OSC. The servers themselves
//! are in `network_server`; this keeps their settings in `app-config.json`
//! and applies changes without a restart.

use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use network_server::mdns::{self, Advertiser};
use network_server::osc::{OscArg, OscListen, OscStatus};
use network_server::pairing::Pairing;
use network_server::{Listen, ServerStatus};
use protocol::InputAction;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::settings::{AppConfig, ConfigStore};

pub struct RemoteState {
    pub pairing: Arc<Pairing>,
    pub listen: watch::Sender<Listen>,
    pub server: Arc<Mutex<ServerStatus>>,
    pub osc: watch::Sender<OscListen>,
    pub osc_status: Arc<Mutex<OscStatus>>,
    pub advertiser: Advertiser,
}

pub fn listen(config: &AppConfig) -> Listen {
    Listen { lan: config.lan_enabled, port: config.ws_port }
}

pub fn osc_listen(config: &AppConfig) -> OscListen {
    OscListen {
        enabled: config.osc_enabled,
        lan: config.lan_enabled,
        port: config.osc_port,
        allowed: config.osc_allowed.iter().filter_map(|ip| ip.parse().ok()).collect(),
    }
}

impl RemoteState {
    /// Applies saved settings to the servers and the name advertisement.
    pub fn apply(&self, config: &AppConfig) {
        self.listen.send_replace(listen(config));
        self.osc.send_replace(osc_listen(config));
        self.advertiser.set(config.lan_enabled.then_some(config.ws_port));
    }
}

/// `/palto/<action>` (an input action such as `next`, `black` or
/// `play_pause`) performs it; a release (0 or false) is ignored, so
/// buttons that send press and release act once.
pub fn osc_action(address: &str, args: &[OscArg]) -> Option<InputAction> {
    let name = address.strip_prefix("/palto/")?;
    if args.first().is_some_and(OscArg::is_release) {
        return None;
    }
    let name = match name {
        "prev" => "previous",
        other => other,
    };
    serde_json::from_value(serde_json::Value::String(name.to_string())).ok()
}

#[derive(Serialize)]
pub struct DeviceInfo {
    id: String,
    name: String,
    paired_ms: i64,
    last_seen_ms: i64,
}

#[derive(Serialize)]
pub struct OscInfo {
    enabled: bool,
    port: u16,
    allowed: Vec<String>,
    listening: bool,
    error: Option<String>,
    /// Sender, address, milliseconds ago, refused.
    last: Option<(String, String, u64, bool)>,
}

#[derive(Serialize)]
pub struct RemoteStatus {
    lan_enabled: bool,
    port: u16,
    listening: bool,
    error: Option<String>,
    remote_clients: usize,
    /// `http://palto.local:<port>/`.
    name_url: String,
    /// The same page by address, private networks first.
    address_urls: Vec<String>,
    /// QR code of the first address URL.
    qr_svg: Option<String>,
    pairing_code: Option<String>,
    pairing_seconds_left: u64,
    devices: Vec<DeviceInfo>,
    osc: OscInfo,
}

#[tauri::command]
pub fn get_remote_status(remote: tauri::State<RemoteState>, config: tauri::State<ConfigStore>) -> Result<RemoteStatus, String> {
    let config = config.get();
    let server = remote.server.lock().map_err(|e| e.to_string())?.clone();
    let address_urls: Vec<String> = if config.lan_enabled {
        mdns::lan_addresses().into_iter().map(|ip| format!("http://{ip}:{}/", config.ws_port)).collect()
    } else {
        Vec::new()
    };
    let code = remote.pairing.code(Instant::now());
    let osc = remote.osc_status.lock().map_err(|e| e.to_string())?.clone();
    Ok(RemoteStatus {
        lan_enabled: config.lan_enabled,
        port: config.ws_port,
        listening: server.listening.is_some(),
        error: server.error,
        remote_clients: server.remote_clients,
        name_url: format!("http://{}:{}/", mdns::HOSTNAME, config.ws_port),
        qr_svg: address_urls.first().and_then(|url| network_server::qr_svg(url)),
        address_urls,
        pairing_code: code.as_ref().map(|(code, _)| code.clone()),
        pairing_seconds_left: code.map_or(0, |(_, left)| left.as_secs()),
        devices: remote
            .pairing
            .devices()
            .into_iter()
            .map(|d| DeviceInfo { id: d.id, name: d.name, paired_ms: d.paired_ms, last_seen_ms: d.last_seen_ms })
            .collect(),
        osc: OscInfo {
            enabled: config.osc_enabled,
            port: config.osc_port,
            allowed: config.osc_allowed.clone(),
            listening: osc.listening.is_some(),
            error: osc.error,
            last: osc.last.map(|(from, address, at, refused)| {
                let ago = SystemTime::now().duration_since(at).map_or(0, |d| d.as_millis() as u64);
                (from.to_string(), address, ago, refused)
            }),
        },
    })
}

#[derive(Deserialize)]
pub struct RemoteSettings {
    lan_enabled: bool,
    port: u16,
    osc_enabled: bool,
    osc_port: u16,
    osc_allowed: Vec<String>,
}

fn check_port(port: u16, what: &str) -> Result<(), String> {
    if port < 1024 {
        return Err(format!("Choose a {what} port from 1024 to 65535"));
    }
    Ok(())
}

/// Saves and applies the remote settings at once; connections from other
/// devices close when LAN access is turned off.
#[tauri::command]
pub fn set_remote_settings(remote: tauri::State<RemoteState>, config: tauri::State<ConfigStore>, settings: RemoteSettings) -> Result<(), String> {
    check_port(settings.port, "remote")?;
    check_port(settings.osc_port, "OSC")?;
    if settings.osc_enabled && settings.osc_port == settings.port {
        // TCP and UDP could share it, but one number for two things confuses setups.
        return Err("Use different ports for the remote and OSC".into());
    }
    let mut allowed = Vec::new();
    for entry in settings.osc_allowed.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let ip: IpAddr = entry.parse().map_err(|_| format!("{entry} is not an IP address (for example 192.168.1.30)"))?;
        allowed.push(ip.to_string());
    }
    let saved = config.update(|c| {
        c.lan_enabled = settings.lan_enabled;
        c.ws_port = settings.port;
        c.osc_enabled = settings.osc_enabled;
        c.osc_port = settings.osc_port;
        c.osc_allowed = allowed;
    })?;
    log::info!("Remote settings: LAN {}, port {}, OSC {} on {}", saved.lan_enabled, saved.ws_port, saved.osc_enabled, saved.osc_port);
    remote.apply(&saved);
    Ok(())
}

/// Shows a pairing code for a few minutes; returns it.
#[tauri::command]
pub fn start_pairing(remote: tauri::State<RemoteState>) -> String {
    remote.pairing.start(Instant::now())
}

#[tauri::command]
pub fn stop_pairing(remote: tauri::State<RemoteState>) {
    remote.pairing.stop();
}

/// Forgets a device; its open connection closes within a few seconds.
#[tauri::command]
pub fn remove_remote_device(remote: tauri::State<RemoteState>, id: String) -> Result<(), String> {
    remote.pairing.remove(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc_addresses_name_input_actions() {
        assert_eq!(osc_action("/palto/next", &[]), Some(InputAction::Next));
        assert_eq!(osc_action("/palto/prev", &[]), Some(InputAction::Previous));
        assert_eq!(osc_action("/palto/play_pause", &[OscArg::Int(1)]), Some(InputAction::PlayPause));
        assert_eq!(osc_action("/palto/black", &[OscArg::Float(0.0)]), None, "a release does nothing");
        assert_eq!(osc_action("/palto/explode", &[]), None);
        assert_eq!(osc_action("/other/next", &[]), None);
    }
}
