use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crossbeam::channel::Sender;

use crate::media::MediaTextures;
use crate::scene::Scene;
use crate::text::Fonts;
use crate::Gpu;

mod presenter;
mod renderer;
mod surface;
mod worker;

pub(crate) use renderer::Renderer;
pub(crate) use surface::{configure_surface, surface_config};
use presenter::*;
use surface::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputRole {
    Projection,
    Confidence,
}

impl OutputRole {
    pub fn label(self) -> &'static str {
        match self {
            OutputRole::Projection => "projection",
            OutputRole::Confidence => "confidence",
        }
    }
}

#[derive(Debug)]
pub(crate) enum OutputEvent {
    Shutdown,
    Wake,
}

#[derive(Debug, Clone, Default)]
pub struct OutputStats {
    pub presented: u64,
    pub max_interval_ms: f64,
    pub surface_errors: u64,
}

#[derive(Clone)]
pub(crate) struct Resources {
    pub gpu: Arc<Gpu>,
    pub fonts: Arc<Fonts>,
    pub media: Arc<MediaTextures>,
    pub status: Arc<std::sync::OnceLock<StatusSink>>,
}

pub type StatusSink = Box<dyn Fn(OutputRole, bool) + Send + Sync>;

#[derive(Default)]
struct Pending {
    size: Mutex<Option<(u32, u32)>>,
    scene: Mutex<Option<Scene>>,
    redraw: std::sync::atomic::AtomicBool,
    shown: std::sync::atomic::AtomicBool,
    aspect: Mutex<Option<(u32, u32)>>,
}

pub(crate) struct OutputWorker {
    events: Sender<OutputEvent>,
    pending: Arc<Pending>,
    stats: Arc<Mutex<OutputStats>>,
    thread: Option<JoinHandle<()>>,
    role: OutputRole,
}
