use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam::channel::{Receiver, RecvTimeoutError};

use crate::paint::Painter;
use crate::Gpu;

use super::*;

/// Idle outputs check for pending media and clock changes this often.
pub(super) const IDLE_POLL: Duration = Duration::from_millis(16);

/// Debug aid (`MAGI_OUTPUT_CAPTURE=<dir>`): saves each scene as presented,
/// once its transitions settle, so the real output can be inspected even
/// when its window is covered.
pub(super) struct DebugCapture {
    pub(super) dir: std::path::PathBuf,
    pub(super) since: Option<Instant>,
    pub(super) count: u32,
}

impl DebugCapture {
    pub(super) fn from_env() -> Option<Self> {
        let dir = std::path::PathBuf::from(std::env::var_os("MAGI_OUTPUT_CAPTURE")?);
        std::fs::create_dir_all(&dir).ok()?;
        Some(Self { dir, since: None, count: 0 })
    }

    /// Saves once transitions end and media is shown (or after 5 s regardless).
    pub(super) fn after_frame(&mut self, role: OutputRole, settled: bool, save: impl FnOnce() -> (Vec<u8>, (u32, u32))) {
        let Some(since) = self.since else { return };
        if !settled && since.elapsed() < Duration::from_secs(5) {
            return;
        }
        self.since = None;
        self.count += 1;
        let (pixels, (w, h)) = save();
        let path = self.dir.join(format!("{}-{:03}.png", role.label(), self.count));
        match image::save_buffer(&path, &pixels, w, h, image::ColorType::Rgba8) {
            Ok(()) => log::info!("Captured {}", path.display()),
            Err(e) => log::error!("Saving {}: {e}", path.display()),
        }
    }
}

pub(super) struct Presenter {
    pub(super) role: OutputRole,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) config: wgpu::SurfaceConfiguration,
    /// Format of the views drawn into: non-sRGB, so blending happens on
    /// sRGB-encoded values as in browsers.
    pub(super) view_format: wgpu::TextureFormat,
    /// True while the window has zero size (minimized or not yet mapped).
    pub(super) zero_size: bool,
    /// True while the window is shown; starts hidden.
    pub(super) shown: bool,
    /// True once the surface matches `config`. A failed configure is retried
    /// before the next frame.
    pub(super) configured: bool,
}

impl Presenter {
    pub(super) fn new(gpu: &Gpu, role: OutputRole, surface: wgpu::Surface<'static>, width: u32, height: u32) -> anyhow::Result<Self> {
        let (config, view_format) = surface_config(gpu, &surface, role.label(), width, height)?;
        // Configured once shown: an unmapped window may refuse a swapchain.
        Ok(Self { role, surface, config, view_format, zero_size: width == 0 || height == 0, shown: false, configured: false })
    }

    pub(super) fn hidden(&self) -> bool {
        self.zero_size || !self.shown
    }

    pub(super) fn configure(&mut self, gpu: &Gpu) -> bool {
        self.configured = configure_surface(gpu, &self.surface, &self.config, self.role.label());
        self.configured
    }

    pub(super) fn run(
        mut self,
        resources: &Resources,
        events: Receiver<OutputEvent>,
        pending: &Pending,
        stats: Arc<Mutex<OutputStats>>,
        stall: Option<Duration>,
    ) {
        let gpu = &resources.gpu;
        let label = self.role.label();
        let mut renderer = Renderer::for_role(self.role);
        let mut painter = Painter::new(&gpu.device, self.view_format);
        let mut dirty = true;
        let mut shown_generation = u64::MAX;
        let mut last_present: Option<Instant> = None;
        let mut window_start = Instant::now();
        let mut window_frames = 0u64;
        let mut window_max_ms = 0f64;
        // Refreshes missed between back-to-back frames (not idle gaps).
        let mut window_dropped = 0u64;
        let mut debug_capture = DebugCapture::from_env();
        let mut reported_loading = false;
        let role = self.role;
        let mut report = |loading: bool| {
            if loading != reported_loading {
                reported_loading = loading;
                if let Some(sink) = resources.status.get() {
                    sink(role, loading);
                }
            }
        };

        loop {
            let now = Instant::now();
            let activity = renderer.activity(now);
            let idle = self.hidden() || !(dirty || activity.redraw || activity.generation != shown_generation);
            let event = if idle {
                // Zero-size outputs park longer; idle ones poll for media readiness.
                let timeout = if self.hidden() { Duration::from_millis(250) } else { IDLE_POLL };
                match events.recv_timeout(timeout) {
                    Ok(event) => Some(event),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            } else {
                events.try_recv().ok()
            };
            for event in event.into_iter().chain(events.try_iter()) {
                match event {
                    OutputEvent::Wake => {}
                    OutputEvent::Shutdown => {
                        log::info!("{label} output stopped");
                        return;
                    }
                }
            }
            if let Some((width, height)) = pending.size.lock().expect("output size poisoned").take() {
                self.zero_size = width == 0 || height == 0;
                if !self.zero_size {
                    self.config.width = width;
                    self.config.height = height;
                    self.configured = false;
                    last_present = None;
                }
                dirty = true;
            }
            if pending.redraw.swap(false, std::sync::atomic::Ordering::AcqRel) {
                dirty = true;
            }
            let shown = pending.shown.load(std::sync::atomic::Ordering::Acquire);
            if shown != self.shown {
                self.shown = shown;
                // A remapped window may have lost its swapchain.
                self.configured = false;
                last_present = None;
                dirty = true;
            }
            if let Some(scene) = pending.scene.lock().expect("output scene poisoned").take() {
                renderer.set_scene(scene, &resources.media, Instant::now());
                dirty = true;
                if let Some(capture) = &mut debug_capture {
                    capture.since = Some(Instant::now());
                }
            }
            if idle {
                // Intervals measure active presentation, not idle gaps.
                last_present = None;
                if !dirty {
                    continue;
                }
            }
            if self.hidden() {
                report(false);
                continue;
            }
            if !self.configured && !self.configure(gpu) {
                std::thread::sleep(IDLE_POLL);
                continue;
            }

            let (texture, suboptimal) = match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
                wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
                wgpu::CurrentSurfaceTexture::Timeout => continue,
                // Returns at once, unlike a timeout; back off instead of spinning.
                wgpu::CurrentSurfaceTexture::Occluded => {
                    std::thread::sleep(IDLE_POLL);
                    continue;
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    self.record_error(&stats);
                    self.configured = false;
                    continue;
                }
                // Recovery would need a new surface; reconfiguring is the best
                // available attempt, so retry it without spinning.
                wgpu::CurrentSurfaceTexture::Lost => {
                    self.record_error(&stats);
                    self.configured = false;
                    std::thread::sleep(IDLE_POLL);
                    continue;
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    self.record_error(&stats);
                    std::thread::sleep(IDLE_POLL);
                    continue;
                }
            };
            let now = Instant::now();
            let drawing = now;
            // The acquired texture's own size: draws never exceed the target,
            // even if the window resized since the last configure.
            let area = crate::fit_aspect((texture.texture.width(), texture.texture.height()), *pending.aspect.lock().expect("output aspect poisoned"));
            let size = (area[2], area[3]);
            let list = renderer.frame(&resources.fonts, size, now);
            let activity = renderer.activity(now);
            report(activity.loading);
            shown_generation = activity.generation;
            dirty = false;
            if let Some(capture) = &mut debug_capture {
                let format = self.view_format;
                capture.after_frame(self.role, activity.settled, || {
                    (crate::capture::paint_to_rgba(gpu, &mut painter, &resources.fonts, &list, size, format), size)
                });
            }
            let view = texture.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(self.view_format), ..Default::default() });
            let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
            painter.paint_in(&gpu.device, &gpu.queue, &resources.fonts, &list, &view, area, &mut encoder);
            gpu.queue.submit(Some(encoder.finish()));
            let submitted = Instant::now();
            crate::profile::draw(role).record(submitted - drawing);
            gpu.queue.on_submitted_work_done(move || crate::profile::gpu(role).record(submitted.elapsed()));
            gpu.queue.present(texture);
            // Reconfigure only after presenting: the acquired texture must be
            // released first. The new swapchain starts empty, so draw again.
            if suboptimal {
                self.configure(gpu);
                dirty = true;
            }

            let now = Instant::now();
            let interval_ms = last_present.map(|t| (now - t).as_secs_f64() * 1000.0).unwrap_or(0.0);
            last_present = Some(now);
            window_frames += 1;
            window_max_ms = window_max_ms.max(interval_ms);
            window_dropped += dropped_refreshes(interval_ms);
            if let Ok(mut s) = stats.lock() {
                s.presented += 1;
                s.max_interval_ms = s.max_interval_ms.max(interval_ms);
            }
            if now - window_start >= Duration::from_secs(5) {
                let fps = window_frames as f64 / (now - window_start).as_secs_f64();
                log::info!("{label}: {fps:.1} fps while active, worst interval {window_max_ms:.1} ms, dropped {window_dropped}");
                window_start = now;
                window_frames = 0;
                window_max_ms = 0.0;
                window_dropped = 0;
            }
            if let Some(stall) = stall {
                std::thread::sleep(stall);
            }
        }
    }

    pub(super) fn record_error(&self, stats: &Mutex<OutputStats>) {
        if let Ok(mut s) = stats.lock() {
            s.surface_errors += 1;
        }
    }
}
