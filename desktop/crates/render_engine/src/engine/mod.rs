use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::Context;

use crate::*;

mod scenes;
mod windows;

pub use windows::fit_aspect;
use windows::*;

pub struct OutputSpec {
    pub role: OutputRole,
    pub target: Box<dyn wgpu::DisplayAndWindowHandle + 'static>,
    pub width: u32,
    pub height: u32,
    pub stall: Option<std::time::Duration>,
}

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
    monitors: Mutex<HashMap<OutputRole, MonitorWorker>>,
    preloads: Mutex<HashSet<String>>,
}

impl RenderEngine {
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
            monitors: Mutex::default(),
            preloads: Mutex::default(),
        })
    }
}

impl RenderEngine {
    pub fn shutdown(&mut self) {
        if let Some(previews) = self.previews.lock().ok().and_then(|mut p| p.take()) {
            previews.stop();
        }
        let monitors: Vec<_> = self.monitors.lock().map(|mut m| m.drain().map(|(_, w)| w).collect()).unwrap_or_default();
        for monitor in &monitors {
            monitor.signal_stop();
        }
        for monitor in monitors {
            monitor.join();
        }
        if let Some(pump) = self.pump.take() {
            pump.stop();
        }
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
