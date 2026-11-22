use std::sync::Arc;

use tauri::Manager;

use super::report::Report;
use crate::capture::Captures;
use crate::input::InputState;
use crate::settings::ConfigStore;

pub fn audio(report: &mut Report) {
    report.section("Audio");
    let (default, all) = media_engine::describe_devices();
    match default {
        Some(device) => {
            let hdmi = ["hdmi", "high definition audio", "displayport"].iter().any(|word| device.to_lowercase().contains(word));
            report.ok(format!("Default output: {device}"));
            if hdmi {
                report.info("Sound goes to a display over HDMI or DisplayPort; PALTO follows it when the default device changes");
            }
        }
        None => report.fail("No default audio output; videos will play silently"),
    }
    let mut seen = std::collections::BTreeSet::new();
    let unique: Vec<String> = all.into_iter().filter(|name| seen.insert(name.clone())).collect();
    if !unique.is_empty() {
        report.info(format!("Output devices: {}", unique.join("; ")));
    }
}

pub fn controls(report: &mut Report, app: &tauri::AppHandle) {
    report.section("Controllers and capture");
    let devices = app.state::<InputState>().devices();
    let list = |names: &[String]| if names.is_empty() { "none connected".to_string() } else { names.join(", ") };
    report.info(format!("MIDI: {}", list(&devices.midi)));
    report.info(format!("Stream Deck: {}", list(&devices.stream_decks)));
    match app.try_state::<Arc<Captures>>().map(|c| c.method_labels()) {
        Some(labels) if !labels.is_empty() => report.ok(format!("Screen capture: {}", labels.join(", "))),
        _ => report.warn("No screen capture method is available"),
    }
    match keep_awake() {
        Some(true) => report.ok("The displays can be kept awake"),
        Some(false) => report.warn("Windows refused to keep the displays awake"),
        None => {}
    }
}

pub fn network(report: &mut Report, app: &tauri::AppHandle) {
    report.section("Network");
    let config = app.state::<ConfigStore>().get();
    let addresses = network_server::mdns::lan_addresses();
    if addresses.is_empty() {
        report.warn("No network connection; the phone remote and Companion cannot connect");
    }
    if let Some((first, others)) = addresses.split_first() {
        report.ok(format!("Phone remote address: http://{first}:{}", config.ws_port));
        if !others.is_empty() {
            report.info(format!("Other addresses: {}", others.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ")));
        }
    }
    report.info(format!("LAN access is {}", if config.lan_enabled { "on" } else { "off (Settings → Remote)" }));
    report.info(format!("Lyrics sharing with OBS is {}", if config.feed_enabled { "on" } else { "off (Settings → Remote)" }));
    match firewall_rule() {
        Some(true) => report.ok("Windows Firewall has a PALTO rule"),
        Some(false) => report.warn("Windows Firewall has no PALTO rule; Windows will ask when LAN access is turned on"),
        None => {}
    }
}

#[cfg(windows)]
fn keep_awake() -> Option<bool> {
    use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS};
    Some(unsafe { SetThreadExecutionState(ES_CONTINUOUS) } != 0)
}

#[cfg(not(windows))]
fn keep_awake() -> Option<bool> {
    None
}

#[cfg(windows)]
fn firewall_rule() -> Option<bool> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let status = std::process::Command::new("netsh")
        .args(["advfirewall", "firewall", "show", "rule", "name=PALTO"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    Some(status.status.success())
}

#[cfg(not(windows))]
fn firewall_rule() -> Option<bool> {
    None
}
