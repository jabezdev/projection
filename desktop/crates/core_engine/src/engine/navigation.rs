use protocol::{
    EngineCommand,
    LiveService, ProgramState, SessionItemKind,
};

use super::*;

pub(super) fn navigation_target(
    service: &LiveService,
    program: &ProgramState,
    cmd: &EngineCommand,
) -> (Option<String>, Option<u32>) {
    let items = &service.items;
    let mut idx = program
        .active_item_id
        .as_ref()
        .and_then(|id| items.iter().position(|it| &it.id == id))
        .unwrap_or(0);
    let mut step = program.active_step_index;
    let steps = match items.get(idx).map(|it| &it.kind) {
        Some(SessionItemKind::Text { steps }) => steps.len(),
        Some(SessionItemKind::Presentation { slides, .. }) => slides.len(),
        _ => 0,
    };

    let step_forward = |cur: Option<u32>| match cur {
        _ if steps == 0 => None,
        None => Some(0),
        Some(i) => Some(if (i.saturating_add(1) as usize) < steps {
            i + 1
        } else {
            i
        }),
    };
    let step_back = |cur: Option<u32>| match cur {
        _ if steps == 0 => None,
        cur => cur.map(|i| i.saturating_sub(1)),
    };

    let next = |from: usize| next_cueable(items, from);
    let previous = |from: Option<usize>| from.and_then(|from| previous_cueable(items, from));
    match cmd {
        EngineCommand::Next => {
            let stepped = step_forward(step);
            if stepped != step {
                step = stepped;
            } else if let Some(i) = next(idx + 1) {
                idx = i;
                step = None;
            }
        }
        EngineCommand::Prev => {
            let stepped = step_back(step);
            if stepped != step {
                step = stepped;
            } else if let Some(i) = previous(idx.checked_sub(1)) {
                idx = i;
                step = None;
            }
        }
        EngineCommand::NextItem => {
            if let Some(i) = next(idx + 1) {
                idx = i;
                step = None;
            }
        }
        EngineCommand::PrevItem => {
            if let Some(i) = previous(idx.checked_sub(1)) {
                idx = i;
                step = None;
            }
        }
        EngineCommand::Cue { item_id, index } => {
            if let Some(i) = items.iter().position(|it| it.id == *item_id) {
                if let Some(c) = next(i) { idx = c; step = if c == i { Some(*index) } else { None }; }
            }
        }
        EngineCommand::GoToItem { item_id } => {
            if let Some(i) = items.iter().position(|it| it.id == *item_id) {
                if let Some(c) = next(i) { idx = c; step = None; }
            }
        }
        EngineCommand::NextStep => step = step_forward(step),
        EngineCommand::PrevStep => step = step_back(step),
        EngineCommand::GoToStep { index } => step = Some(*index),
        _ => {}
    }

    (items.get(idx).map(|it| it.id.clone()), step)
}
