use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crossbeam::channel::{bounded, Receiver, RecvTimeoutError, Sender, TrySendError};
use tokio::sync::broadcast;

use core_engine::{coalesce, Assets, Engine, EngineInput, MediaBackend, Ports, ServiceStore};
use protocol::{EngineCommand, EngineEvent, EngineSnapshot, PlaybackReport};

const QUEUE_CAPACITY: usize = 1024;
const MAX_BATCH: usize = 64;
const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone)]
pub struct EngineHandle {
    inputs: Sender<EngineInput>,
}

impl EngineHandle {
    pub fn channel() -> (Self, Receiver<EngineInput>) {
        let (inputs, receiver) = bounded(QUEUE_CAPACITY);
        (Self { inputs }, receiver)
    }

    pub fn command(&self, command: EngineCommand) -> Result<(), String> {
        match self.inputs.try_send(EngineInput::Command(command)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err("The engine is busy; the command was not applied.".into()),
            Err(TrySendError::Disconnected(_)) => Err("The engine has stopped.".into()),
        }
    }

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

impl<A: Assets, M: MediaBackend, S: ServiceStore> EngineLoop<A, M, S> {
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
