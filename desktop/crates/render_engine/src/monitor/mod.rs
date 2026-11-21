//! Live monitors: real-time views of the outputs inside the operator window.
//!
//! Each monitor presents into a small native child window placed over the
//! operator UI. It runs its own thread, renderer and painter, so it never
//! delays an output, and draws the same scene from the same shared media
//! textures: no readback, no encoding and no second decoder. The scene is
//! laid out at the output's own size (so it looks exactly like the output),
//! painted offscreen, then scaled into the monitor's surface.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam::channel::{bounded, Sender};

use crate::output::{surface_config, OutputRole, Resources};
use crate::scene::Scene;

mod scaler;
mod surface_monitor;

use scaler::*;
use surface_monitor::*;

/// Offscreen format: non-sRGB like the outputs' views, so blending matches.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Layout happens at the output's size up to this width.
const MAX_RENDER_WIDTH: u32 = 1920;
/// Monitors are for watching, not timing: 30 fps keeps them light.
const FRAME_INTERVAL: Duration = Duration::from_millis(33);
/// Idle monitors check for media readiness and clock changes this often.
const IDLE_POLL: Duration = Duration::from_millis(33);
const HIDDEN_POLL: Duration = Duration::from_millis(250);

/// A child window to present a monitor into. It must outlive the
/// [`crate::RenderEngine`]; the engine joins monitor threads on shutdown.
pub struct MonitorSpec {
    pub role: OutputRole,
    pub target: Box<dyn wgpu::DisplayAndWindowHandle + 'static>,
}

#[derive(Default)]
struct Pending {
    scene: Mutex<Option<Scene>>,
    /// The output's content size (after letterboxing): the layout size.
    content: Mutex<Option<(u32, u32)>>,
    /// The monitor surface's size in physical pixels; `None` while hidden.
    surface: Mutex<Option<Option<(u32, u32)>>>,
    stop: AtomicBool,
}

pub(crate) struct MonitorWorker {
    pending: Arc<Pending>,
    wake: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl MonitorWorker {
    pub fn spawn(resources: Resources, role: OutputRole, surface: wgpu::Surface<'static>, content: (u32, u32), scene: Option<Scene>) -> anyhow::Result<Self> {
        let label = format!("{} monitor", role.label());
        let (config, view_format) = surface_config(&resources.gpu, &surface, &label, 1, 1)?;
        let pending = Arc::new(Pending::default());
        *pending.content.lock().expect("monitor poisoned") = Some(content);
        *pending.scene.lock().expect("monitor poisoned") = scene;
        let (wake, woken) = bounded(1);
        let thread = {
            let pending = pending.clone();
            std::thread::Builder::new().name(format!("monitor-{}", role.label())).spawn(move || {
                let monitor = Monitor::new(&resources.gpu, role, label, surface, config, view_format);
                monitor.run(&resources, &pending, &woken);
            })?
        };
        Ok(Self { pending, wake, thread: Some(thread) })
    }

    pub fn set_scene(&self, scene: Scene) {
        *self.pending.scene.lock().expect("monitor poisoned") = Some(scene);
        self.wake();
    }

    pub fn set_content_size(&self, size: (u32, u32)) {
        *self.pending.content.lock().expect("monitor poisoned") = Some(size);
        self.wake();
    }

    /// Shows the monitor at `size` physical pixels, or parks it (`None`).
    pub fn set_surface(&self, size: Option<(u32, u32)>) {
        *self.pending.surface.lock().expect("monitor poisoned") = Some(size.filter(|&(w, h)| w > 0 && h > 0));
        self.wake();
    }

    fn wake(&self) {
        let _ = self.wake.try_send(());
    }

    pub fn signal_stop(&self) {
        self.pending.stop.store(true, Ordering::Release);
        self.wake();
    }

    pub fn join(mut self) {
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                log::error!("Monitor thread panicked");
            }
        }
    }
}

#[cfg(test)]
mod tests;
