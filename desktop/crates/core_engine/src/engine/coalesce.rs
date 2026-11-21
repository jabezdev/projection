use protocol::{
    EngineCommand,
    MediaAction,
};

use super::*;

pub fn coalesce(inputs: Vec<EngineInput>) -> Vec<EngineInput> {
    let mut merged: Vec<EngineInput> = Vec::with_capacity(inputs.len());
    for input in inputs {
        let input = match input {
            EngineInput::Command(cmd) => EngineInput::Command(cmd.normalize()),
            report => report,
        };
        if let Some(last) = merged.last_mut() {
            if supersedes(&input, last) {
                *last = input;
                continue;
            }
        }
        merged.push(input);
    }
    merged
}

fn supersedes(next: &EngineInput, previous: &EngineInput) -> bool {
    use EngineCommand::{Media, SaveService};
    use EngineInput::{Command, Report};
    match (previous, next) {
        (Report(a), Report(b)) => a.session_id == b.session_id && a.phase == b.phase,
        (
            Command(Media { session_id: a, action: MediaAction::Seek { .. } }),
            Command(Media { session_id: b, action: MediaAction::Seek { .. } }),
        )
        | (
            Command(Media { session_id: a, action: MediaAction::SetVolume { .. } }),
            Command(Media { session_id: b, action: MediaAction::SetVolume { .. } }),
        ) => a == b,
        (Command(SaveService { service: a }), Command(SaveService { service: b })) => a.id == b.id,
        _ => false,
    }
}
