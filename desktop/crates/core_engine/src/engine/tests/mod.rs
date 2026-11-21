use super::*;
use protocol::{
    Background, BackgroundKind, DisplayMode, EngineCommand, EngineEvent, MediaAction, MediaDirective,
    PlaybackKind, PlaybackPhase, PlaybackReport, PlaybackState, ProjectModel, SessionItem, SessionItemKind, VideoOptions,
};
use protocol::SessionModel;
use std::cell::RefCell;

mod coalescing;
mod cues;
mod media;

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

fn section(id: i64) -> SessionItem {
    SessionItem { song: None, background: None, id, title: Some("Worship".into()), kind: SessionItemKind::Section {} }
}
