//! MIDI controllers: every connected input port is listened to, and ports
//! that appear or disappear are picked up within a couple of seconds. Notes,
//! control changes and program changes become input triggers.

use std::collections::HashMap;
use std::time::Duration;

use protocol::{InputTrigger, MidiMessage};
use tauri::Manager;

use crate::input::{self, InputState};

const RESCAN: Duration = Duration::from_secs(2);
const CLIENT: &str = "PAKIT";

/// The trigger in one MIDI message, if it is a press: note on with velocity,
/// a control rising to 64 or more (from below), or a program change.
/// `controls` remembers each control's last value per channel.
pub fn parse(message: &[u8], controls: &mut HashMap<(u8, u8), u8>) -> Option<InputTrigger> {
    let (&status, data) = message.split_first()?;
    let channel = (status & 0x0f) + 1;
    match (status & 0xf0, data) {
        (0x90, &[number, velocity, ..]) if velocity > 0 => Some(InputTrigger::Midi { message: MidiMessage::Note, channel, number }),
        (0xb0, &[number, value, ..]) => {
            let previous = controls.insert((channel, number), value).unwrap_or(0);
            (value >= 64 && previous < 64).then_some(InputTrigger::Midi { message: MidiMessage::Control, channel, number })
        }
        (0xc0, &[number, ..]) => Some(InputTrigger::Midi { message: MidiMessage::Program, channel, number }),
        _ => None,
    }
}

/// Starts the listener thread. Without MIDI support on the system it logs
/// once and stops.
pub fn start(app: tauri::AppHandle) -> anyhow::Result<()> {
    std::thread::Builder::new().name("midi".into()).spawn(move || run(app))?;
    Ok(())
}

fn run(app: tauri::AppHandle) {
    let mut connections: HashMap<String, midir::MidiInputConnection<()>> = HashMap::new();
    let mut warned = false;
    loop {
        match midir::MidiInput::new(CLIENT) {
            Ok(scanner) => {
                let ports: Vec<(String, midir::MidiInputPort)> = scanner
                    .ports()
                    .into_iter()
                    .filter_map(|port| scanner.port_name(&port).ok().map(|name| (name, port)))
                    // Our own client's ports would echo back; ALSA's loopback is no controller.
                    .filter(|(name, _)| !name.starts_with(CLIENT) && !name.starts_with("Midi Through"))
                    .collect();
                let before = connections.len();
                connections.retain(|name, _| ports.iter().any(|(n, _)| n == name));
                if connections.len() != before {
                    log::info!("MIDI input disconnected; {} connected", connections.len());
                }
                for (name, port) in ports {
                    if connections.contains_key(&name) {
                        continue;
                    }
                    let Ok(input) = midir::MidiInput::new(CLIENT) else { continue };
                    let handle = app.clone();
                    let mut controls = HashMap::new();
                    match input.connect(&port, "pakit-input", move |_, message, _| {
                        if let Some(trigger) = parse(message, &mut controls) {
                            input::triggered(&handle, trigger);
                        }
                    }, ()) {
                        Ok(connection) => {
                            log::info!("MIDI input connected: {name}");
                            connections.insert(name, connection);
                        }
                        Err(e) => log::warn!("Cannot open MIDI input {name}: {e}"),
                    }
                }
                let names: Vec<String> = connections.keys().cloned().collect();
                app.state::<InputState>().set_devices(|devices| devices.midi = names);
            }
            Err(e) if !warned => {
                warned = true;
                log::warn!("MIDI is unavailable: {e}");
            }
            Err(_) => {}
        }
        std::thread::sleep(RESCAN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presses_become_triggers() {
        let mut controls = HashMap::new();
        assert_eq!(parse(&[0x91, 60, 100], &mut controls), Some(InputTrigger::Midi { message: MidiMessage::Note, channel: 2, number: 60 }));
        assert_eq!(parse(&[0x91, 60, 0], &mut controls), None, "note on with zero velocity is a release");
        assert_eq!(parse(&[0x81, 60, 64], &mut controls), None, "note off");
        assert_eq!(parse(&[0xc0, 5], &mut controls), Some(InputTrigger::Midi { message: MidiMessage::Program, channel: 1, number: 5 }));
    }

    #[test]
    fn a_control_triggers_once_as_it_rises_past_the_middle() {
        let mut controls = HashMap::new();
        let cc = |value| [0xb0, 64, value];
        assert_eq!(parse(&cc(127), &mut controls), Some(InputTrigger::Midi { message: MidiMessage::Control, channel: 1, number: 64 }));
        assert_eq!(parse(&cc(120), &mut controls), None, "held");
        assert_eq!(parse(&cc(0), &mut controls), None, "released");
        assert!(parse(&cc(127), &mut controls).is_some(), "pressed again");
    }
}
