//! Low-frequency still images of what the outputs show, for the operator UI.
//!
//! One background thread keeps its own renderers (separate from the output
//! threads, so a preview never delays presentation) and renders a scene once
//! its transitions settle. Video layers sample the shared textures, so a
//! preview shows the frame current at that moment and opens no decoder.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam::channel::{bounded, Receiver, RecvTimeoutError, Sender};

use crate::capture::paint_to_rgba;
use crate::output::{OutputRole, Renderer, Resources};
use crate::paint::Painter;
use crate::scene::Scene;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Scenes render at output size up to this width, then scale down to the
/// preview width, so pixel-sized borders keep their proportions.
const MAX_RENDER_WIDTH: u32 = 1920;
const PREVIEW_WIDTH: u32 = 640;
/// A scene whose media never loads is still previewed after this long.
const SETTLE_LIMIT: Duration = Duration::from_secs(3);
const BUSY_POLL: Duration = Duration::from_millis(50);
const IDLE_POLL: Duration = Duration::from_secs(1);

/// What a preview shows: an output's current scene, or a style sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreviewTarget {
    Projection,
    Confidence,
    /// A main scene for the output settings dialog, at the projection's size.
    Style,
    /// A confidence scene for the output settings dialog, at the confidence size.
    ConfidenceStyle,
}

impl PreviewTarget {
    pub fn label(self) -> &'static str {
        match self {
            PreviewTarget::Projection => "projection",
            PreviewTarget::Confidence => "confidence",
            PreviewTarget::Style => "style",
            PreviewTarget::ConfidenceStyle => "confidence-style",
        }
    }

    fn role(self) -> OutputRole {
        match self {
            PreviewTarget::Confidence | PreviewTarget::ConfidenceStyle => OutputRole::Confidence,
            PreviewTarget::Projection | PreviewTarget::Style => OutputRole::Projection,
        }
    }
}

/// A JPEG still of one preview target.
pub struct Snapshot {
    pub width: u32,
    pub height: u32,
    pub jpeg: Vec<u8>,
}

pub type SnapshotSink = Box<dyn Fn(PreviewTarget, Snapshot) + Send>;

#[derive(Default)]
struct Pending {
    scenes: HashMap<PreviewTarget, Scene>,
    sizes: HashMap<OutputRole, (u32, u32)>,
}

pub(crate) struct Previews {
    pending: Arc<Mutex<Pending>>,
    wake: Sender<()>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Previews {
    pub fn start(resources: Resources, sizes: HashMap<OutputRole, (u32, u32)>, sink: SnapshotSink) -> anyhow::Result<Self> {
        let pending = Arc::new(Mutex::new(Pending { scenes: HashMap::new(), sizes }));
        let (wake, woken) = bounded(1);
        let running = Arc::new(AtomicBool::new(true));
        let thread = {
            let (pending, running) = (pending.clone(), running.clone());
            std::thread::Builder::new()
                .name("output-previews".into())
                .spawn(move || run(&resources, &pending, &woken, &running, &sink))?
        };
        Ok(Self { pending, wake, running, thread: Some(thread) })
    }

    pub fn set_scene(&self, target: PreviewTarget, scene: Scene) {
        self.pending.lock().expect("preview poisoned").scenes.insert(target, scene);
        let _ = self.wake.try_send(());
    }

    pub fn resize(&self, role: OutputRole, width: u32, height: u32) {
        self.pending.lock().expect("preview poisoned").sizes.insert(role, (width, height));
        let _ = self.wake.try_send(());
    }

    pub fn stop(mut self) {
        self.running.store(false, Ordering::Release);
        let _ = self.wake.try_send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Slot {
    renderer: Renderer,
    has_scene: bool,
    /// Set while a new scene or size waits to be previewed.
    since: Option<Instant>,
    clock: String,
}

fn run(resources: &Resources, pending: &Mutex<Pending>, woken: &Receiver<()>, running: &AtomicBool, sink: &SnapshotSink) {
    let mut painter = Painter::new(&resources.gpu.device, FORMAT);
    let mut slots: HashMap<PreviewTarget, Slot> = HashMap::new();
    let mut sizes: HashMap<OutputRole, (u32, u32)> = HashMap::new();
    while running.load(Ordering::Acquire) {
        let busy = slots.values().any(|slot| slot.since.is_some());
        match woken.recv_timeout(if busy { BUSY_POLL } else { IDLE_POLL }) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if !running.load(Ordering::Acquire) {
            return;
        }
        let now = Instant::now();
        {
            let mut pending = pending.lock().expect("preview poisoned");
            for (role, size) in pending.sizes.drain() {
                if sizes.insert(role, size) != Some(size) {
                    for (target, slot) in &mut slots {
                        if target.role() == role && slot.has_scene {
                            slot.since = Some(now);
                        }
                    }
                }
            }
            for (target, scene) in pending.scenes.drain() {
                let slot = slots.entry(target).or_insert_with(|| Slot {
                    renderer: Renderer::for_role(target.role()),
                    has_scene: false,
                    since: None,
                    clock: String::new(),
                });
                slot.renderer.set_scene(scene, &resources.media, now);
                slot.has_scene = true;
                slot.since = Some(now);
            }
        }
        // The confidence clock is the only thing that changes while idle.
        let clock = chrono::Local::now().format("%H:%M").to_string();
        for (target, slot) in &mut slots {
            if *target == PreviewTarget::Confidence && slot.has_scene && slot.since.is_none() && slot.clock != clock {
                slot.since = Some(now);
            }
        }
        for (target, slot) in &mut slots {
            let Some(since) = slot.since else { continue };
            let Some(&(width, height)) = sizes.get(&target.role()).filter(|(w, h)| *w > 0 && *h > 0) else { continue };
            let size = render_size(width, height);
            // Building the draw list advances loads and layout builds.
            let list = slot.renderer.frame(&resources.fonts, size, now);
            if !slot.renderer.activity(now).settled && since.elapsed() < SETTLE_LIMIT {
                continue;
            }
            slot.since = None;
            slot.clock = clock.clone();
            let pixels = paint_to_rgba(&resources.gpu, &mut painter, &resources.fonts, &list, size, FORMAT);
            match encode(pixels, size) {
                Ok(snapshot) => sink(*target, snapshot),
                Err(e) => log::error!("Encoding the {} preview: {e:#}", target.label()),
            }
        }
    }
}

fn render_size(width: u32, height: u32) -> (u32, u32) {
    if width <= MAX_RENDER_WIDTH {
        return (width, height);
    }
    let height = (u64::from(height) * u64::from(MAX_RENDER_WIDTH) / u64::from(width)).max(1) as u32;
    (MAX_RENDER_WIDTH, height)
}

fn encode(pixels: Vec<u8>, (width, height): (u32, u32)) -> anyhow::Result<Snapshot> {
    let image = image::RgbaImage::from_raw(width, height, pixels).ok_or_else(|| anyhow::anyhow!("preview size mismatch"))?;
    let image = image::DynamicImage::ImageRgba8(image).to_rgb8();
    let image = if width > PREVIEW_WIDTH {
        let scaled_height = (u64::from(height) * u64::from(PREVIEW_WIDTH) / u64::from(width)).max(1) as u32;
        image::imageops::resize(&image, PREVIEW_WIDTH, scaled_height, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85).encode_image(&image)?;
    Ok(Snapshot { width: image.width(), height: image.height(), jpeg })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::media::MediaTextures;
    use crate::scene::MainScene;
    use crate::text::Fonts;
    use crate::Gpu;

    #[test]
    fn previews_each_settled_scene_and_follow_output_resizes() {
        let gpu = Arc::new(Gpu::headless().expect("headless GPU"));
        let resources = Resources { gpu: gpu.clone(), fonts: Fonts::load(), media: MediaTextures::new(gpu), status: Arc::default() };
        let (sent, received) = crossbeam::channel::unbounded();
        let sink: SnapshotSink = Box::new(move |target, snapshot| {
            let _ = sent.send((target, snapshot));
        });
        let sizes = HashMap::from([(OutputRole::Projection, (1920, 1080))]);
        let previews = Previews::start(resources, sizes, sink).expect("start previews");
        let scene = |text: &str| Scene::Main(MainScene { text: text.into(), transition_ms: 250.0, ..Default::default() });

        previews.set_scene(PreviewTarget::Projection, scene("One"));
        previews.set_scene(PreviewTarget::Projection, scene("Two"));
        let (target, snapshot) = received.recv_timeout(Duration::from_secs(5)).expect("a preview");
        assert_eq!((target, snapshot.width, snapshot.height), (PreviewTarget::Projection, 640, 360));
        assert_eq!(&snapshot.jpeg[..2], &[0xFF, 0xD8], "JPEG data");
        assert!(received.recv_timeout(Duration::from_millis(500)).is_err(), "only the latest scene is previewed");

        previews.resize(OutputRole::Projection, 1440, 1080);
        let (_, snapshot) = received.recv_timeout(Duration::from_secs(5)).expect("a preview after resize");
        assert_eq!((snapshot.width, snapshot.height), (640, 480));

        previews.set_scene(PreviewTarget::Style, scene("Sample"));
        assert_eq!(received.recv_timeout(Duration::from_secs(5)).expect("a style preview").0, PreviewTarget::Style);
        previews.stop();
    }

    #[test]
    fn renders_at_output_size_up_to_1080p_width() {
        assert_eq!(render_size(1280, 720), (1280, 720));
        assert_eq!(render_size(3840, 2160), (1920, 1080));
        assert_eq!(render_size(3840, 1), (1920, 1));
    }
}
