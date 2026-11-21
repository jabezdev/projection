use protocol::{
    Background, EngineEvent, ProjectModel, SessionItem, SessionItemKind,
};

use super::*;

impl Engine {
    pub(super) fn select(
        &mut self,
        item_id: Option<i64>,
        step: Option<u32>,
        force_media: bool,
        ports: &mut Ports,
        errors: &mut Vec<EngineEvent>,
    ) {
        let Some(project) = &self.state.project else {
            return;
        };
        let sel = resolve_selection(project, item_id, step);
        let active_item = sel
            .active_item_id
            .and_then(|id| project.session.items.iter().find(|it| it.id == id))
            .cloned();

        let program = &mut self.state.program_state;
        let item_changed = program.active_item_id != sel.active_item_id;
        let step_changed = program.active_step_index != sel.step;
        program.active_item_id = sel.active_item_id;
        program.next_item_id = sel.next_item_id;
        program.active_step_index = sel.step;
        if program.active_text != sel.text || force_media {
            program.active_text = sel.text;
        }

        self.sync_session(active_item.as_ref(), ports);

        let presentation_step = step_changed
            && matches!(active_item.as_ref().map(|it| &it.kind), Some(SessionItemKind::Presentation { .. }));
        if item_changed || force_media || presentation_step {
            if let Some(path) = active_item.as_ref().and_then(|it| it.background.clone()).filter(|p| !p.is_empty()) {
                self.song_background = Some(Background { kind: background_kind(&path), path });
            }
            let background = match active_item.as_ref() {
                Some(item) => self.item_background(item),
                None => None,
            };
            if background != self.state.program_state.background || force_media {
                self.show_background(background, ports, errors);
            }
        }
    }
}

pub(super) struct Selection {
    active_item_id: Option<i64>,
    next_item_id: Option<i64>,
    step: Option<u32>,
    text: String,
}

pub(super) fn resolve_selection(project: &ProjectModel, item_id: Option<i64>, step: Option<u32>) -> Selection {
    let items = &project.session.items;
    if items.is_empty() {
        return Selection {
            active_item_id: None,
            next_item_id: None,
            step: None,
            text: String::new(),
        };
    }

    let requested = item_id.and_then(|id| items.iter().position(|it| it.id == id)).unwrap_or(0);
    let Some(idx) = next_cueable(items, requested).or_else(|| previous_cueable(items, requested)) else {
        return Selection { active_item_id: None, next_item_id: None, step: None, text: String::new() };
    };
    let item = &items[idx];
    let next_item_id = next_cueable(items, idx + 1).map(|i| items[i].id);

    let (step, text) = match &item.kind {
        SessionItemKind::Text { steps } if !steps.is_empty() => {
            let i = step.filter(|&i| (i as usize) < steps.len()).unwrap_or(0);
            (Some(i), steps[i as usize].clone())
        }
        SessionItemKind::Presentation { slides, .. } if !slides.is_empty() => {
            (Some(step.filter(|&i| (i as usize) < slides.len()).unwrap_or(0)), String::new())
        }
        _ => (None, String::new()),
    };

    Selection {
        active_item_id: Some(item.id),
        next_item_id,
        step,
        text,
    }
}

pub(super) fn next_cueable(items: &[SessionItem], from: usize) -> Option<usize> {
    (from..items.len()).find(|&i| items[i].kind.is_cueable())
}

pub(super) fn previous_cueable(items: &[SessionItem], from: usize) -> Option<usize> {
    (0..=from.min(items.len().checked_sub(1)?)).rev().find(|&i| items[i].kind.is_cueable())
}
