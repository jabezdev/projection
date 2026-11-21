use super::*;
use protocol::{
    Background, BackgroundKind, DisplayMode, EngineCommand, EngineEvent, MediaAction, MediaDirective,
    PlaybackKind, PlaybackPhase, PlaybackReport, PlaybackState, LiveService, SessionItem, SessionItemKind, VideoOptions,
};
use protocol::model::Service;
use std::cell::RefCell;

mod coalescing;
mod cues;
mod media;

#[derive(Default)]
struct FakeAssets {
    backgrounds: RefCell<Vec<String>>,
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
    services: RefCell<Vec<LiveService>>,
}

impl ServiceStore for FakeStore {
    fn load_service(&self, service_id: &str) -> anyhow::Result<LiveService> {
        self.services.borrow().iter().find(|s| s.id() == service_id).cloned().ok_or_else(|| anyhow::anyhow!("service {service_id} not found"))
    }
    fn save_service(&self, service: &Service) -> anyhow::Result<LiveService> {
        let mut services = self.services.borrow_mut();
        let stored = services.iter_mut().find(|s| s.id() == service.id).ok_or_else(|| anyhow::anyhow!("service {} not found", service.id))?;
        stored.service = service.clone();
        Ok(stored.clone())
    }
    fn resolve_service(&self, service: &Service) -> anyhow::Result<LiveService> {
        self.load_service(&service.id)
    }
}

fn live(id: &str, title: &str, items: Vec<SessionItem>) -> LiveService {
    LiveService { service: Service { id: id.into(), scheduled_at_ms: 0, title: title.into(), leader: None, notes: None, ended_at_ms: None, items: Vec::new() }, items }
}

fn text_item(id: i64, steps: &[&str]) -> SessionItem {
    SessionItem {
        song: None,
        background: None,
        id: id.to_string(),
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
        id: id.to_string(),
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
        self.send(EngineCommand::LoadService { service_id: "1".into() });
    }
    fn items(&self) -> std::cell::RefMut<'_, Vec<SessionItem>> {
        std::cell::RefMut::map(self.store.services.borrow_mut(), |s| &mut s[0].items)
    }
    fn save(&mut self, change: impl FnOnce(&mut Vec<SessionItem>)) -> Vec<EngineEvent> {
        change(&mut self.items());
        let service = self.store.services.borrow()[0].service.clone();
        self.send(EngineCommand::SaveService { service })
    }
    fn playback(&self) -> &PlaybackState {
        self.engine.snapshot().playback.as_ref().expect("a cued media session")
    }
    fn take_directives(&mut self) -> Vec<MediaDirective> {
        std::mem::take(&mut self.media.directives)
    }
}

fn setup() -> Harness {
    let service = live("1", "Service", vec![text_item(10, &["a", "b"]), image_item(20, "bg.png"), text_item(30, &["c"])]);
    let store = FakeStore::default();
    store.services.borrow_mut().push(service);
    Harness { engine: Engine::new(), assets: FakeAssets::default(), media: FakeMedia::default(), store, now_ms: 1_000 }
}

fn selection(engine: &Engine) -> (Option<i64>, Option<u32>, &str) {
    let p = &engine.snapshot().program_state;
    (
        p.active_item_id.as_ref().map(|id| id.parse().unwrap()),
        p.active_step_index,
        p.active_text.as_str(),
    )
}

fn section(id: i64) -> SessionItem {
    SessionItem { song: None, background: None, id: id.to_string(), title: Some("Worship".into()), kind: SessionItemKind::Section {} }
}
