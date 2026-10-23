//! The input layer: keyboard bindings (forwarded by the operator UI), MIDI
//! controllers and Stream Decks all trigger the same actions, which become
//! engine commands here. Bindings live in `input-bindings.json`.
//!
//! While the bindings editor is learning, the next MIDI or Stream Deck
//! trigger is reported to the UI (`input:learned`) instead of acting.

use std::path::PathBuf;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use protocol::{DisplayMode, EngineCommand, InputAction, InputBindings, InputTrigger, MediaAction};
use tauri::{Emitter, Manager};

use crate::settings::{load_or_set_aside, write_atomic};
use crate::AppState;

pub const FILENAME: &str = "input-bindings.json";
/// Learning ends on its own if nothing is pressed.
const LEARN_TIMEOUT: Duration = Duration::from_secs(30);

pub struct InputState {
    bindings: RwLock<InputBindings>,
    path: PathBuf,
    learning: Mutex<Option<Instant>>,
    /// Connected devices by kind, for the bindings editor.
    devices: Mutex<InputDevices>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct InputDevices {
    pub midi: Vec<String>,
    pub stream_decks: Vec<String>,
}

impl InputState {
    /// Loads the saved bindings; a missing file gives the default map, an
    /// unreadable one is set aside (reported by the caller).
    pub fn load(dir: &std::path::Path, now_ms: i64) -> (Self, Option<PathBuf>) {
        let path = dir.join(FILENAME);
        let loaded = load_or_set_aside::<InputBindings>(&path, now_ms);
        let state = Self { bindings: RwLock::new(loaded.value), path, learning: Mutex::default(), devices: Mutex::default() };
        (state, loaded.set_aside)
    }

    pub fn set_devices(&self, update: impl FnOnce(&mut InputDevices)) {
        if let Ok(mut devices) = self.devices.lock() {
            update(&mut devices);
        }
    }
}

/// The engine commands for `action`, given what is on air now.
pub fn commands(action: InputAction, display: DisplayMode, playing: Option<bool>) -> Vec<EngineCommand> {
    let toggle = |mode: DisplayMode| EngineCommand::SetDisplay { display: if display == mode { DisplayMode::Live } else { mode } };
    match action {
        InputAction::Next => vec![EngineCommand::Next],
        InputAction::Previous => vec![EngineCommand::Prev],
        InputAction::NextItem => vec![EngineCommand::NextItem],
        InputAction::PreviousItem => vec![EngineCommand::PrevItem],
        InputAction::Live => vec![EngineCommand::SetDisplay { display: DisplayMode::Live }],
        InputAction::Black => vec![toggle(DisplayMode::Black)],
        InputAction::Clear => vec![toggle(DisplayMode::Clear)],
        InputAction::Logo => vec![toggle(DisplayMode::Logo)],
        InputAction::PlayPause => match playing {
            Some(true) => vec![EngineCommand::Media { session_id: None, action: MediaAction::Pause }],
            Some(false) => vec![EngineCommand::Media { session_id: None, action: MediaAction::Play }],
            None => Vec::new(),
        },
        InputAction::Stop => playing.map(|_| vec![EngineCommand::Media { session_id: None, action: MediaAction::Stop }]).unwrap_or_default(),
    }
}

/// Performs `action` on the engine.
pub fn perform(app: &tauri::AppHandle, action: InputAction) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (display, playing) = {
        let snapshot = state.snapshot.read().map_err(|_| "snapshot lock poisoned")?;
        (snapshot.program_state.display, snapshot.playback.as_ref().map(|p| !p.paused))
    };
    for command in commands(action, display, playing) {
        state.send(command)?;
    }
    Ok(())
}

/// A trigger from a MIDI controller or Stream Deck: learned while the
/// editor listens, otherwise performed if bound.
pub fn triggered(app: &tauri::AppHandle, trigger: InputTrigger) {
    let input = app.state::<InputState>();
    let learning = input.learning.lock().ok().and_then(|mut learning| learning.take_if(|since| since.elapsed() < LEARN_TIMEOUT).map(|_| ()));
    if learning.is_some() {
        let _ = app.emit_to("main", "input:learned", &trigger);
        return;
    }
    let action = input.bindings.read().ok().and_then(|b| b.action_for(&trigger));
    match action {
        Some(action) => {
            log::debug!("{trigger:?} → {action:?}");
            if let Err(e) = perform(app, action) {
                log::warn!("Input {action:?}: {e}");
            }
        }
        None => log::debug!("Unbound input {trigger:?}"),
    }
}

#[tauri::command]
pub fn get_input_bindings(input: tauri::State<InputState>) -> Result<InputBindings, String> {
    input.bindings.read().map(|b| b.clone()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_input_bindings(app: tauri::AppHandle, input: tauri::State<InputState>, bindings: InputBindings) -> Result<(), String> {
    bindings.validate()?;
    write_atomic(&input.path, &bindings).map_err(|e| format!("Saving input bindings: {e}"))?;
    *input.bindings.write().map_err(|e| e.to_string())? = bindings.clone();
    app.emit("input:bindings", bindings).map_err(|e| e.to_string())
}

/// Restores the default keyboard map (MIDI and Stream Deck bindings are removed).
#[tauri::command]
pub fn reset_input_bindings(app: tauri::AppHandle, input: tauri::State<InputState>) -> Result<(), String> {
    set_input_bindings(app, input, InputBindings::default())
}

/// Keyboard bindings are matched in the operator UI, which sends the action.
#[tauri::command]
pub fn trigger_action(app: tauri::AppHandle, action: InputAction) -> Result<(), String> {
    perform(&app, action)
}

/// Starts or stops listening for the next MIDI or Stream Deck trigger.
#[tauri::command]
pub fn learn_input(input: tauri::State<InputState>, learning: bool) -> Result<(), String> {
    *input.learning.lock().map_err(|e| e.to_string())? = learning.then(Instant::now);
    Ok(())
}

#[tauri::command]
pub fn get_input_devices(input: tauri::State<InputState>) -> Result<InputDevices, String> {
    input.devices.lock().map(|d| d.clone()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_clear_and_logo_toggle_back_to_live() {
        assert!(matches!(commands(InputAction::Black, DisplayMode::Live, None).as_slice(), [EngineCommand::SetDisplay { display: DisplayMode::Black }]));
        assert!(matches!(commands(InputAction::Black, DisplayMode::Black, None).as_slice(), [EngineCommand::SetDisplay { display: DisplayMode::Live }]));
        assert!(matches!(commands(InputAction::Logo, DisplayMode::Black, None).as_slice(), [EngineCommand::SetDisplay { display: DisplayMode::Logo }]));
    }

    #[test]
    fn play_pause_follows_the_transport_and_needs_media() {
        assert!(matches!(commands(InputAction::PlayPause, DisplayMode::Live, Some(true)).as_slice(), [EngineCommand::Media { action: MediaAction::Pause, .. }]));
        assert!(matches!(commands(InputAction::PlayPause, DisplayMode::Live, Some(false)).as_slice(), [EngineCommand::Media { action: MediaAction::Play, .. }]));
        assert!(commands(InputAction::PlayPause, DisplayMode::Live, None).is_empty());
        assert!(commands(InputAction::Stop, DisplayMode::Live, None).is_empty());
    }
}
