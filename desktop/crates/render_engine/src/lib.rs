//! Native output rendering: one GPU device shared by every output surface.
//!
//! Each output presents on its own thread so a slow, occluded or minimized
//! surface cannot stall another output. Video frames upload once into shared
//! textures that every output samples; no output owns a decoder. Outputs draw
//! scenes (see [`scene`]) with their own painter and glyph atlas.

mod capture;
mod confidence;
mod frame;
mod main_scene;
mod media;
mod output;
mod paint;
mod preview;
pub mod scene;
mod shader_bg;
mod test_source;
pub mod text;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::Context;

pub use capture::Capture;
pub use frame::{PixelFrame, SharedFrame, VideoFrames, VideoKey};
pub use main_scene::LyricGeometry;
pub use media::MediaTextures;
pub use output::{OutputRole, OutputStats, StatusSink};
pub use paint::{css_color, Rgba, TextEffects};
pub use preview::{PreviewTarget, Snapshot, SnapshotSink};
pub use scene::{
    ConfidenceGroup, ConfidenceScene, ConfidenceStyle, Direction, Display, Easing, LyricStyle, MainScene, MediaLayer, MediaSource, Scene, Shadow, Stroke,
    Transition, TransitionKind, Vertical,
};
pub use shader_bg::{ShaderSpec, SHADER_SIZE};
pub use test_source::TestPatternSource;
pub use text::{Fonts, TextAlign};

use output::{OutputWorker, Resources};
use preview::Previews;

/// Device and queue shared by every output and frame source.
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    fn open(instance: wgpu::Instance, adapter: wgpu::Adapter) -> anyhow::Result<Self> {
        let info = adapter.get_info();
        log::info!("Native output adapter: {} ({:?}, {:?})", info.name, info.backend, info.device_type);
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Native output device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            },
        ))?;
        device.on_uncaptured_error(Arc::new(|error| log::error!("GPU error: {error}")));
        Ok(Self { instance, adapter, device, queue })
    }

    /// A device without surfaces, for captures and tests.
    pub fn headless() -> anyhow::Result<Self> {
        let instance = new_instance();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .context("No GPU adapter available")?;
        Self::open(instance, adapter)
    }
}

/// Vulkan and DX12 need no instance display handle; each surface brings its
/// window's own (`SurfaceTarget::DisplayAndWindow`).
fn new_instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..wgpu::InstanceDescriptor::new_without_display_handle() })
}

/// A window to present into. The window must outlive the [`RenderEngine`];
/// call [`RenderEngine::shutdown`] before destroying it.
pub struct OutputSpec {
    pub role: OutputRole,
    pub target: Box<dyn wgpu::DisplayAndWindowHandle + 'static>,
    pub width: u32,
    pub height: u32,
    /// Debug-only artificial delay per presented frame, to prove isolation.
    pub stall: Option<std::time::Duration>,
}

/// The largest video frame a texture holds; decoders scale to fit.
pub const FRAME_CAPACITY: (u32, u32) = (1920, 1080);

pub struct RenderEngine {
    gpu: Arc<Gpu>,
    media: Arc<MediaTextures>,
    outputs: HashMap<OutputRole, OutputWorker>,
    scenes: Mutex<HashMap<OutputRole, Scene>>,
    pump: Option<FramePump>,
    resources: Resources,
    sizes: Mutex<HashMap<OutputRole, (u32, u32)>>,
    aspects: Mutex<HashMap<OutputRole, (u32, u32)>>,
    previews: Mutex<Option<Previews>>,
    /// Stills loaded ahead of their cue, kept with the scenes' own media.
    preloads: Mutex<HashSet<String>>,
}

impl RenderEngine {
    /// Creates surfaces on the calling thread, selects one adapter compatible
    /// with all of them and starts one presentation thread per output.
    pub fn start(specs: Vec<OutputSpec>, frames: Arc<dyn VideoFrames>) -> anyhow::Result<Self> {
        anyhow::ensure!(!specs.is_empty(), "At least one output is required");
        let fonts = Fonts::load();
        let instance = new_instance();

        let mut surfaces = Vec::with_capacity(specs.len());
        for spec in specs {
            let surface = instance
                .create_surface(wgpu::SurfaceTarget::DisplayAndWindow(spec.target))
                .with_context(|| format!("Creating {} surface", spec.role.label()))?;
            surfaces.push((spec.role, surface, spec.width, spec.height, spec.stall));
        }

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surfaces[0].1),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .context("No GPU adapter can present to the projection output")?;
        for (role, surface, ..) in &surfaces {
            // Cross-adapter output configurations are not supported yet.
            anyhow::ensure!(
                adapter.is_surface_supported(surface),
                "The {} display is not supported by the selected GPU",
                role.label()
            );
        }
        let gpu = Arc::new(Gpu::open(instance, adapter)?);
        let media = MediaTextures::new(gpu.clone());
        let pump = FramePump::start(gpu.clone(), media.clone(), frames)?;
        let resources = Resources { gpu: gpu.clone(), fonts, media: media.clone(), status: Arc::default() };
        let mut outputs = HashMap::new();
        let mut sizes = HashMap::new();
        for (role, surface, width, height, stall) in surfaces {
            let worker = OutputWorker::spawn(resources.clone(), role, surface, width, height, stall)?;
            outputs.insert(role, worker);
            sizes.insert(role, (width, height));
        }
        Ok(Self {
            gpu,
            media,
            outputs,
            scenes: Mutex::default(),
            pump: Some(pump),
            resources,
            sizes: Mutex::new(sizes),
            aspects: Mutex::default(),
            previews: Mutex::default(),
            preloads: Mutex::default(),
        })
    }

    /// Starts rendering still previews of every output's scene, delivered to
    /// `sink` once each new scene settles.
    pub fn start_previews(&self, sink: SnapshotSink) -> anyhow::Result<()> {
        let aspects = self.aspects.lock().expect("aspects poisoned").clone();
        let sizes = self.sizes.lock().expect("sizes poisoned").iter()
            .map(|(&role, &size)| {
                let [_, _, width, height] = fit_aspect(size, aspects.get(&role).copied());
                (role, (width, height))
            })
            .collect();
        let previews = Previews::start(self.resources.clone(), sizes, sink)?;
        for (role, scene) in self.scenes.lock().expect("scenes poisoned").iter() {
            previews.set_scene(preview_target(*role), scene.clone());
        }
        *self.previews.lock().expect("previews poisoned") = Some(previews);
        Ok(())
    }

    /// Reports when a shown output waits for new media and when it shows it.
    /// Set once.
    pub fn on_status(&self, sink: StatusSink) {
        let _ = self.resources.status.set(sink);
    }

    /// Previews a scene that no output shows: a style sample of the
    /// projection (main scene) or the confidence screen.
    pub fn preview_style(&self, scene: Scene) {
        let target = match scene {
            Scene::Main(_) => PreviewTarget::Style,
            Scene::Confidence(_) => PreviewTarget::ConfidenceStyle,
        };
        if let Some(previews) = self.previews.lock().expect("previews poisoned").as_ref() {
            previews.set_scene(target, scene);
        }
    }

    pub fn gpu(&self) -> &Arc<Gpu> {
        &self.gpu
    }

    /// Replaces what `role` shows. Textures no current scene uses are
    /// released (outputs keep their own until fades end).
    pub fn set_scene(&self, role: OutputRole, scene: Scene) {
        let mut scenes = self.scenes.lock().expect("scenes poisoned");
        scenes.insert(role, scene.clone());
        self.retain_media(&scenes);
        drop(scenes);
        if let Some(previews) = self.previews.lock().expect("previews poisoned").as_ref() {
            previews.set_scene(preview_target(role), scene.clone());
        }
        if let Some(worker) = self.outputs.get(&role) {
            worker.set_scene(scene);
        }
    }

    /// Loads `images` ahead of their cues (the next and previous slides), so
    /// they appear without a decode delay. Replaces the previous set.
    pub fn preload(&self, images: &[String]) {
        let wanted: HashSet<String> = images.iter().cloned().collect();
        let mut preloads = self.preloads.lock().expect("preloads poisoned");
        if *preloads == wanted {
            return;
        }
        for path in &wanted {
            self.media.image(path);
        }
        *preloads = wanted;
        drop(preloads);
        let scenes = self.scenes.lock().expect("scenes poisoned");
        self.retain_media(&scenes);
    }

    /// Keeps the textures the scenes use plus the preloaded stills.
    fn retain_media(&self, scenes: &HashMap<OutputRole, Scene>) {
        let (mut videos, mut images) = (HashSet::new(), self.preloads.lock().expect("preloads poisoned").clone());
        for scene in scenes.values() {
            for source in scene_sources(scene) {
                match source {
                    MediaSource::Video(key) => videos.insert(key),
                    MediaSource::Image(path) => images.insert(path),
                };
            }
        }
        self.media.retain(&videos, &images);
    }

    pub fn resize(&self, role: OutputRole, width: u32, height: u32) {
        self.sizes.lock().expect("sizes poisoned").insert(role, (width, height));
        self.resize_preview(role);
        if let Some(worker) = self.outputs.get(&role) {
            worker.resize(width, height);
        }
    }

    /// Letterboxes `role`'s content to `aspect` (width, height); `None` fills
    /// the window.
    pub fn set_aspect(&self, role: OutputRole, aspect: Option<(u32, u32)>) {
        let mut aspects = self.aspects.lock().expect("aspects poisoned");
        match aspect {
            Some(aspect) => aspects.insert(role, aspect),
            None => aspects.remove(&role),
        };
        drop(aspects);
        self.resize_preview(role);
        if let Some(worker) = self.outputs.get(&role) {
            worker.set_aspect(aspect);
        }
    }

    /// Shows or parks `role`'s presentation, following its window.
    pub fn set_shown(&self, role: OutputRole, shown: bool) {
        if let Some(worker) = self.outputs.get(&role) {
            worker.set_shown(shown);
        }
    }

    /// Previews render the letterboxed content only.
    fn resize_preview(&self, role: OutputRole) {
        let Some(&size) = self.sizes.lock().expect("sizes poisoned").get(&role) else { return };
        let aspect = self.aspects.lock().expect("aspects poisoned").get(&role).copied();
        let [_, _, width, height] = fit_aspect(size, aspect);
        if let Some(previews) = self.previews.lock().expect("previews poisoned").as_ref() {
            previews.resize(role, width, height);
        }
    }

    /// Presents `role` again, for windows whose contents the system discarded.
    pub fn redraw(&self, role: OutputRole) {
        if let Some(worker) = self.outputs.get(&role) {
            worker.redraw();
        }
    }

    pub fn stats(&self, role: OutputRole) -> Option<OutputStats> {
        self.outputs.get(&role).map(OutputWorker::stats)
    }

    /// Stops the frame pump and joins every output thread, releasing all
    /// surfaces. Must run before the output windows are destroyed.
    pub fn shutdown(&mut self) {
        if let Some(previews) = self.previews.lock().ok().and_then(|mut p| p.take()) {
            previews.stop();
        }
        if let Some(pump) = self.pump.take() {
            pump.stop();
        }
        // Signal every output before joining so their exits overlap.
        for worker in self.outputs.values() {
            worker.signal_stop();
        }
        for (_, worker) in self.outputs.drain() {
            worker.join();
        }
    }
}

impl Drop for RenderEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The centered (x, y, width, height) area of `size` with the `aspect`
/// ratio; the whole of `size` without one.
pub fn fit_aspect(size: (u32, u32), aspect: Option<(u32, u32)>) -> [u32; 4] {
    let (width, height) = size;
    let Some((aw, ah)) = aspect.filter(|&(w, h)| w > 0 && h > 0 && width > 0 && height > 0) else {
        return [0, 0, width, height];
    };
    let (fit_w, fit_h) = if width as u64 * ah as u64 > height as u64 * aw as u64 {
        ((height as u64 * aw as u64 / ah as u64) as u32, height)
    } else {
        (width, (width as u64 * ah as u64 / aw as u64) as u32)
    };
    let (fit_w, fit_h) = (fit_w.max(1), fit_h.max(1));
    [(width - fit_w) / 2, (height - fit_h) / 2, fit_w, fit_h]
}

fn preview_target(role: OutputRole) -> PreviewTarget {
    match role {
        OutputRole::Projection => PreviewTarget::Projection,
        OutputRole::Confidence => PreviewTarget::Confidence,
    }
}

/// Media a scene draws.
fn scene_sources(scene: &Scene) -> Vec<MediaSource> {
    match scene {
        Scene::Main(main) => main
            .background
            .iter()
            .map(|layer| layer.source.clone())
            .chain(main.logo.clone().filter(|_| main.display == Display::Logo))
            .collect(),
        Scene::Confidence(confidence) => confidence.video.iter().map(|key| MediaSource::Video(key.clone())).collect(),
    }
}

/// Polls the frame source for every video texture a scene uses and uploads
/// each new frame once for all outputs.
struct FramePump {
    running: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Polling interval: well under one 60 Hz refresh, so a due frame is never
/// more than ~4 ms late to the texture.
const PUMP_INTERVAL: std::time::Duration = std::time::Duration::from_millis(4);

impl FramePump {
    fn start(gpu: Arc<Gpu>, media: Arc<MediaTextures>, frames: Arc<dyn VideoFrames>) -> anyhow::Result<Self> {
        use std::sync::atomic::{AtomicBool, Ordering};
        let running = Arc::new(AtomicBool::new(true));
        let thread = {
            let running = running.clone();
            std::thread::Builder::new().name("frame-pump".into()).spawn(move || {
                let mut shaders = shader_bg::ShaderBackgrounds::new(&gpu.device);
                while running.load(Ordering::Acquire) {
                    let mut uploaded = false;
                    let videos = media.videos();
                    let now = std::time::Instant::now();
                    for (key, texture) in &videos {
                        match key {
                            // Rendered here rather than decoded; submits its own work.
                            VideoKey::Shader(spec) => {
                                shaders.tick(&gpu, spec, texture, now);
                            }
                            // A stopped source keeps its last frame for fading layers.
                            _ => {
                                if let Some(next) = frames.frame(key).filter(|f| f.id() != texture.last_id()) {
                                    uploaded |= texture.upload(&gpu.queue, next.as_ref());
                                }
                            }
                        }
                    }
                    shaders.retain(&videos.iter().filter_map(|(key, _)| match key { VideoKey::Shader(spec) => Some(spec), _ => None }).collect::<Vec<_>>());
                    // Runs completion callbacks (the shader budget) without waiting.
                    let _ = gpu.device.poll(wgpu::PollType::Poll);
                    if uploaded {
                        // Writes are applied on the next submit; without one, an
                        // idle output would never see the frame.
                        gpu.queue.submit(None);
                    }
                    std::thread::sleep(PUMP_INTERVAL);
                }
            })?
        };
        Ok(Self { running, thread: Some(thread) })
    }

    fn stop(mut self) {
        self.running.store(false, std::sync::atomic::Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod fit_tests {
    use super::fit_aspect;

    #[test]
    fn letterboxes_and_pillarboxes_to_the_aspect() {
        assert_eq!(fit_aspect((1920, 1080), None), [0, 0, 1920, 1080]);
        assert_eq!(fit_aspect((1920, 1080), Some((4, 3))), [240, 0, 1440, 1080]);
        assert_eq!(fit_aspect((1024, 768), Some((16, 9))), [0, 96, 1024, 576]);
        assert_eq!(fit_aspect((0, 0), Some((16, 9))), [0, 0, 0, 0]);
    }
}
