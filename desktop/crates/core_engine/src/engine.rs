//! The authoritative engine state machine.
//!
//! `Engine` turns `EngineCommand`s into state changes and `EngineEvent`s. It
//! knows nothing about windows, GPUs or databases: rendering goes through
//! [`Output`] and project storage through [`ProjectStore`], so the whole
//! command surface can be unit-tested with fakes.

use protocol::{
    Background, BackgroundKind, EngineCommand, EngineEvent, EngineSnapshot, ProgramState,
    ProjectModel, SessionItemKind, Transport,
};

/// Where engine decisions become pixels.
pub trait Output {
    fn show_text(&mut self, text: &str);
    fn show_background(&mut self, background: &Background) -> anyhow::Result<()>;
    /// Layers are scene nodes back to front: 0 = background, 1 = text.
    fn set_opacity(&mut self, layer: usize, value: f32) -> anyhow::Result<()>;
    fn resize(&mut self, width: u32, height: u32);
}

pub trait ProjectStore {
    fn load_project(&self, project_id: i64) -> anyhow::Result<ProjectModel>;
    fn save_project(&self, project: &mut ProjectModel) -> anyhow::Result<()>;
}

#[derive(Default)]
pub struct Engine {
    state: EngineSnapshot,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> &EngineSnapshot {
        &self.state
    }

    /// Applies one command. Emits `ProjectChanged`/`ProgramChanged` only when
    /// that domain actually changed (both share one version bump), followed by
    /// any errors.
    pub fn handle(
        &mut self,
        cmd: EngineCommand,
        out: &mut dyn Output,
        store: &dyn ProjectStore,
    ) -> Vec<EngineEvent> {
        let program_before = self.state.program_state.clone();
        let mut errors = Vec::new();
        let project_changed = self.apply(cmd, out, store, &mut errors);
        let program_changed = self.state.program_state != program_before;

        let mut events = Vec::new();
        if project_changed || program_changed {
            self.state.version += 1;
            let version = self.state.version;
            if project_changed {
                events.push(EngineEvent::ProjectChanged {
                    project: self.state.project.clone(),
                    version,
                });
            }
            if program_changed {
                events.push(EngineEvent::ProgramChanged {
                    program_state: self.state.program_state.clone(),
                    version,
                });
            }
        }
        events.extend(errors);
        events
    }

    /// Returns whether the loaded project changed.
    fn apply(
        &mut self,
        cmd: EngineCommand,
        out: &mut dyn Output,
        store: &dyn ProjectStore,
        errors: &mut Vec<EngineEvent>,
    ) -> bool {
        let program = &mut self.state.program_state;
        match cmd {
            EngineCommand::Play => program.transport = Transport::Playing,
            EngineCommand::Pause => program.transport = Transport::Paused,
            EngineCommand::Stop => {
                program.transport = Transport::Stopped;
                program.time_ms = 0;
            }
            EngineCommand::Seek { time_ms } => {
                program.time_ms = time_ms;
                errors.push(error(
                    "seek",
                    "Seek updates time_ms, but video seek is not implemented yet.",
                ));
            }
            EngineCommand::SetActiveText { text } => {
                program.active_step_index = None;
                out.show_text(&text);
                program.active_text = text;
            }
            EngineCommand::SetOpacity { layer, value } => {
                if let Err(e) = out.set_opacity(layer, value.clamp(0.0, 1.0)) {
                    errors.push(error("opacity", format!("{e:#}")));
                }
            }
            EngineCommand::SetBackground { path } => {
                let background = Background {
                    kind: background_kind(&path),
                    path,
                };
                self.show_background(background, out, errors);
            }
            EngineCommand::Resize { width, height } => out.resize(width, height),

            EngineCommand::LoadProject { project_id } => match store.load_project(project_id) {
                Ok(project) => {
                    self.state.project = Some(project);
                    self.select(None, None, true, out, errors);
                    return true;
                }
                Err(e) => errors.push(error(
                    "project",
                    format!("Failed to load project {project_id}: {e:#}"),
                )),
            },
            EngineCommand::SaveProject { mut project } => {
                if let Err(e) = store.save_project(&mut project) {
                    errors.push(error(
                        "project",
                        format!("Failed to save project {}: {e:#}", project.id),
                    ));
                    return false;
                }
                // Saving some other project doesn't touch what's on air.
                if self.state.project.as_ref().map(|p| p.id) != Some(project.id) {
                    return false;
                }
                self.state.project = Some(project);
                // Re-resolve so an edited/removed active item stays consistent.
                let program = &self.state.program_state;
                let (item, step) = (program.active_item_id, program.active_step_index);
                self.select(item, step, false, out, errors);
                return true;
            }

            nav @ (EngineCommand::Next
            | EngineCommand::Prev
            | EngineCommand::NextItem
            | EngineCommand::PrevItem
            | EngineCommand::GoToItem { .. }
            | EngineCommand::NextStep
            | EngineCommand::PrevStep
            | EngineCommand::GoToStep { .. }) => {
                let Some(project) = &self.state.project else {
                    errors.push(error(
                        "navigation",
                        "No project loaded; navigation has no effect.",
                    ));
                    return false;
                };
                let (item, step) = navigation_target(project, &self.state.program_state, &nav);
                self.select(item, step, false, out, errors);
            }
        }
        false
    }

    /// Makes `(item_id, step)` the active selection, resolving it against the
    /// loaded project. Media for the active item is (re)applied when the item
    /// changes, or always with `force_media`.
    fn select(
        &mut self,
        item_id: Option<i64>,
        step: Option<u32>,
        force_media: bool,
        out: &mut dyn Output,
        errors: &mut Vec<EngineEvent>,
    ) {
        let Some(project) = &self.state.project else {
            return;
        };
        let sel = resolve_selection(project, item_id, step);
        let active_kind = sel
            .active_item_id
            .and_then(|id| project.session.items.iter().find(|it| it.id == id))
            .map(|it| it.kind.clone());

        let program = &mut self.state.program_state;
        let item_changed = program.active_item_id != sel.active_item_id;
        program.active_item_id = sel.active_item_id;
        program.next_item_id = sel.next_item_id;
        program.active_step_index = sel.step;
        if program.active_text != sel.text || force_media {
            out.show_text(&sel.text);
            program.active_text = sel.text;
        }

        if item_changed || force_media {
            if let Some(kind) = active_kind {
                self.apply_item_media(&kind, out, errors);
            }
        }
    }

    fn apply_item_media(
        &mut self,
        kind: &SessionItemKind,
        out: &mut dyn Output,
        errors: &mut Vec<EngineEvent>,
    ) {
        let background = match kind {
            SessionItemKind::Text { .. } => return,
            SessionItemKind::Image { path } => Background {
                kind: BackgroundKind::Image,
                path: path.clone(),
            },
            SessionItemKind::Video { path } => Background {
                kind: BackgroundKind::Video,
                path: path.clone(),
            },
            SessionItemKind::Presentation { path } => {
                errors.push(error(
                    "presentation",
                    format!("Presentation items are not implemented yet: {path}"),
                ));
                return;
            }
        };
        self.show_background(background, out, errors);
    }

    /// The program background only changes if the output accepted it.
    fn show_background(
        &mut self,
        background: Background,
        out: &mut dyn Output,
        errors: &mut Vec<EngineEvent>,
    ) {
        match out.show_background(&background) {
            Ok(()) => self.state.program_state.background = Some(background),
            Err(e) => errors.push(error(
                "background",
                format!("Failed to load background {}: {e:#}", background.path),
            )),
        }
    }
}

fn error(source: &str, message: impl Into<String>) -> EngineEvent {
    EngineEvent::Error {
        source: source.to_string(),
        message: message.into(),
        recoverable: true,
    }
}

fn background_kind(path: &str) -> BackgroundKind {
    let lower = path.to_ascii_lowercase();
    if [".mp4", ".mkv", ".mov", ".webm"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        BackgroundKind::Video
    } else {
        BackgroundKind::Image
    }
}

struct Selection {
    active_item_id: Option<i64>,
    next_item_id: Option<i64>,
    step: Option<u32>,
    text: String,
}

/// Resolves a desired selection against the project. Unknown items fall back
/// to the first item; out-of-range steps fall back to step 0.
fn resolve_selection(
    project: &ProjectModel,
    item_id: Option<i64>,
    step: Option<u32>,
) -> Selection {
    let items = &project.session.items;
    if items.is_empty() {
        return Selection {
            active_item_id: None,
            next_item_id: None,
            step: None,
            text: String::new(),
        };
    }

    let idx = item_id
        .and_then(|id| items.iter().position(|it| it.id == id))
        .unwrap_or(0);
    let item = &items[idx];
    let next_item_id = items.get(idx + 1).map(|it| it.id);

    let (step, text) = match &item.kind {
        SessionItemKind::Text { steps } if !steps.is_empty() => {
            let i = step.filter(|&i| (i as usize) < steps.len()).unwrap_or(0);
            (Some(i), steps[i as usize].clone())
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

/// Where a navigation command wants to go, as `(item_id, step)`.
fn navigation_target(
    project: &ProjectModel,
    program: &ProgramState,
    cmd: &EngineCommand,
) -> (Option<i64>, Option<u32>) {
    let items = &project.session.items;
    let mut idx = program
        .active_item_id
        .and_then(|id| items.iter().position(|it| it.id == id))
        .unwrap_or(0);
    let mut step = program.active_step_index;
    let steps = match items.get(idx).map(|it| &it.kind) {
        Some(SessionItemKind::Text { steps }) => steps.len(),
        _ => 0,
    };

    // Within-item moves; `None` when the item has no steps to move through.
    let step_forward = |cur: Option<u32>| match cur {
        _ if steps == 0 => None,
        None => Some(0),
        Some(i) => Some(if (i.saturating_add(1) as usize) < steps { i + 1 } else { i }),
    };
    let step_back = |cur: Option<u32>| match cur {
        _ if steps == 0 => None,
        cur => cur.map(|i| i.saturating_sub(1)),
    };

    match cmd {
        // Smart next/prev: move within the item, else to the neighbouring item.
        EngineCommand::Next => {
            let stepped = step_forward(step);
            if stepped != step {
                step = stepped;
            } else if idx + 1 < items.len() {
                idx += 1;
                step = None;
            }
        }
        EngineCommand::Prev => {
            let stepped = step_back(step);
            if stepped != step {
                step = stepped;
            } else if idx > 0 {
                idx -= 1;
                step = None;
            }
        }
        EngineCommand::NextItem if idx + 1 < items.len() => {
            idx += 1;
            step = None;
        }
        EngineCommand::PrevItem if idx > 0 => {
            idx -= 1;
            step = None;
        }
        EngineCommand::GoToItem { item_id } => {
            if let Some(i) = items.iter().position(|it| it.id == *item_id) {
                idx = i;
                step = None;
            }
        }
        EngineCommand::NextStep => step = step_forward(step),
        EngineCommand::PrevStep => step = step_back(step),
        EngineCommand::GoToStep { index } => step = Some(*index),
        _ => {}
    }

    (items.get(idx).map(|it| it.id), step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{SessionItem, SessionModel};
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeOutput {
        text: String,
        backgrounds: Vec<String>,
        /// Paths that fail to load.
        broken: Vec<String>,
    }

    impl Output for FakeOutput {
        fn show_text(&mut self, text: &str) {
            self.text = text.to_string();
        }
        fn show_background(&mut self, background: &Background) -> anyhow::Result<()> {
            if self.broken.contains(&background.path) {
                anyhow::bail!("no such file");
            }
            self.backgrounds.push(background.path.clone());
            Ok(())
        }
        fn set_opacity(&mut self, layer: usize, _value: f32) -> anyhow::Result<()> {
            anyhow::ensure!(layer < 2, "unknown layer {layer}");
            Ok(())
        }
        fn resize(&mut self, _width: u32, _height: u32) {}
    }

    #[derive(Default)]
    struct FakeStore {
        projects: RefCell<Vec<ProjectModel>>,
    }

    impl ProjectStore for FakeStore {
        fn load_project(&self, project_id: i64) -> anyhow::Result<ProjectModel> {
            self.projects
                .borrow()
                .iter()
                .find(|p| p.id == project_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("project {project_id} not found"))
        }
        fn save_project(&self, project: &mut ProjectModel) -> anyhow::Result<()> {
            let mut projects = self.projects.borrow_mut();
            match projects.iter_mut().find(|p| p.id == project.id) {
                Some(p) => *p = project.clone(),
                None => anyhow::bail!("project {} not found", project.id),
            }
            Ok(())
        }
    }

    fn text_item(id: i64, steps: &[&str]) -> SessionItem {
        SessionItem {
            id,
            title: None,
            kind: SessionItemKind::Text {
                steps: steps.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    fn image_item(id: i64, path: &str) -> SessionItem {
        SessionItem {
            id,
            title: None,
            kind: SessionItemKind::Image {
                path: path.to_string(),
            },
        }
    }

    /// Text(a, b) → Image → Text(c)
    fn setup() -> (Engine, FakeOutput, FakeStore) {
        let project = ProjectModel {
            id: 1,
            name: "Service".into(),
            session: SessionModel {
                items: vec![
                    text_item(10, &["a", "b"]),
                    image_item(20, "bg.png"),
                    text_item(30, &["c"]),
                ],
            },
        };
        let store = FakeStore::default();
        store.projects.borrow_mut().push(project);
        (Engine::new(), FakeOutput::default(), store)
    }

    fn load(engine: &mut Engine, out: &mut FakeOutput, store: &FakeStore) {
        engine.handle(EngineCommand::LoadProject { project_id: 1 }, out, store);
    }

    fn selection(engine: &Engine) -> (Option<i64>, Option<u32>, &str) {
        let p = &engine.snapshot().program_state;
        (p.active_item_id, p.active_step_index, p.active_text.as_str())
    }

    #[test]
    fn load_project_emits_both_domains_with_one_version() {
        let (mut engine, mut out, store) = setup();
        let events = engine.handle(EngineCommand::LoadProject { project_id: 1 }, &mut out, &store);

        assert!(matches!(
            events.as_slice(),
            [
                EngineEvent::ProjectChanged { version: 1, .. },
                EngineEvent::ProgramChanged { version: 1, .. },
            ]
        ));
        assert_eq!(selection(&engine), (Some(10), Some(0), "a"));
        assert_eq!(engine.snapshot().program_state.next_item_id, Some(20));
        assert_eq!(out.text, "a");
    }

    #[test]
    fn next_walks_steps_then_items_and_applies_media() {
        let (mut engine, mut out, store) = setup();
        load(&mut engine, &mut out, &store);

        engine.handle(EngineCommand::Next, &mut out, &store);
        assert_eq!(selection(&engine), (Some(10), Some(1), "b"));

        engine.handle(EngineCommand::Next, &mut out, &store);
        assert_eq!(selection(&engine), (Some(20), None, ""));
        assert_eq!(out.backgrounds, ["bg.png"]);
        assert_eq!(
            engine.snapshot().program_state.background.as_ref().map(|b| b.path.as_str()),
            Some("bg.png")
        );

        engine.handle(EngineCommand::Next, &mut out, &store);
        assert_eq!(selection(&engine), (Some(30), Some(0), "c"));

        // At the very end, Next is a no-op and emits nothing.
        let events = engine.handle(EngineCommand::Next, &mut out, &store);
        assert!(events.is_empty());
    }

    #[test]
    fn prev_at_first_step_goes_to_previous_item() {
        let (mut engine, mut out, store) = setup();
        load(&mut engine, &mut out, &store);
        engine.handle(EngineCommand::GoToItem { item_id: 30 }, &mut out, &store);

        engine.handle(EngineCommand::Prev, &mut out, &store);
        assert_eq!(selection(&engine), (Some(20), None, ""));
    }

    #[test]
    fn navigation_without_project_is_an_error_without_version_bump() {
        let (mut engine, mut out, store) = setup();
        let events = engine.handle(EngineCommand::Next, &mut out, &store);

        assert!(matches!(
            events.as_slice(),
            [EngineEvent::Error { source, .. }] if source == "navigation"
        ));
        assert_eq!(engine.snapshot().version, 0);
    }

    #[test]
    fn unchanged_state_emits_nothing() {
        let (mut engine, mut out, store) = setup();
        assert_eq!(engine.handle(EngineCommand::Play, &mut out, &store).len(), 1);
        assert!(engine.handle(EngineCommand::Play, &mut out, &store).is_empty());
        assert_eq!(engine.snapshot().version, 1);
    }

    #[test]
    fn failed_background_keeps_previous_background() {
        let (mut engine, mut out, store) = setup();
        out.broken.push("missing.png".into());
        engine.handle(
            EngineCommand::SetBackground { path: "ok.png".into() },
            &mut out,
            &store,
        );

        let events = engine.handle(
            EngineCommand::SetBackground { path: "missing.png".into() },
            &mut out,
            &store,
        );
        assert!(matches!(events.as_slice(), [EngineEvent::Error { .. }]));
        assert_eq!(
            engine.snapshot().program_state.background.as_ref().map(|b| b.path.as_str()),
            Some("ok.png")
        );
    }

    #[test]
    fn background_kind_is_inferred_from_extension() {
        assert_eq!(background_kind("clip.MP4"), BackgroundKind::Video);
        assert_eq!(background_kind("still.png"), BackgroundKind::Image);
    }

    #[test]
    fn saving_another_project_leaves_loaded_project_alone() {
        let (mut engine, mut out, store) = setup();
        store.projects.borrow_mut().push(ProjectModel::empty(2, "Other"));
        load(&mut engine, &mut out, &store);

        let events = engine.handle(
            EngineCommand::SaveProject {
                project: ProjectModel::empty(2, "Other renamed"),
            },
            &mut out,
            &store,
        );
        assert!(events.is_empty());
        assert_eq!(engine.snapshot().project.as_ref().map(|p| p.id), Some(1));
    }

    #[test]
    fn saving_loaded_project_reresolves_selection() {
        let (mut engine, mut out, store) = setup();
        load(&mut engine, &mut out, &store);

        let mut edited = engine.snapshot().project.clone().unwrap();
        edited.session.items[0] = text_item(10, &["a (edited)", "b"]);
        let events = engine.handle(EngineCommand::SaveProject { project: edited }, &mut out, &store);

        assert_eq!(events.len(), 2);
        assert_eq!(selection(&engine), (Some(10), Some(0), "a (edited)"));
        assert_eq!(out.text, "a (edited)");
    }
}
