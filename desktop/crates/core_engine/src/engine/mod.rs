use protocol::{
    Background, EngineCommand, EngineEvent, EngineSnapshot, MediaDirective, PlaybackPhase, PlaybackReport, LiveService, SessionId,
};

mod background;
mod coalesce;
mod commands;
mod navigation;
mod selection;
mod session;

pub use coalesce::coalesce;
use background::background_kind;
use navigation::navigation_target;
use selection::{next_cueable, previous_cueable};
use session::{seek, transport_directive};

pub trait Assets {
    fn check_background(&self, background: &Background) -> anyhow::Result<()>;
}

pub trait MediaBackend {
    fn send(&mut self, directive: MediaDirective);
}

pub trait ServiceStore {
    fn load_service(&self, service_id: &str) -> anyhow::Result<LiveService>;
    fn save_service(&self, service: &protocol::model::Service) -> anyhow::Result<LiveService>;
    fn resolve_service(&self, service: &protocol::model::Service) -> anyhow::Result<LiveService>;
    fn step_service(&self, service_id: &str, redo: bool, request: u64) -> anyhow::Result<Option<LiveService>>;
}

pub struct Ports<'a> {
    pub assets: &'a dyn Assets,
    pub media: &'a mut dyn MediaBackend,
    pub store: &'a dyn ServiceStore,
    pub now_ms: i64,
}

#[derive(Debug, Clone)]
pub enum EngineInput {
    Command(EngineCommand),
    Report(PlaybackReport),
}

pub struct Engine {
    state: EngineSnapshot,
    song_background: Option<Background>,
    next_session_id: SessionId,
    autoplay: bool,
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

    pub fn with_session_seed(seed: SessionId) -> Self {
        Self { state: EngineSnapshot::default(), song_background: None, next_session_id: seed, autoplay: true }
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

    pub fn handle(&mut self, cmd: EngineCommand, ports: &mut Ports) -> Vec<EngineEvent> {
        self.observe(ports, |engine, ports, errors| engine.apply(cmd.normalize(), ports, errors))
    }

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
        let service_changed = change(self, ports, &mut errors);
        let program_changed = self.state.program_state != program_before;

        let mut events = Vec::new();
        if service_changed || program_changed {
            self.state.version += 1;
            let version = self.state.version;
            if service_changed {
                events.push(EngineEvent::ServiceChanged {
                    service: self.state.service.clone(),
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
}

fn error(source: &str, message: impl Into<String>) -> EngineEvent {
    EngineEvent::Error {
        source: source.to_string(),
        message: message.into(),
        recoverable: true,
    }
}

#[cfg(test)]
mod tests;
