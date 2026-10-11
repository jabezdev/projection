use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam::channel::{bounded, Receiver, RecvTimeoutError, Sender, TrySendError};

use crate::confidence::ConfidenceRenderer;
use crate::main_scene::MainRenderer;
use crate::media::MediaTextures;
use crate::paint::{DrawList, Painter};
use crate::scene::{Activity, Scene};
use crate::text::Fonts;
use crate::Gpu;

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
}

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

impl OutputWorker {
    pub fn spawn(
        resources: Resources,
        role: OutputRole,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        stall: Option<Duration>,
    ) -> anyhow::Result<Self> {
        // Bounded: a stalled worker never makes the UI thread block.
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

    fn wake(&self) {
        match self.events.try_send(OutputEvent::Wake) {
            // A full queue already holds a wake for the stored values.
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    pub fn stats(&self) -> OutputStats {
        self.stats.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Asks the worker to exit. A worker blocked acquiring a frame from a
    /// hidden surface notices after the acquire times out.
    pub fn signal_stop(&self) {
        // Shutdown must not be dropped by a full queue.
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

/// The scene an output draws: the projection shows the main scene, the
/// confidence output the teleprompter.
pub(crate) enum Renderer {
    Main(MainRenderer),
    Confidence(ConfidenceRenderer),
}

impl Renderer {
    pub fn for_role(role: OutputRole) -> Self {
        match role {
            OutputRole::Projection => Renderer::Main(MainRenderer::default()),
            OutputRole::Confidence => Renderer::Confidence(ConfidenceRenderer::default()),
        }
    }

    pub fn set_scene(&mut self, scene: Scene, media: &MediaTextures, now: Instant) {
        match (self, scene) {
            (Renderer::Main(r), Scene::Main(s)) => r.set_scene(s, media, now),
            (Renderer::Confidence(r), Scene::Confidence(s)) => r.set_scene(s, media, now),
            _ => log::warn!("Scene does not match the output's role"),
        }
    }

    pub fn activity(&self, now: Instant) -> Activity {
        match self {
            Renderer::Main(r) => r.activity(now),
            Renderer::Confidence(r) => r.activity(now),
        }
    }

    pub fn frame(&mut self, fonts: &Arc<Fonts>, size: (u32, u32), now: Instant) -> DrawList {
        match self {
            Renderer::Main(r) => r.frame(fonts, size, now),
            Renderer::Confidence(r) => r.frame(fonts, size, now),
        }
    }
}

/// Idle outputs check for pending media and clock changes this often.
const IDLE_POLL: Duration = Duration::from_millis(16);

/// Debug aid (`MAGI_OUTPUT_CAPTURE=<dir>`): saves each scene as presented,
/// once its transitions settle, so the real output can be inspected even
/// when its window is covered.
struct DebugCapture {
    dir: std::path::PathBuf,
    since: Option<Instant>,
    count: u32,
}

impl DebugCapture {
    fn from_env() -> Option<Self> {
        let dir = std::path::PathBuf::from(std::env::var_os("MAGI_OUTPUT_CAPTURE")?);
        std::fs::create_dir_all(&dir).ok()?;
        Some(Self { dir, since: None, count: 0 })
    }

    /// Saves once transitions end and media is shown (or after 5 s regardless).
    fn after_frame(&mut self, role: OutputRole, settled: bool, save: impl FnOnce() -> (Vec<u8>, (u32, u32))) {
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

struct Presenter {
    role: OutputRole,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// Format of the views drawn into: non-sRGB, so blending happens on
    /// sRGB-encoded values as in browsers.
    view_format: wgpu::TextureFormat,
    /// True while the window has zero size (minimized or not yet mapped).
    zero_size: bool,
    /// True while the window is shown; starts hidden.
    shown: bool,
    /// True once the surface matches `config`. A failed configure is retried
    /// before the next frame.
    configured: bool,
}

impl Presenter {
    fn new(gpu: &Gpu, role: OutputRole, surface: wgpu::Surface<'static>, width: u32, height: u32) -> anyhow::Result<Self> {
        let caps = surface.get_capabilities(&gpu.adapter);
        anyhow::ensure!(!caps.formats.is_empty(), "{} surface has no supported formats", role.label());
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let view_format = format.remove_srgb_suffix();
        let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
            wgpu::CompositeAlphaMode::Opaque
        } else {
            caps.alpha_modes[0]
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: if view_format != format { vec![view_format] } else { vec![] },
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        log::info!("{} surface: {:?} {}x{}", role.label(), format, width, height);
        // Configured once shown: an unmapped window may refuse a swapchain.
        Ok(Self { role, surface, config, view_format, zero_size: width == 0 || height == 0, shown: false, configured: false })
    }

    /// Applies `config`, retrying briefly: wgpu refuses a configure while
    /// another thread submits work (the frame pump, the other output or the
    /// previews), reporting it only as an error. Returns whether it applied.
    fn hidden(&self) -> bool {
        self.zero_size || !self.shown
    }

    fn configure(&mut self, gpu: &Gpu) -> bool {
        const ATTEMPTS: u32 = 5;
        for attempt in 1..=ATTEMPTS {
            let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
            self.surface.configure(&gpu.device, &self.config);
            match pollster::block_on(scope.pop()) {
                None => {
                    self.configured = true;
                    return true;
                }
                Some(error) if attempt == ATTEMPTS => {
                    log::warn!("{} surface configure failed; retrying next frame: {error}", self.role.label());
                }
                Some(_) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        self.configured = false;
        false
    }

    fn run(
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
        let mut debug_capture = DebugCapture::from_env();

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
            if idle && !dirty {
                // Intervals measure active presentation, not idle gaps.
                last_present = None;
                continue;
            }
            if self.hidden() {
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
            // The acquired texture's own size: draws never exceed the target,
            // even if the window resized since the last configure.
            let area = crate::fit_aspect((texture.texture.width(), texture.texture.height()), *pending.aspect.lock().expect("output aspect poisoned"));
            let size = (area[2], area[3]);
            let list = renderer.frame(&resources.fonts, size, now);
            let activity = renderer.activity(now);
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
            if let Ok(mut s) = stats.lock() {
                s.presented += 1;
                s.max_interval_ms = s.max_interval_ms.max(interval_ms);
            }
            if now - window_start >= Duration::from_secs(5) {
                let fps = window_frames as f64 / (now - window_start).as_secs_f64();
                log::info!("{label}: {fps:.1} fps while active, worst interval {window_max_ms:.1} ms");
                window_start = now;
                window_frames = 0;
                window_max_ms = 0.0;
            }
            if let Some(stall) = stall {
                std::thread::sleep(stall);
            }
        }
    }

    fn record_error(&self, stats: &Mutex<OutputStats>) {
        if let Ok(mut s) = stats.lock() {
            s.surface_errors += 1;
        }
    }
}
