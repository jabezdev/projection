use protocol::{
    Background, EngineEvent, LiveService, SessionItem, SessionItemKind,
};

use super::*;

impl Engine {
    pub(super) fn select(
        &mut self,
        item_id: Option<String>,
        step: Option<u32>,
        force_media: bool,
        ports: &mut Ports,
        errors: &mut Vec<EngineEvent>,
    ) {
        let Some(service) = &self.state.service else {
            return;
        };
        let sel = resolve_selection(service, item_id.as_deref(), step);
        let active_item = sel.active_item_id.as_deref().and_then(|id| service.item(id)).cloned();

        let program = &mut self.state.program_state;
        let item_changed = program.active_item_id != sel.active_item_id;
        let step_changed = program.active_step_index != sel.step;
        program.active_item_id = sel.active_item_id;
        program.next_item_id = sel.next_item_id;
        program.active_step_index = sel.step;
        if program.active_text != sel.text || force_media {
            program.active_text = sel.text;
        }
        program.active_parallel = sel.parallel;
        program.active_caption = sel.caption;

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
    active_item_id: Option<String>,
    next_item_id: Option<String>,
    step: Option<u32>,
    text: String,
    parallel: String,
    caption: String,
}

pub(super) fn resolve_selection(service: &LiveService, item_id: Option<&str>, step: Option<u32>) -> Selection {
    let items = &service.items;
    if items.is_empty() {
        return Selection::empty();
    }

    let requested = item_id.and_then(|id| items.iter().position(|it| it.id == id)).unwrap_or(0);
    let Some(idx) = next_cueable(items, requested).or_else(|| previous_cueable(items, requested)) else {
        return Selection::empty();
    };
    let item = &items[idx];
    let next_item_id = next_cueable(items, idx + 1).map(|i| items[i].id.clone());

    let at = |list: &[String], i: u32| list.get(i as usize).cloned().unwrap_or_default();
    let (step, text, parallel, caption) = match &item.kind {
        SessionItemKind::Text { steps, parallel, captions } if !steps.is_empty() => {
            let i = step.filter(|&i| (i as usize) < steps.len()).unwrap_or(0);
            (Some(i), at(steps, i), at(parallel, i), at(captions, i))
        }
        SessionItemKind::Presentation { slides, .. } if !slides.is_empty() => {
            (Some(step.filter(|&i| (i as usize) < slides.len()).unwrap_or(0)), String::new(), String::new(), String::new())
        }
        _ => (None, String::new(), String::new(), String::new()),
    };

    Selection { active_item_id: Some(item.id.clone()), next_item_id, step, text, parallel, caption }
}

impl Selection {
    fn empty() -> Self {
        Selection { active_item_id: None, next_item_id: None, step: None, text: String::new(), parallel: String::new(), caption: String::new() }
    }
}

pub(super) fn next_cueable(items: &[SessionItem], from: usize) -> Option<usize> {
    (from..items.len()).find(|&i| items[i].kind.is_cueable())
}

pub(super) fn previous_cueable(items: &[SessionItem], from: usize) -> Option<usize> {
    (0..=from.min(items.len().checked_sub(1)?)).rev().find(|&i| items[i].kind.is_cueable())
}
