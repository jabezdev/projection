use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use protocol::CaptureSource;
use render_engine::PixelFrame;
use serde::Serialize;

use super::*;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CaptureStatus {
    pub id: String,
    pub name: String,
    pub error: Option<String>,
}

pub struct Captures {
    pub(super) backends: Vec<Arc<dyn Backend>>,
    pub(super) choice: Mutex<String>,
    pub(super) running: Mutex<HashMap<String, (CaptureSource, Box<dyn CaptureStream>, FrameSlot)>>,
    pub(super) failed: Mutex<HashMap<String, (CaptureSource, String, FrameSlot)>>,
}

impl Captures {
    pub fn new(choice: &str, data_dir: &std::path::Path) -> Self {
        Self::with_backends(detect(Some(data_dir)), choice)
    }

    pub(super) fn with_backends(backends: Vec<Arc<dyn Backend>>, choice: &str) -> Self {
        Self { backends, choice: Mutex::new(choice.to_string()), running: Mutex::default(), failed: Mutex::default() }
    }

    pub fn method_labels(&self) -> Vec<&'static str> {
        self.backends.iter().map(|b| b.label()).collect()
    }

    pub fn backend(&self) -> Option<Arc<dyn Backend>> {
        let choice = self.choice.lock().expect("capture choice poisoned").clone();
        self.backends.iter().find(|b| b.name() == choice).or(self.backends.first()).cloned()
    }

    pub fn set_choice(&self, choice: &str) {
        *self.choice.lock().expect("capture choice poisoned") = choice.to_string();
        let sources: Vec<CaptureSource> = self.running.lock().expect("captures poisoned").values().map(|(s, ..)| s.clone()).collect();
        self.running.lock().expect("captures poisoned").clear();
        self.failed.lock().expect("captures poisoned").clear();
        self.set_active(&sources);
    }

    pub fn set_active(&self, sources: &[CaptureSource]) {
        let mut running = self.running.lock().expect("captures poisoned");
        let mut failed = self.failed.lock().expect("captures poisoned");
        running.retain(|id, _| sources.iter().any(|s| s.id == *id));
        failed.retain(|id, _| sources.iter().any(|s| s.id == *id));
        for source in sources {
            if running.contains_key(&source.id) || failed.contains_key(&source.id) {
                continue;
            }
            let slot = FrameSlot::default();
            let started = match self.backend() {
                Some(backend) => backend.start(source, slot.clone()),
                None => Err(anyhow::anyhow!("No screen capture method is available on this computer")),
            };
            match started {
                Ok(stream) => {
                    log::info!("Capturing {} ({})", source.name, source.id);
                    running.insert(source.id.clone(), (source.clone(), stream, slot));
                }
                Err(e) => {
                    log::error!("Cannot capture {}: {e:#}", source.name);
                    let reason = format!("{e:#}");
                    slot.lose(reason.clone());
                    failed.insert(source.id.clone(), (source.clone(), reason, slot));
                }
            }
        }
    }

    pub fn frame(&self, id: &str) -> Option<Arc<dyn PixelFrame>> {
        let slot = self.running.lock().expect("captures poisoned").get(id).map(|(_, _, slot)| slot.clone());
        let slot = slot.or_else(|| self.failed.lock().expect("captures poisoned").get(id).map(|(.., slot)| slot.clone()));
        match slot {
            Some(slot) => slot.frame(Instant::now()).map(|f| f as Arc<dyn PixelFrame>),
            None => None,
        }
    }

    pub fn status(&self) -> Vec<CaptureStatus> {
        let running = self.running.lock().expect("captures poisoned");
        let failed = self.failed.lock().expect("captures poisoned");
        running
            .values()
            .map(|(source, _, slot)| CaptureStatus { id: source.id.clone(), name: source.name.clone(), error: slot.lost() })
            .chain(failed.values().map(|(source, error, _)| CaptureStatus { id: source.id.clone(), name: source.name.clone(), error: Some(error.clone()) }))
            .collect()
    }

    pub fn shutdown(&self) {
        self.running.lock().expect("captures poisoned").clear();
    }
}
