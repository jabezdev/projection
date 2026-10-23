//! Elgato Stream Decks: every connected deck is read on its own thread, and
//! decks plugged in later are found within a few seconds. A key press (not
//! its release) becomes an input trigger numbered from the top-left key.
//!
//! On Linux the hidraw devices need a udev rule giving the user access
//! (see README.md); without it no deck is listed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use elgato_streamdeck::{list_devices, new_hidapi, refresh_device_list, StreamDeck, StreamDeckInput};
use protocol::InputTrigger;
use tauri::Manager;

use crate::input::{self, InputState};

const RESCAN: Duration = Duration::from_secs(3);
const READ_TIMEOUT: Duration = Duration::from_millis(500);

/// Keys that went from up to down between two state reports.
pub fn pressed(previous: &[bool], current: &[bool]) -> Vec<u8> {
    current.iter().enumerate().filter(|&(i, &down)| down && !previous.get(i).copied().unwrap_or(false)).map(|(i, _)| i as u8).collect()
}

pub fn start(app: tauri::AppHandle) -> anyhow::Result<()> {
    std::thread::Builder::new().name("stream-deck".into()).spawn(move || run(app))?;
    Ok(())
}

fn run(app: tauri::AppHandle) {
    // Serial number → product name, for decks with a reader running.
    let connected: Arc<Mutex<HashMap<String, String>>> = Arc::default();
    let mut api = None;
    let mut warned = false;
    loop {
        if api.is_none() {
            match new_hidapi() {
                Ok(new) => api = Some(new),
                Err(e) if !warned => {
                    warned = true;
                    log::warn!("Stream Deck support is unavailable: {e}");
                }
                Err(_) => {}
            }
        }
        if let Some(api) = api.as_mut() {
            let _ = refresh_device_list(api);
            for (kind, serial) in list_devices(api) {
                if connected.lock().map(|c| c.contains_key(&serial)).unwrap_or(true) {
                    continue;
                }
                match StreamDeck::connect(api, kind, &serial) {
                    Ok(deck) => {
                        let product = deck.product().unwrap_or_else(|_| format!("{kind:?}"));
                        log::info!("Stream Deck connected: {product} ({serial})");
                        if let Ok(mut c) = connected.lock() {
                            c.insert(serial.clone(), product);
                        }
                        let (app, connected) = (app.clone(), connected.clone());
                        let spawned = std::thread::Builder::new().name("stream-deck-read".into()).spawn(move || {
                            read(&app, &deck);
                            if let Ok(mut c) = connected.lock() {
                                c.remove(&serial);
                            }
                            log::info!("Stream Deck disconnected ({serial})");
                        });
                        if let Err(e) = spawned {
                            log::error!("Starting a Stream Deck reader: {e}");
                        }
                    }
                    Err(e) => log::warn!("Cannot open Stream Deck {serial}: {e}"),
                }
            }
        }
        let names: Vec<String> = connected.lock().map(|c| c.values().cloned().collect()).unwrap_or_default();
        app.state::<InputState>().set_devices(|devices| devices.stream_decks = names);
        std::thread::sleep(RESCAN);
    }
}

/// Reads key changes until the deck is unplugged.
fn read(app: &tauri::AppHandle, deck: &StreamDeck) {
    let mut keys: Vec<bool> = Vec::new();
    loop {
        match deck.read_input(Some(READ_TIMEOUT)) {
            Ok(StreamDeckInput::ButtonStateChange(current)) => {
                for button in pressed(&keys, &current) {
                    input::triggered(app, InputTrigger::StreamDeck { button });
                }
                keys = current;
            }
            Ok(_) => {}
            Err(e) => {
                log::debug!("Stream Deck read: {e}");
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_new_presses_trigger() {
        assert_eq!(pressed(&[], &[false, true, false]), [1]);
        assert_eq!(pressed(&[false, true, false], &[true, true, false]), [0], "a held key does not repeat");
        assert!(pressed(&[true, true], &[false, false]).is_empty(), "releases do nothing");
    }
}
