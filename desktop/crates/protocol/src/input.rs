//! The operator input layer: what an operator can do from a keyboard,
//! presenter clicker, MIDI controller or Stream Deck, and which trigger does
//! what. Saved in `input-bindings.json`; every device shares one vocabulary.

use serde::{Deserialize, Serialize};

pub const INPUT_BINDINGS_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InputAction {
    /// Next slide, or the next item after the last slide.
    Next,
    Previous,
    NextItem,
    PreviousItem,
    Live,
    /// Black, Clear and Logo toggle: pressed again, they return to Live.
    Black,
    Clear,
    Logo,
    PlayPause,
    Stop,
}

impl InputAction {
    pub const ALL: [InputAction; 10] = [
        InputAction::Next,
        InputAction::Previous,
        InputAction::NextItem,
        InputAction::PreviousItem,
        InputAction::Live,
        InputAction::Black,
        InputAction::Clear,
        InputAction::Logo,
        InputAction::PlayPause,
        InputAction::Stop,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MidiMessage {
    /// Note on with a velocity above zero.
    Note,
    /// Control change, triggered when the value rises to 64 or more.
    Control,
    Program,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "device", rename_all = "snake_case")]
pub enum InputTrigger {
    /// `KeyboardEvent.key`, with single characters lowercase and the space
    /// bar as `Space`. Modifiers must match exactly.
    Key {
        key: String,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        shift: bool,
    },
    /// From any connected MIDI input. Channels are 1–16.
    Midi { message: MidiMessage, channel: u8, number: u8 },
    /// A key on any connected Stream Deck, numbered from 0.
    StreamDeck { button: u8 },
}

impl InputTrigger {
    pub fn key(key: &str) -> Self {
        InputTrigger::Key { key: key.into(), ctrl: false, alt: false, shift: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputBinding {
    pub trigger: InputTrigger,
    pub action: InputAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct InputBindings {
    pub version: u32,
    pub bindings: Vec<InputBinding>,
}

impl Default for InputBindings {
    /// Arrows, space and the PageUp/PageDown that presenter clickers send;
    /// B (and the `.` many clickers send for their black button), C and L
    /// toggle Black, Clear and Logo; Escape returns to Live.
    fn default() -> Self {
        let keys = [
            ("ArrowRight", InputAction::Next),
            ("Space", InputAction::Next),
            ("PageDown", InputAction::Next),
            ("ArrowLeft", InputAction::Previous),
            ("PageUp", InputAction::Previous),
            ("ArrowDown", InputAction::NextItem),
            ("ArrowUp", InputAction::PreviousItem),
            ("Escape", InputAction::Live),
            ("b", InputAction::Black),
            (".", InputAction::Black),
            ("c", InputAction::Clear),
            ("l", InputAction::Logo),
            ("p", InputAction::PlayPause),
        ];
        let bindings = keys.into_iter().map(|(key, action)| InputBinding { trigger: InputTrigger::key(key), action }).collect();
        Self { version: INPUT_BINDINGS_VERSION, bindings }
    }
}

impl InputBindings {
    pub fn action_for(&self, trigger: &InputTrigger) -> Option<InputAction> {
        self.bindings.iter().find(|b| &b.trigger == trigger).map(|b| b.action)
    }

    /// Rejects a trigger bound twice (it could only ever do one thing).
    pub fn validate(&self) -> Result<(), String> {
        for (index, binding) in self.bindings.iter().enumerate() {
            if self.bindings[..index].iter().any(|b| b.trigger == binding.trigger) {
                return Err("The same key or button is bound twice".into());
            }
            if let InputTrigger::Midi { channel, number, .. } = binding.trigger {
                if !(1..=16).contains(&channel) || number > 127 {
                    return Err("MIDI channels are 1–16 and numbers 0–127".into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_map_covers_the_roadmap_keys() {
        let map = InputBindings::default();
        map.validate().unwrap();
        assert_eq!(map.action_for(&InputTrigger::key("ArrowRight")), Some(InputAction::Next));
        assert_eq!(map.action_for(&InputTrigger::key("Space")), Some(InputAction::Next));
        assert_eq!(map.action_for(&InputTrigger::key("ArrowLeft")), Some(InputAction::Previous));
        assert_eq!(map.action_for(&InputTrigger::key("PageDown")), Some(InputAction::Next), "presenter clickers");
        assert_eq!(map.action_for(&InputTrigger::key("b")), Some(InputAction::Black));
        let ctrl_b = InputTrigger::Key { key: "b".into(), ctrl: true, alt: false, shift: false };
        assert_eq!(map.action_for(&ctrl_b), None, "modifiers must match");
    }

    #[test]
    fn triggers_have_a_stable_wire_format() {
        let midi = InputTrigger::Midi { message: MidiMessage::Note, channel: 1, number: 60 };
        assert_eq!(serde_json::to_value(&midi).unwrap(), serde_json::json!({"device": "midi", "message": "note", "channel": 1, "number": 60}));
        let key: InputTrigger = serde_json::from_str(r#"{"device":"key","key":"b"}"#).unwrap();
        assert_eq!(key, InputTrigger::key("b"));
    }

    #[test]
    fn duplicate_triggers_are_rejected() {
        let mut map = InputBindings::default();
        map.bindings.push(InputBinding { trigger: InputTrigger::key("b"), action: InputAction::Logo });
        assert!(map.validate().is_err());
    }
}
