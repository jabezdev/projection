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
    /// A new size or scene is waiting in the worker's slots.
    Wake,
}

/// Presentation telemetry, sampled by the app for logs and status.
#[derive(Debug, Clone, Default)]
pub struct OutputStats {
    pub presented: u64,
    pub max_interval_ms: f64,
    pub surface_errors: u64,
}

/// What a scene renderer needs from the engine.
#[derive(Clone)]
pub(crate) struct Resources {
    pub gpu: Arc<Gpu>,
    pub fonts: Arc<Fonts>,
    pub media: Arc<MediaTextures>,
    /// Told when an output starts or stops waiting for new media.
    pub status: Arc<std::sync::OnceLock<StatusSink>>,
}

/// Receives `(output, loading)` whenever a shown output starts or stops
/// waiting for its new media (the previous picture stays up meanwhile).
pub type StatusSink = Box<dyn Fn(OutputRole, bool) + Send + Sync>;

/// Latest requested size and scene. Kept outside the event queue so a burst
/// while the worker is blocked can never drop the newest value.
#[derive(Default)]
struct Pending {
    size: Mutex<Option<(u32, u32)>>,
    scene: Mutex<Option<Scene>>,
    /// The window lost its contents (exposed, mapped) and needs a new frame.
    redraw: std::sync::atomic::AtomicBool,
    /// The window is on screen. Hidden outputs present nothing.
    shown: std::sync::atomic::AtomicBool,
    /// Content aspect ratio, letterboxed in the window; `None` fills it.
    aspect: Mutex<Option<(u32, u32)>>,
}

pub(crate) struct OutputWorker {
    events: Sender<OutputEvent>,
    pending: Arc<Pending>,
    stats: Arc<Mutex<OutputStats>>,
    thread: Option<JoinHandle<()>>,
    role: OutputRole,
}
