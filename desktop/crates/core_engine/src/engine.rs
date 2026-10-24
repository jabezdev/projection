//! The authoritative engine state machine.
//!
//! `Engine` turns `EngineCommand`s and media backend reports into state changes
//! and `EngineEvent`s. It knows nothing about windows, GPUs, decoders or
//! databases: background files are checked through [`Assets`], playback goes
//! through [`MediaBackend`] and project storage through [`ProjectStore`], so
//! the whole command surface can be unit-tested with fakes. Outputs draw from
//! the published state.

use protocol::{
    Background, BackgroundKind, DisplayMode, EngineCommand, EngineEvent, EngineSnapshot,
    MediaAction, MediaDirective, PlaybackKind, PlaybackPhase, PlaybackReport, PlaybackState,
    ProgramState, ProjectModel, SessionId, SessionItem, SessionItemKind, VideoOptions,
};

/// Checks a background before the program switches to it, so a missing file
/// is reported and the previous background stays on air.
pub trait Assets {
    fn check_background(&self, background: &Background) -> anyhow::Result<()>;
}

/// Whatever plays foreground media. Failures come back as `PlaybackReport`s.
pub trait MediaBackend {
    fn send(&mut self, directive: MediaDirective);
}

pub trait ProjectStore {
    fn load_project(&self, project_id: i64) -> anyhow::Result<ProjectModel>;
    fn save_project(&self, project: &mut ProjectModel) -> anyhow::Result<()>;
}

/// The engine's adapters for one call.
pub struct Ports<'a> {
    pub assets: &'a dyn Assets,
    pub media: &'a mut dyn MediaBackend,
    pub store: &'a dyn ProjectStore,
    /// Wall-clock time used to stamp playback positions.
    pub now_ms: i64,
}

/// Everything the engine consumes, in arrival order.
#[derive(Debug, Clone)]
pub enum EngineInput {
    Command(EngineCommand),
    Report(PlaybackReport),
}

pub struct Engine {
    state: EngineSnapshot,
    /// The last intentional song background; a foreground image or video
    /// never becomes one.
    song_background: Option<Background>,
    next_session_id: SessionId,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self::with_session_seed(1)
    }

    /// Session ids start at `seed`; seeding from the clock keeps ids from a
    /// previous run from matching new sessions.
    pub fn with_session_seed(seed: SessionId) -> Self {
        Self { state: EngineSnapshot::default(), song_background: None, next_session_id: seed }
    }

    pub fn snapshot(&self) -> &EngineSnapshot {
        &self.state
    }

    pub fn handle_input(&mut self, input: EngineInput, ports: &mut Ports) -> Vec<EngineEvent> {
        match input {
            EngineInput::Command(cmd) => self.handle(cmd, ports),
            EngineInput::Report(report) => self.handle_report(report, ports),
        }
    }

    /// Applies one command. Emits `ProjectChanged`/`ProgramChanged` only when
    /// that domain actually changed (both share one version bump),
    /// `PlaybackChanged` when transport changed, then any errors.
    pub fn handle(&mut self, cmd: EngineCommand, ports: &mut Ports) -> Vec<EngineEvent> {
        self.observe(ports, |engine, ports, errors| engine.apply(cmd.normalize(), ports, errors))
    }

    /// Applies a backend report. Reports for an earlier session or an earlier
    /// seek generation are stale and change nothing.
    pub fn handle_report(&mut self, report: PlaybackReport, ports: &mut Ports) -> Vec<EngineEvent> {
        self.observe(ports, |engine, ports, _| {
            let Some(playback) = engine.state.playback.as_mut() else { return false };
            if report.session_id != playback.session_id || report.generation != playback.generation {
                return false;
            }
            playback.phase = report.phase;
            playback.position_ms = report.position_ms;
            playback.position_at_ms = ports.now_ms;
            playback.duration_ms = report.duration_ms.or(playback.duration_ms);
            playback.error = report.error;
            if report.phase == PlaybackPhase::Ended && !playback.looping {
                playback.paused = true;
            }
            false
        })
    }

    fn observe(
        &mut self,
        ports: &mut Ports,
        change: impl FnOnce(&mut Self, &mut Ports, &mut Vec<EngineEvent>) -> bool,
    ) -> Vec<EngineEvent> {
        let program_before = self.state.program_state.clone();
        let playback_before = self.state.playback.clone();
        let mut errors = Vec::new();
        let project_changed = change(self, ports, &mut errors);
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
        if self.state.playback != playback_before {
            self.state.playback_revision += 1;
            events.push(EngineEvent::PlaybackChanged {
                playback: self.state.playback.clone(),
                revision: self.state.playback_revision,
            });
        }
        events.extend(errors);
        events
    }

    /// Returns whether the loaded project changed.
    fn apply(&mut self, cmd: EngineCommand, ports: &mut Ports, errors: &mut Vec<EngineEvent>) -> bool {
        let program = &mut self.state.program_state;
        match cmd {
            EngineCommand::SetDisplay { display } => {
                let was_logo = program.display == DisplayMode::Logo;
                program.display = display;
                // Logo suspends foreground video and resumes it on the way out;
                // Black and Clear leave transport running. Standalone audio has
                // no picture to hide, so it keeps playing under the Logo.
                if let Some(playback) = self.state.playback.as_mut().filter(|p| p.kind == PlaybackKind::Video) {
                    let is_logo = display == DisplayMode::Logo;
                    if is_logo != was_logo {
                        let was_running = playback.running();
                        playback.suspended = is_logo;
                        if was_running != playback.running() {
                            ports.media.send(transport_directive(playback));
                        }
                    }
                }
            }
            EngineCommand::Media { session_id, action } => self.media(session_id, action, ports, errors),
            EngineCommand::Play | EngineCommand::Pause | EngineCommand::Stop | EngineCommand::Seek { .. } => {
                unreachable!("normalized to EngineCommand::Media")
            }
            EngineCommand::SetActiveText { text } => {
                program.active_step_index = None;
                program.active_text = text;
            }
            EngineCommand::SetBackground { path } => {
                let background = Background { kind: background_kind(&path), path };
                self.show_background(Some(background), ports, errors);
            }

            EngineCommand::LoadProject { project_id } => match ports.store.load_project(project_id) {
                Ok(project) => {
                    self.state.project = Some(project);
                    self.song_background = None;
                    self.select(None, None, true, ports, errors);
                    return true;
                }
                Err(e) => errors.push(error(
                    "project",
                    format!("Failed to load project {project_id}: {e:#}"),
                )),
            },
            EngineCommand::SaveProject { mut project } => {
                if let Err(e) = ports.store.save_project(&mut project) {
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
                let active_id = self.state.program_state.active_item_id;
                let find = |p: &ProjectModel| p.session.items.iter().find(|item| Some(item.id) == active_id).cloned();
                let old_item = self.state.project.as_ref().and_then(find);
                let new_item = find(&project);
                let media_changed = old_item.as_ref().map(|i| (&i.background, &i.kind))
                    != new_item.as_ref().map(|i| (&i.background, &i.kind));
                self.state.project = Some(project);
                // Re-resolve so an edited/removed active item stays consistent.
                let program = &self.state.program_state;
                let (item, step) = (program.active_item_id, program.active_step_index);
                self.select(item, step, media_changed, ports, errors);
                return true;
            }

            nav @ (EngineCommand::Next
            | EngineCommand::Prev
            | EngineCommand::NextItem
            | EngineCommand::PrevItem
            | EngineCommand::Cue { .. }
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
                self.select(item, step, false, ports, errors);
            }
        }
        false
    }

    fn media(
        &mut self,
        session_id: Option<SessionId>,
        action: MediaAction,
        ports: &mut Ports,
        errors: &mut Vec<EngineEvent>,
    ) {
        let Some(playback) = self.state.playback.as_mut() else {
            errors.push(error("media", "No media is cued."));
            return;
        };
        if let Some(id) = session_id.filter(|&id| id != playback.session_id) {
            errors.push(error("media", format!("Ignored a command for an earlier media session ({id}).")));
            return;
        }
        let session_id = playback.session_id;
        match action {
            MediaAction::Play if playback.paused => {
                if playback.phase == PlaybackPhase::Ended {
                    let start = playback.start_ms;
                    seek(playback, start, ports);
                }
                playback.paused = false;
                if playback.running() {
                    ports.media.send(MediaDirective::Play { session_id });
                }
            }
            MediaAction::Pause if !playback.paused => {
                let was_running = playback.running();
                playback.paused = true;
                if was_running {
                    ports.media.send(MediaDirective::Pause { session_id });
                }
            }
            MediaAction::Play | MediaAction::Pause => {}
            MediaAction::Stop => {
                if playback.running() {
                    ports.media.send(MediaDirective::Pause { session_id });
                }
                playback.paused = true;
                let start = playback.start_ms;
                seek(playback, start, ports);
            }
            MediaAction::Seek { position_ms } => {
                let end = match (playback.end_ms, playback.duration_ms) {
                    (Some(end), Some(duration)) => Some(end.min(duration)),
                    (end, duration) => end.or(duration),
                };
                let position = end
                    .map(|end| position_ms.min(end.saturating_sub(1)))
                    .unwrap_or(position_ms)
                    .max(playback.start_ms);
                seek(playback, position, ports);
            }
            MediaAction::SetMuted { muted } => {
                if playback.muted != muted {
                    playback.muted = muted;
                    ports.media.send(MediaDirective::Configure { playback: playback.clone() });
                }
            }
            MediaAction::SetVolume { volume } => {
                let volume = volume.min(100);
                if playback.volume != volume {
                    playback.volume = volume;
                    ports.media.send(MediaDirective::Configure { playback: playback.clone() });
                }
            }
        }
    }

    /// Makes `(item_id, step)` the active selection, resolving it against the
    /// loaded project. Media for the active item is (re)applied when the item
    /// changes, or always with `force_media`.
    fn select(
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

        // Foreground media sessions follow the active item, before visuals change.
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

    /// What fills the screen behind the text for `item`. Non-visual items show
    /// the intentional song background, never a previous foreground video.
    fn item_background(&self, item: &SessionItem) -> Option<Background> {
        match &item.kind {
            // A capture is drawn over the song background by the scene feed,
            // on the screens it is routed to.
            SessionItemKind::Text { .. } | SessionItemKind::Audio { .. } | SessionItemKind::Capture { .. } => self.song_background.clone(),
            SessionItemKind::Image { path } => Some(Background { kind: BackgroundKind::Image, path: path.clone() }),
            SessionItemKind::Video { path, .. } => Some(Background { kind: BackgroundKind::Video, path: path.clone() }),
            SessionItemKind::Presentation { path, slides } => {
                let index = self.state.program_state.active_step_index.unwrap_or(0) as usize;
                Some(Background { kind: BackgroundKind::Image, path: slides.get(index).unwrap_or(path).clone() })
            }
        }
    }

    /// Keeps exactly one foreground session for the active media item. Edits to
    /// the active item's options reconfigure the session without restarting it.
    fn sync_session(&mut self, item: Option<&SessionItem>, ports: &mut Ports) {
        let target = item.and_then(|item| match &item.kind {
            SessionItemKind::Video { path, options } => Some((item.id, PlaybackKind::Video, path, options)),
            SessionItemKind::Audio { path, options } => Some((item.id, PlaybackKind::Audio, path, options)),
            _ => None,
        });
        if let (Some(playback), Some((item_id, kind, path, options))) = (self.state.playback.as_mut(), target) {
            if playback.item_id == item_id && playback.kind == kind && &playback.path == path {
                let before = playback.clone();
                apply_options(playback, options);
                if *playback != before {
                    ports.media.send(MediaDirective::Configure { playback: playback.clone() });
                }
                return;
            }
        }
        if let Some(old) = self.state.playback.take() {
            ports.media.send(MediaDirective::Close { session_id: old.session_id });
        }
        if let Some((item_id, kind, path, options)) = target {
            let session_id = self.next_session_id;
            self.next_session_id += 1;
            let mut playback = PlaybackState {
                session_id,
                generation: 0,
                item_id,
                path: path.clone(),
                kind,
                paused: false,
                suspended: kind == PlaybackKind::Video && self.state.program_state.display == DisplayMode::Logo,
                phase: PlaybackPhase::Preparing,
                position_ms: options.start_ms,
                duration_ms: None,
                position_at_ms: ports.now_ms,
                volume: 100,
                muted: false,
                rate_percent: 100,
                looping: false,
                start_ms: 0,
                end_ms: None,
                confidence: false,
                error: None,
            };
            apply_options(&mut playback, options);
            ports.media.send(MediaDirective::Open { playback: playback.clone() });
            self.state.playback = Some(playback);
        }
    }

    /// The program background only changes if its file checks out.
    fn show_background(
        &mut self,
        background: Option<Background>,
        ports: &mut Ports,
        errors: &mut Vec<EngineEvent>,
    ) {
        let Some(background) = background else {
            self.state.program_state.background = None;
            return;
        };
        match ports.assets.check_background(&background) {
            Ok(()) => self.state.program_state.background = Some(background),
            Err(e) => errors.push(error(
                "background",
                format!("Failed to load background {}: {e:#}", background.path),
            )),
        }
    }
}

/// Starts a new seek generation at `position_ms`.
fn seek(playback: &mut PlaybackState, position_ms: u64, ports: &mut Ports) {
    playback.generation += 1;
    playback.position_ms = position_ms;
    playback.position_at_ms = ports.now_ms;
    if playback.phase == PlaybackPhase::Ended {
        playback.phase = PlaybackPhase::Paused;
    }
    ports.media.send(MediaDirective::Seek {
        session_id: playback.session_id,
        generation: playback.generation,
        position_ms,
    });
}

fn transport_directive(playback: &PlaybackState) -> MediaDirective {
    let session_id = playback.session_id;
    if playback.running() {
        MediaDirective::Play { session_id }
    } else {
        MediaDirective::Pause { session_id }
    }
}

/// Copies saved clip/volume/rate/loop options onto a session.
fn apply_options(playback: &mut PlaybackState, options: &VideoOptions) {
    playback.volume = options.volume.min(100);
    playback.rate_percent = options.rate_percent.clamp(50, 200);
    playback.looping = options.looping;
    playback.confidence = options.confidence;
    playback.start_ms = options.start_ms;
    // Empty or reversed clips play to the end.
    playback.end_ms = options.end_ms.filter(|&end| end > options.start_ms);
}

/// Merges adjacent inputs where only the latest matters (seek scrubbing,
/// volume drags, position reports, repeated saves of one project), keeping
/// every other input in order.
pub fn coalesce(inputs: Vec<EngineInput>) -> Vec<EngineInput> {
    let mut merged: Vec<EngineInput> = Vec::with_capacity(inputs.len());
    for input in inputs {
        // Shorthands such as `Seek` merge with their addressed equivalents.
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
    use EngineCommand::{Media, SaveProject};
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
        (Command(SaveProject { project: a }), Command(SaveProject { project: b })) => a.id == b.id,
        _ => false,
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
    if path.starts_with(protocol::SHADER_PREFIX) {
        return BackgroundKind::Shader;
    }
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
fn resolve_selection(project: &ProjectModel, item_id: Option<i64>, step: Option<u32>) -> Selection {
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
        Some(SessionItemKind::Presentation { slides, .. }) => slides.len(),
        _ => 0,
    };

    // Within-item moves; `None` when the item has no steps to move through.
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
        EngineCommand::Cue { item_id, index } => {
            if let Some(i) = items.iter().position(|it| it.id == *item_id) { idx = i; step = Some(*index); }
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
    use protocol::SessionModel;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeAssets {
        /// Backgrounds accepted, in order.
        backgrounds: RefCell<Vec<String>>,
        /// Paths that fail to load.
        broken: Vec<String>,
    }

    impl Assets for FakeAssets {
        fn check_background(&self, background: &Background) -> anyhow::Result<()> {
            if self.broken.contains(&background.path) {
                anyhow::bail!("no such file");
            }
            self.backgrounds.borrow_mut().push(background.path.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeMedia {
        directives: Vec<MediaDirective>,
    }

    impl MediaBackend for FakeMedia {
        fn send(&mut self, directive: MediaDirective) {
            self.directives.push(directive);
        }
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
            song: None,
            background: None,
            id,
            title: None,
            kind: SessionItemKind::Text {
                steps: steps.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    fn image_item(id: i64, path: &str) -> SessionItem {
        SessionItem {
            song: None,
            background: None,
            id,
            title: None,
            kind: SessionItemKind::Image {
                path: path.to_string(),
            },
        }
    }

    struct Harness {
        engine: Engine,
        assets: FakeAssets,
        media: FakeMedia,
        store: FakeStore,
        now_ms: i64,
    }

    impl Harness {
        fn ports(&mut self) -> (&mut Engine, Ports<'_>) {
            let ports = Ports { assets: &self.assets, media: &mut self.media, store: &self.store, now_ms: self.now_ms };
            (&mut self.engine, ports)
        }
        fn send(&mut self, cmd: EngineCommand) -> Vec<EngineEvent> {
            let (engine, mut ports) = self.ports();
            engine.handle(cmd, &mut ports)
        }
        fn report(&mut self, report: PlaybackReport) -> Vec<EngineEvent> {
            let (engine, mut ports) = self.ports();
            engine.handle_report(report, &mut ports)
        }
        fn load(&mut self) {
            self.send(EngineCommand::LoadProject { project_id: 1 });
        }
        fn playback(&self) -> &PlaybackState {
            self.engine.snapshot().playback.as_ref().expect("a cued media session")
        }
        /// Directives sent since the last call.
        fn take_directives(&mut self) -> Vec<MediaDirective> {
            std::mem::take(&mut self.media.directives)
        }
    }

    /// Text(a, b) → Image → Text(c)
    fn setup() -> Harness {
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
        Harness { engine: Engine::new(), assets: FakeAssets::default(), media: FakeMedia::default(), store, now_ms: 1_000 }
    }

    fn selection(engine: &Engine) -> (Option<i64>, Option<u32>, &str) {
        let p = &engine.snapshot().program_state;
        (
            p.active_item_id,
            p.active_step_index,
            p.active_text.as_str(),
        )
    }

    #[test]
    fn cue_selects_item_and_slide_in_one_event_without_showing_first_slide() {
        let mut h = setup();
        h.load();
        h.send(EngineCommand::GoToItem { item_id: 30 });
        let events = h.send(EngineCommand::Cue { item_id: 10, index: 1 });
        assert_eq!(selection(&h.engine), (Some(10), Some(1), "b"));
        assert_eq!(h.engine.snapshot().program_state.active_text, "b");
        assert_eq!(events.len(), 1);
        match &events[0] {
            EngineEvent::ProgramChanged { program_state, .. } => {
                assert_eq!(program_state.active_text, "b");
                assert_eq!(program_state.active_step_index, Some(1));
            }
            event => panic!("Expected one final cue, got {event:?}"),
        }
    }

    #[test]
    fn song_background_changes_on_selection_and_reuses_between_slides() {
        let mut h = setup();
        h.store.projects.borrow_mut()[0].session.items[0].background = Some("song.jpg".into());
        h.load();
        assert_eq!(*h.assets.backgrounds.borrow(), vec!["song.jpg"]);
        h.send(EngineCommand::GoToStep { index: 1 });
        assert_eq!(h.assets.backgrounds.borrow().len(), 1);
        h.send(EngineCommand::GoToItem { item_id: 30 });
        assert_eq!(
            h.engine
                .snapshot()
                .program_state
                .background
                .as_ref()
                .unwrap()
                .path,
            "song.jpg"
        );
        h.send(EngineCommand::GoToItem { item_id: 20 });
        assert_eq!(*h.assets.backgrounds.borrow(), vec!["song.jpg", "bg.png"]);
    }

    #[test]
    fn editing_active_song_background_applies_without_reselecting() {
        let mut h = setup();
        h.load();
        let mut project = h.engine.snapshot().project.clone().unwrap();
        project.session.items[0].background = Some("edited.jpg".into());
        h.send(EngineCommand::SaveProject { project });
        assert_eq!(*h.assets.backgrounds.borrow(), vec!["edited.jpg"]);
        assert_eq!(selection(&h.engine), (Some(10), Some(0), "a"));
    }

    #[test]
    fn display_override_keeps_selection_and_lyrics_for_return_to_live() {
        let mut h = setup();
        h.load();
        for display in [
            protocol::DisplayMode::Black,
            protocol::DisplayMode::Clear,
            protocol::DisplayMode::Logo,
            protocol::DisplayMode::Live,
        ] {
            let events = h.send(EngineCommand::SetDisplay { display });
            assert_eq!(h.engine.snapshot().program_state.display, display);
            assert_eq!(selection(&h.engine), (Some(10), Some(0), "a"));
            assert!(matches!(
                events.as_slice(),
                [EngineEvent::ProgramChanged { .. }]
            ));
        }
    }

    #[test]
    fn load_project_emits_both_domains_with_one_version() {
        let mut h = setup();
        let events = h.send(EngineCommand::LoadProject { project_id: 1 });

        assert!(matches!(
            events.as_slice(),
            [
                EngineEvent::ProjectChanged { version: 1, .. },
                EngineEvent::ProgramChanged { version: 1, .. },
            ]
        ));
        assert_eq!(selection(&h.engine), (Some(10), Some(0), "a"));
        assert_eq!(h.engine.snapshot().program_state.next_item_id, Some(20));
        assert_eq!(h.engine.snapshot().program_state.active_text, "a");
    }

    #[test]
    fn next_walks_steps_then_items_and_applies_media() {
        let mut h = setup();
        h.load();

        h.send(EngineCommand::Next);
        assert_eq!(selection(&h.engine), (Some(10), Some(1), "b"));

        h.send(EngineCommand::Next);
        assert_eq!(selection(&h.engine), (Some(20), None, ""));
        assert_eq!(*h.assets.backgrounds.borrow(), ["bg.png"]);
        assert_eq!(
            h.engine
                .snapshot()
                .program_state
                .background
                .as_ref()
                .map(|b| b.path.as_str()),
            Some("bg.png")
        );

        h.send(EngineCommand::Next);
        assert_eq!(selection(&h.engine), (Some(30), Some(0), "c"));

        // At the very end, Next is a no-op and emits nothing.
        let events = h.send(EngineCommand::Next);
        assert!(events.is_empty());
    }

    #[test]
    fn prev_at_first_step_goes_to_previous_item() {
        let mut h = setup();
        h.load();
        h.send(EngineCommand::GoToItem { item_id: 30 });

        h.send(EngineCommand::Prev);
        assert_eq!(selection(&h.engine), (Some(20), None, ""));
    }

    #[test]
    fn navigation_without_project_is_an_error_without_version_bump() {
        let mut h = setup();
        let events = h.send(EngineCommand::Next);

        assert!(matches!(
            events.as_slice(),
            [EngineEvent::Error { source, .. }] if source == "navigation"
        ));
        assert_eq!(h.engine.snapshot().version, 0);
    }

    #[test]
    fn unchanged_state_emits_nothing() {
        let mut h = setup();
        let black = || EngineCommand::SetDisplay { display: DisplayMode::Black };
        assert_eq!(h.send(black()).len(), 1);
        assert!(h.send(black()).is_empty());
        assert_eq!(h.engine.snapshot().version, 1);
    }

    #[test]
    fn failed_background_keeps_previous_background() {
        let mut h = setup();
        h.assets.broken.push("missing.png".into());
        h.send(EngineCommand::SetBackground {
                path: "ok.png".into(),
            });

        let events = h.send(EngineCommand::SetBackground {
                path: "missing.png".into(),
            });
        assert!(matches!(events.as_slice(), [EngineEvent::Error { .. }]));
        assert_eq!(
            h.engine
                .snapshot()
                .program_state
                .background
                .as_ref()
                .map(|b| b.path.as_str()),
            Some("ok.png")
        );
    }

    #[test]
    fn background_kind_is_inferred_from_extension() {
        assert_eq!(background_kind("clip.MP4"), BackgroundKind::Video);
        assert_eq!(background_kind("still.png"), BackgroundKind::Image);
        assert_eq!(background_kind("shader:aurora"), BackgroundKind::Shader);
    }

    #[test]
    fn saving_another_project_leaves_loaded_project_alone() {
        let mut h = setup();
        h.store
            .projects
            .borrow_mut()
            .push(ProjectModel::empty(2, "Other"));
        h.load();

        let events = h.send(EngineCommand::SaveProject {
                project: ProjectModel::empty(2, "Other renamed"),
            });
        assert!(events.is_empty());
        assert_eq!(h.engine.snapshot().project.as_ref().map(|p| p.id), Some(1));
    }

    #[test]
    fn saving_loaded_project_reresolves_selection() {
        let mut h = setup();
        h.load();

        let mut edited = h.engine.snapshot().project.clone().unwrap();
        edited.session.items[0] = text_item(10, &["a (edited)", "b"]);
        let events = h.send(EngineCommand::SaveProject { project: edited });

        assert_eq!(events.len(), 2);
        assert_eq!(selection(&h.engine), (Some(10), Some(0), "a (edited)"));
        assert_eq!(h.engine.snapshot().program_state.active_text, "a (edited)");
    }
    #[test]
    fn presentation_steps_cue_images_and_follow_navigation() {
        let mut h = setup();
        h.store.projects.borrow_mut()[0].session.items[1].kind = SessionItemKind::Presentation { path: "first.png".into(), slides: vec!["first.png".into(), "second.png".into(), "third.png".into()] };
        h.load();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        assert_eq!(selection(&h.engine), (Some(20), Some(0), ""));
        assert_eq!(h.assets.backgrounds.borrow().last().unwrap(), "first.png");
        h.send(EngineCommand::Next);
        assert_eq!(selection(&h.engine), (Some(20), Some(1), ""));
        assert_eq!(h.assets.backgrounds.borrow().last().unwrap(), "second.png");
        h.send(EngineCommand::GoToStep { index: 2 });
        assert_eq!(h.assets.backgrounds.borrow().last().unwrap(), "third.png");
        h.send(EngineCommand::Next);
        assert_eq!(selection(&h.engine), (Some(30), Some(0), "c"));
    }

    #[test]
    fn editing_active_presentation_refreshes_the_selected_image() {
        let mut h = setup();
        h.store.projects.borrow_mut()[0].session.items[1].kind = SessionItemKind::Presentation { path: "first.png".into(), slides: vec!["first.png".into(), "second.png".into()] };
        h.load();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        h.send(EngineCommand::GoToStep { index: 1 });
        let mut project = h.engine.snapshot().project.clone().unwrap();
        project.session.items[1].kind = SessionItemKind::Presentation { path: "first.png".into(), slides: vec!["first.png".into(), "edited.png".into()] };
        h.send(EngineCommand::SaveProject { project });
        assert_eq!(selection(&h.engine), (Some(20), Some(1), ""));
        assert_eq!(h.assets.backgrounds.borrow().last().unwrap(), "edited.png");
    }


    fn video_item(id: i64, path: &str, options: VideoOptions) -> SessionItem {
        SessionItem { song: None, background: None, id, title: None, kind: SessionItemKind::Video { path: path.into(), options } }
    }

    /// Song(a, b; song.jpg) → Video → Text(c)
    fn video_setup() -> Harness {
        let mut h = setup();
        {
            let mut projects = h.store.projects.borrow_mut();
            let items = &mut projects[0].session.items;
            items[0].background = Some("song.jpg".into());
            items[1] = video_item(20, "clip.mp4", VideoOptions { start_ms: 2_000, end_ms: Some(9_000), ..VideoOptions::default() });
        }
        h.load();
        h
    }

    fn report(h: &Harness, phase: PlaybackPhase, position_ms: u64) -> PlaybackReport {
        let p = h.playback();
        PlaybackReport { session_id: p.session_id, generation: p.generation, phase, position_ms, duration_ms: Some(10_000), error: None }
    }

    #[test]
    fn cueing_a_video_opens_one_session_and_leaving_closes_it_once() {
        let mut h = video_setup();
        h.take_directives();
        let events = h.send(EngineCommand::GoToItem { item_id: 20 });
        let playback = h.playback().clone();
        assert_eq!((playback.item_id, playback.position_ms, playback.end_ms), (20, 2_000, Some(9_000)));
        assert!(!playback.paused);
        assert!(matches!(h.take_directives().as_slice(), [MediaDirective::Open { playback: p }] if p.session_id == playback.session_id));
        assert!(events.iter().any(|e| matches!(e, EngineEvent::PlaybackChanged { playback: Some(_), .. })));

        h.send(EngineCommand::GoToItem { item_id: 30 });
        assert_eq!(h.take_directives(), vec![MediaDirective::Close { session_id: playback.session_id }]);
        assert!(h.engine.snapshot().playback.is_none());
        h.send(EngineCommand::GoToStep { index: 0 });
        assert!(h.take_directives().is_empty(), "the session closes exactly once");
    }

    #[test]
    fn a_foreground_video_never_becomes_the_next_song_background() {
        let mut h = video_setup();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        assert_eq!(h.engine.snapshot().program_state.background.as_ref().map(|b| b.path.as_str()), Some("clip.mp4"));
        h.send(EngineCommand::GoToItem { item_id: 30 });
        // Item 30 has no background of its own: the intentional song background returns.
        assert_eq!(h.engine.snapshot().program_state.background.as_ref().map(|b| b.path.as_str()), Some("song.jpg"));
    }

    #[test]
    fn without_a_song_background_leaving_a_video_clears_the_background() {
        let mut h = video_setup();
        h.store.projects.borrow_mut()[0].session.items[0].background = None;
        h.load();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        h.send(EngineCommand::GoToItem { item_id: 30 });
        assert_eq!(h.engine.snapshot().program_state.background, None);
    }

    #[test]
    fn recueing_the_same_file_rejects_commands_for_the_earlier_session() {
        let mut h = video_setup();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        let first = h.playback().session_id;
        h.send(EngineCommand::GoToItem { item_id: 30 });
        h.send(EngineCommand::GoToItem { item_id: 20 });
        let second = h.playback().session_id;
        assert_ne!(first, second);
        h.take_directives();

        let events = h.send(EngineCommand::Media { session_id: Some(first), action: MediaAction::Pause });
        assert!(matches!(events.as_slice(), [EngineEvent::Error { source, .. }] if source == "media"));
        assert!(!h.playback().paused);
        assert!(h.take_directives().is_empty());

        h.send(EngineCommand::Media { session_id: Some(second), action: MediaAction::Pause });
        assert!(h.playback().paused);
        assert_eq!(h.take_directives(), vec![MediaDirective::Pause { session_id: second }]);
    }

    #[test]
    fn reports_from_earlier_sessions_or_seek_generations_are_ignored() {
        let mut h = video_setup();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        let stale_generation = report(&h, PlaybackPhase::Playing, 8_000);
        h.send(EngineCommand::Media { session_id: None, action: MediaAction::Seek { position_ms: 3_000 } });
        assert_eq!(h.playback().generation, 1);

        assert!(h.report(stale_generation).is_empty(), "a pre-seek position must not snap back");
        let mut other_session = report(&h, PlaybackPhase::Playing, 1_000);
        other_session.session_id += 99;
        assert!(h.report(other_session).is_empty());
        assert_eq!(h.playback().position_ms, 3_000);

        h.now_ms = 5_000;
        let events = h.report(report(&h, PlaybackPhase::Playing, 3_250));
        let p = h.playback();
        assert_eq!((p.phase, p.position_ms, p.position_at_ms, p.duration_ms), (PlaybackPhase::Playing, 3_250, 5_000, Some(10_000)));
        // Position reports revise playback only, never the project/program version.
        assert!(matches!(events.as_slice(), [EngineEvent::PlaybackChanged { .. }]));
    }

    #[test]
    fn seeks_clamp_to_the_clip_and_start_a_new_generation() {
        let mut h = video_setup();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        let session_id = h.playback().session_id;
        h.take_directives();
        h.send(EngineCommand::Media { session_id: None, action: MediaAction::Seek { position_ms: 500 } });
        h.send(EngineCommand::Media { session_id: None, action: MediaAction::Seek { position_ms: 60_000 } });
        assert_eq!(
            h.take_directives(),
            vec![
                MediaDirective::Seek { session_id, generation: 1, position_ms: 2_000 },
                MediaDirective::Seek { session_id, generation: 2, position_ms: 8_999 },
            ]
        );
    }

    #[test]
    fn stop_pauses_and_returns_to_the_clip_start() {
        let mut h = video_setup();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        let session_id = h.playback().session_id;
        h.take_directives();
        h.send(EngineCommand::Stop);
        assert!(h.playback().paused);
        assert_eq!(
            h.take_directives(),
            vec![MediaDirective::Pause { session_id }, MediaDirective::Seek { session_id, generation: 1, position_ms: 2_000 }]
        );
    }

    #[test]
    fn play_after_the_clip_ended_restarts_from_the_clip_start() {
        let mut h = video_setup();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        h.report(report(&h, PlaybackPhase::Ended, 9_000));
        assert!(h.playback().paused, "a non-looping clip pauses at its end");
        let session_id = h.playback().session_id;
        h.take_directives();
        h.send(EngineCommand::Play);
        assert_eq!(
            h.take_directives(),
            vec![MediaDirective::Seek { session_id, generation: 1, position_ms: 2_000 }, MediaDirective::Play { session_id }]
        );
    }

    #[test]
    fn logo_suspends_foreground_media_and_live_resumes_it() {
        let mut h = video_setup();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        let session_id = h.playback().session_id;
        h.take_directives();

        // Black and Clear leave transport running.
        h.send(EngineCommand::SetDisplay { display: DisplayMode::Black });
        h.send(EngineCommand::SetDisplay { display: DisplayMode::Clear });
        assert!(h.take_directives().is_empty());

        h.send(EngineCommand::SetDisplay { display: DisplayMode::Logo });
        assert!(h.playback().suspended && !h.playback().paused);
        assert_eq!(h.take_directives(), vec![MediaDirective::Pause { session_id }]);

        h.send(EngineCommand::SetDisplay { display: DisplayMode::Live });
        assert!(h.playback().running());
        assert_eq!(h.take_directives(), vec![MediaDirective::Play { session_id }]);
    }

    #[test]
    fn a_paused_video_stays_paused_after_logo() {
        let mut h = video_setup();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        h.send(EngineCommand::Pause);
        h.take_directives();
        h.send(EngineCommand::SetDisplay { display: DisplayMode::Logo });
        h.send(EngineCommand::SetDisplay { display: DisplayMode::Live });
        assert!(h.take_directives().is_empty());
        assert!(h.playback().paused);
    }

    #[test]
    fn video_cued_under_logo_waits_for_live() {
        let mut h = video_setup();
        h.send(EngineCommand::SetDisplay { display: DisplayMode::Logo });
        h.send(EngineCommand::GoToItem { item_id: 20 });
        assert!(h.playback().suspended);
        let session_id = h.playback().session_id;
        h.take_directives();
        h.send(EngineCommand::SetDisplay { display: DisplayMode::Live });
        assert_eq!(h.take_directives(), vec![MediaDirective::Play { session_id }]);
    }

    /// Song(a, b; song.jpg) → Audio → Text(c)
    fn audio_setup() -> Harness {
        let mut h = setup();
        {
            let mut projects = h.store.projects.borrow_mut();
            let items = &mut projects[0].session.items;
            items[0].background = Some("song.jpg".into());
            items[1] = SessionItem {
                song: None,
                background: None,
                id: 20,
                title: None,
                kind: SessionItemKind::Audio { path: "song.mp3".into(), options: VideoOptions { looping: true, volume: 40, ..VideoOptions::default() } },
            };
        }
        h.load();
        h
    }

    #[test]
    fn cueing_standalone_audio_opens_an_audio_session_over_the_song_background() {
        let mut h = audio_setup();
        h.send(EngineCommand::GoToItem { item_id: 0 });
        h.take_directives();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        let playback = h.playback().clone();
        assert_eq!((playback.kind, playback.volume, playback.looping, playback.paused), (PlaybackKind::Audio, 40, true, false));
        assert!(matches!(h.take_directives().as_slice(), [MediaDirective::Open { playback: p }] if p.kind == PlaybackKind::Audio));
        assert_eq!(h.engine.snapshot().program_state.background.as_ref().map(|b| b.path.as_str()), Some("song.jpg"));

        h.send(EngineCommand::GoToItem { item_id: 30 });
        assert_eq!(h.take_directives(), vec![MediaDirective::Close { session_id: playback.session_id }]);
    }

    #[test]
    fn standalone_audio_keeps_playing_under_the_logo() {
        let mut h = audio_setup();
        h.send(EngineCommand::SetDisplay { display: DisplayMode::Logo });
        h.send(EngineCommand::GoToItem { item_id: 20 });
        assert!(h.playback().running(), "audio cued under the Logo starts");
        h.take_directives();
        h.send(EngineCommand::SetDisplay { display: DisplayMode::Live });
        h.send(EngineCommand::SetDisplay { display: DisplayMode::Logo });
        assert!(h.playback().running());
        assert!(h.take_directives().is_empty());
    }

    #[test]
    fn editing_the_active_video_options_reconfigures_without_restarting() {
        let mut h = video_setup();
        h.send(EngineCommand::GoToItem { item_id: 20 });
        let session_id = h.playback().session_id;
        h.take_directives();
        let mut project = h.engine.snapshot().project.clone().unwrap();
        project.session.items[1] = video_item(20, "clip.mp4", VideoOptions { volume: 40, looping: true, ..VideoOptions::default() });
        h.send(EngineCommand::SaveProject { project });
        let p = h.playback();
        assert_eq!((p.session_id, p.volume, p.looping, p.start_ms, p.end_ms), (session_id, 40, true, 0, None));
        assert!(matches!(h.take_directives().as_slice(), [MediaDirective::Configure { .. }]));
    }

    #[test]
    fn media_commands_without_a_cued_session_are_errors() {
        let mut h = setup();
        h.load();
        let events = h.send(EngineCommand::Play);
        assert!(matches!(events.as_slice(), [EngineEvent::Error { source, .. }] if source == "media"));
        assert!(h.media.directives.is_empty());
    }

    #[test]
    fn shorthand_and_addressed_transport_commands_are_equivalent() {
        let (mut a, mut b) = (video_setup(), video_setup());
        a.send(EngineCommand::GoToItem { item_id: 20 });
        b.send(EngineCommand::GoToItem { item_id: 20 });
        let id = b.playback().session_id;
        a.take_directives();
        b.take_directives();
        a.send(EngineCommand::Seek { time_ms: 4_000 });
        b.send(EngineCommand::Media { session_id: Some(id), action: MediaAction::Seek { position_ms: 4_000 } });
        assert_eq!(a.take_directives(), b.take_directives());
        assert_eq!(a.playback(), b.playback());
    }

    #[test]
    fn coalescing_keeps_the_latest_scrub_and_preserves_order() {
        let seek = |position_ms| EngineInput::Command(EngineCommand::Media { session_id: Some(7), action: MediaAction::Seek { position_ms } });
        let cue = EngineInput::Command(EngineCommand::Cue { item_id: 1, index: 0 });
        let merged = coalesce(vec![seek(1), seek(2), seek(3), cue, seek(4), seek(5)]);
        let positions: Vec<_> = merged
            .iter()
            .map(|input| match input {
                EngineInput::Command(EngineCommand::Media { action: MediaAction::Seek { position_ms }, .. }) => Some(*position_ms),
                _ => None,
            })
            .collect();
        assert_eq!(positions, vec![Some(3), None, Some(5)]);
    }

    #[test]
    fn coalescing_never_merges_different_sessions_projects_or_phases() {
        let seek = |session_id| EngineInput::Command(EngineCommand::Media { session_id: Some(session_id), action: MediaAction::Seek { position_ms: 0 } });
        assert_eq!(coalesce(vec![seek(1), seek(2)]).len(), 2);
        let save = |id| EngineInput::Command(EngineCommand::SaveProject { project: ProjectModel::empty(id, "p") });
        assert_eq!(coalesce(vec![save(1), save(1), save(2)]).len(), 2);
        let report = |phase| EngineInput::Report(PlaybackReport { session_id: 1, generation: 0, phase, position_ms: 0, duration_ms: None, error: None });
        assert_eq!(coalesce(vec![report(PlaybackPhase::Playing), report(PlaybackPhase::Playing), report(PlaybackPhase::Ended)]).len(), 2);
    }
}
