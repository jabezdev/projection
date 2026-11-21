use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crossbeam::channel::{Receiver, RecvTimeoutError};

use crate::output::{configure_surface, OutputRole, Renderer, Resources};
use crate::paint::Painter;
use crate::Gpu;

use super::*;

pub(super) struct Monitor {
    pub(super) role: OutputRole,
    pub(super) label: String,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) view_format: wgpu::TextureFormat,
    pub(super) configured: bool,
    pub(super) scaler: Scaler,
}

impl Monitor {
    pub(super) fn new(gpu: &Gpu, role: OutputRole, label: String, surface: wgpu::Surface<'static>, config: wgpu::SurfaceConfiguration, view_format: wgpu::TextureFormat) -> Self {
        let scaler = Scaler::new(&gpu.device, view_format);
        Self { role, label, surface, config, view_format, configured: false, scaler }
    }

    pub(super) fn run(mut self, resources: &Resources, pending: &Pending, woken: &Receiver<()>) {
        let gpu = &resources.gpu;
        let mut renderer = Renderer::for_role(self.role);
        let mut painter = Painter::new(&gpu.device, FORMAT);
        let mut content = (1920, 1080);
        let mut shown: Option<(u32, u32)> = None;
        let mut dirty = true;
        let mut shown_generation = u64::MAX;
        let mut last_frame: Option<Instant> = None;
        loop {
            let now = Instant::now();
            let activity = renderer.activity(now);
            let wants_frame = shown.is_some() && (dirty || activity.redraw || activity.generation != shown_generation);
            let timeout = match (shown, wants_frame, last_frame) {
                (None, ..) => HIDDEN_POLL,
                (_, true, Some(last)) => FRAME_INTERVAL.saturating_sub(last.elapsed()),
                (_, true, None) => Duration::ZERO,
                (_, false, _) => IDLE_POLL,
            };
            if !timeout.is_zero() {
                match woken.recv_timeout(timeout) {
                    Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            if pending.stop.load(Ordering::Acquire) {
                log::info!("{} stopped", self.label);
                return;
            }
            if let Some(size) = pending.content.lock().expect("monitor poisoned").take() {
                content = size;
                dirty = true;
            }
            if let Some(size) = pending.surface.lock().expect("monitor poisoned").take() {
                if size != shown {
                    if let Some((width, height)) = size {
                        self.config.width = width;
                        self.config.height = height;
                    }
                    self.configured = false;
                    shown = size;
                    dirty = true;
                }
            }
            if let Some(scene) = pending.scene.lock().expect("monitor poisoned").take() {
                renderer.set_scene(scene, &resources.media, Instant::now());
                dirty = true;
            }
            if shown.is_none() {
                continue;
            }
            let now = Instant::now();
            let activity = renderer.activity(now);
            if !(dirty || activity.redraw || activity.generation != shown_generation) {
                continue;
            }
            if last_frame.is_some_and(|last| last.elapsed() < FRAME_INTERVAL) {
                continue;
            }
            if !self.configured {
                self.configured = configure_surface(gpu, &self.surface, &self.config, &self.label);
                if !self.configured {
                    continue;
                }
            }
            let texture = match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => continue,
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Validation => {
                    self.configured = false;
                    continue;
                }
            };
            let layout = layout_size(content);
            let list = renderer.frame(&resources.fonts, layout, now);
            shown_generation = renderer.activity(now).generation;
            dirty = false;
            last_frame = Some(now);

            let target = (texture.texture.width(), texture.texture.height());
            let area = crate::fit_aspect(target, Some(layout));
            let source = self.scaler.source(&gpu.device, layout);
            let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(&self.label) });
            painter.paint(&gpu.device, &gpu.queue, &resources.fonts, &list, &source, layout, &mut encoder);
            let view = texture.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(self.view_format), ..Default::default() });
            self.scaler.draw(gpu, &mut encoder, &view, area);
            gpu.queue.submit(Some(encoder.finish()));
            gpu.queue.present(texture);
        }
    }
}

pub(super) fn layout_size((width, height): (u32, u32)) -> (u32, u32) {
    let (width, height) = (width.max(1), height.max(1));
    if width <= MAX_RENDER_WIDTH {
        return (width, height);
    }
    (MAX_RENDER_WIDTH, (u64::from(height) * u64::from(MAX_RENDER_WIDTH) / u64::from(width)).max(1) as u32)
}

pub(super) fn taps(scale: f32) -> u32 {
    ((scale / 2.0).ceil() as u32).clamp(1, 4)
}
