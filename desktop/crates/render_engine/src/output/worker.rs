use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam::channel::{bounded, TrySendError};

use crate::scene::Scene;

use super::*;

impl OutputWorker {
    pub fn spawn(
        resources: Resources,
        role: OutputRole,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        stall: Option<Duration>,
    ) -> anyhow::Result<Self> {
        let (events, receiver) = bounded(64);
        let stats = Arc::new(Mutex::new(OutputStats::default()));
        let pending = Arc::new(Pending::default());
        let presenter = Presenter::new(&resources.gpu, role, surface, width, height)?;
        let thread = {
            let (stats, pending) = (stats.clone(), pending.clone());
            std::thread::Builder::new()
                .name(format!("output-{}", role.label()))
                .spawn(move || presenter.run(&resources, receiver, &pending, stats, stall))?
        };
        Ok(Self { events, pending, stats, thread: Some(thread), role })
    }

    pub fn resize(&self, width: u32, height: u32) {
        *self.pending.size.lock().expect("output size poisoned") = Some((width, height));
        self.wake();
    }

    pub fn set_scene(&self, scene: Scene) {
        *self.pending.scene.lock().expect("output scene poisoned") = Some(scene);
        self.wake();
    }

    pub fn redraw(&self) {
        self.pending.redraw.store(true, std::sync::atomic::Ordering::Release);
        self.wake();
    }

    pub fn set_shown(&self, shown: bool) {
        self.pending.shown.store(shown, std::sync::atomic::Ordering::Release);
        self.wake();
    }

    pub fn set_aspect(&self, aspect: Option<(u32, u32)>) {
        *self.pending.aspect.lock().expect("output aspect poisoned") = aspect;
        self.pending.redraw.store(true, std::sync::atomic::Ordering::Release);
        self.wake();
    }

    pub(super) fn wake(&self) {
        match self.events.try_send(OutputEvent::Wake) {
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    pub fn stats(&self) -> OutputStats {
        self.stats.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn signal_stop(&self) {
        let _ = self.events.send_timeout(OutputEvent::Shutdown, Duration::from_secs(1));
    }

    pub fn join(mut self) {
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                log::error!("{} output thread panicked", self.role.label());
            }
        }
    }
}
