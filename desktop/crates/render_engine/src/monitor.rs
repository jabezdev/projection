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
use std::time::{Duration, Instant};

use crossbeam::channel::{bounded, Receiver, RecvTimeoutError, Sender};

use crate::output::{configure_surface, surface_config, OutputRole, Renderer, Resources};
use crate::paint::Painter;
use crate::scene::Scene;
use crate::Gpu;

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

struct Monitor {
    role: OutputRole,
    label: String,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    view_format: wgpu::TextureFormat,
    configured: bool,
    scaler: Scaler,
}

impl Monitor {
    fn new(gpu: &Gpu, role: OutputRole, label: String, surface: wgpu::Surface<'static>, config: wgpu::SurfaceConfiguration, view_format: wgpu::TextureFormat) -> Self {
        let scaler = Scaler::new(&gpu.device, view_format);
        Self { role, label, surface, config, view_format, configured: false, scaler }
    }

    fn run(mut self, resources: &Resources, pending: &Pending, woken: &Receiver<()>) {
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
                    // A remapped window may have lost its swapchain.
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

/// The output's content size, capped to [`MAX_RENDER_WIDTH`].
fn layout_size((width, height): (u32, u32)) -> (u32, u32) {
    let (width, height) = (width.max(1), height.max(1));
    if width <= MAX_RENDER_WIDTH {
        return (width, height);
    }
    (MAX_RENDER_WIDTH, (u64::from(height) * u64::from(MAX_RENDER_WIDTH) / u64::from(width)).max(1) as u32)
}

/// Taps per axis for a `scale`:1 reduction; each bilinear tap covers 2×2.
fn taps(scale: f32) -> u32 {
    ((scale / 2.0).ceil() as u32).clamp(1, 4)
}

/// The offscreen frame at layout size and the pass that scales it down.
struct Scaler {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    sampler: wgpu::Sampler,
    source: Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
}

impl Scaler {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Monitor scale"),
            source: wgpu::ShaderSource::Wgsl(include_str!("monitor.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Monitor scale"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Monitor scale"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Monitor scale"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Monitor scale params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Monitor scale"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { pipeline, layout, params, sampler, source: None }
    }

    /// The offscreen view to paint into, recreated when the size changes.
    fn source(&mut self, device: &wgpu::Device, (width, height): (u32, u32)) -> wgpu::TextureView {
        let current = self.source.as_ref().is_some_and(|(texture, ..)| (texture.width(), texture.height()) == (width, height));
        if !current {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Monitor frame"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Monitor scale"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.params.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });
            self.source = Some((texture, view, group));
        }
        self.source.as_ref().expect("monitor source").1.clone()
    }

    /// Scales the source into `area` of `target`; the rest is black.
    fn draw(&self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, area: [u32; 4]) {
        let Some((texture, _, group)) = &self.source else { return };
        let (w, h) = (area[2].max(1) as f32, area[3].max(1) as f32);
        let scale = (texture.width() as f32 / w).max(texture.height() as f32 / h);
        gpu.queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&[1.0 / w, 1.0 / h, taps(scale) as f32, 0.0]));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Monitor scale"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        if area[2] == 0 || area[3] == 0 {
            return;
        }
        pass.set_viewport(area[0] as f32, area[1] as f32, w, h, 0.0, 1.0);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_at_output_size_up_to_1080p_width() {
        assert_eq!(layout_size((1280, 720)), (1280, 720));
        assert_eq!(layout_size((3840, 2160)), (1920, 1080));
        assert_eq!(layout_size((0, 0)), (1, 1));
    }

    #[test]
    fn scales_a_full_size_frame_into_a_small_surface() {
        let gpu = Gpu::headless().expect("headless GPU");
        let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut scaler = Scaler::new(&gpu.device, FORMAT);
        let source = scaler.source(&gpu.device, (1920, 1080));
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: 480, height: 300, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            // A white source frame.
            let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &source,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
        }
        scaler.draw(&gpu, &mut encoder, &view, crate::fit_aspect((480, 300), Some((1920, 1080))));
        gpu.queue.submit(Some(encoder.finish()));
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        assert!(pollster::block_on(scope.pop()).is_none(), "the scale pass is valid");
        // A second size reuses nothing stale.
        let _ = scaler.source(&gpu.device, (1280, 720));
        assert_eq!(scaler.source.as_ref().map(|(t, ..)| (t.width(), t.height())), Some((1280, 720)));
    }

    #[test]
    fn uses_more_taps_for_larger_reductions() {
        assert_eq!(taps(1.0), 1);
        assert_eq!(taps(2.0), 1);
        assert_eq!(taps(4.0), 2);
        assert_eq!(taps(6.0), 3);
        assert_eq!(taps(20.0), 4);
    }
}
