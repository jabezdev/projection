//! Runs the engine on its own thread behind one bounded input queue shared by
//! the operator UI, WebSocket clients and media backends.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crossbeam::channel::{bounded, Receiver, RecvTimeoutError, Sender, TrySendError};
use tokio::sync::broadcast;

use core_engine::{coalesce, Assets, Engine, EngineInput, MediaBackend, Ports, ProjectStore};
use protocol::{EngineCommand, EngineEvent, EngineSnapshot, PlaybackReport};

/// Room for bursts of input; a full queue means the engine is stalled.
const QUEUE_CAPACITY: usize = 1024;
/// Inputs applied before publishing, so continuous input cannot delay cues.
const MAX_BATCH: usize = 64;
/// Wake periodically to observe shutdown.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Cloneable sender for everything the engine consumes.
#[derive(Clone)]
pub struct EngineHandle {
    inputs: Sender<EngineInput>,
}

impl EngineHandle {
    pub fn channel() -> (Self, Receiver<EngineInput>) {
        let (inputs, receiver) = bounded(QUEUE_CAPACITY);
        (Self { inputs }, receiver)
    }

    /// Queues an operator or remote command; the caller learns if it was dropped.
    pub fn command(&self, command: EngineCommand) -> Result<(), String> {
        match self.inputs.try_send(EngineInput::Command(command)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err("The engine is busy; the command was not applied.".into()),
            Err(TrySendError::Disconnected(_)) => Err("The engine has stopped.".into()),
        }
    }

    /// Queues a backend report. Dropping one is harmless: the next supersedes it.
    pub fn report(&self, report: PlaybackReport) {
        let _ = self.inputs.try_send(EngineInput::Report(report));
    }
}

pub struct EngineLoop<A, M, S> {
    pub inputs: Receiver<EngineInput>,
    pub assets: A,
    pub media: M,
    pub store: S,
    pub snapshot: Arc<RwLock<EngineSnapshot>>,
    pub events: broadcast::Sender<EngineEvent>,
    pub shutdown: Arc<AtomicBool>,
}

impl<A: Assets, M: MediaBackend, S: ProjectStore> EngineLoop<A, M, S> {
    pub fn run(mut self) {
        let mut engine = Engine::with_session_seed(crate::now_ms().max(1) as u64);
        while !self.shutdown.load(Ordering::Acquire) {
            let first = match self.inputs.recv_timeout(POLL_INTERVAL) {
                Ok(input) => input,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            let mut batch = vec![first];
            batch.extend(self.inputs.try_iter().take(MAX_BATCH - 1));

            let mut ports = Ports {
                assets: &self.assets,
                media: &mut self.media,
                store: &self.store,
                now_ms: crate::now_ms(),
            };
            let mut emitted = Vec::new();
            for input in coalesce(batch) {
                match &input {
                    EngineInput::Command(command) => log::debug!("command {command:?}"),
                    EngineInput::Report(report) => log::trace!("report {report:?}"),
                }
                emitted.extend(engine.handle_input(input, &mut ports));
            }
            if !emitted.is_empty() {
                *self.snapshot.write().expect("snapshot lock poisoned") = engine.snapshot().clone();
                for event in emitted {
                    let _ = self.events.send(event);
                }
            }
        }
    }
}
