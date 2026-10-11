//! Offscreen rendering of a scene, for tests and the parity check. Uses the
//! same renderers and painter as the windows.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::media::MediaTextures;
use crate::output::{OutputRole, Renderer, Resources};
use crate::paint::{DrawList, Painter};
use crate::scene::Scene;
use crate::text::Fonts;
use crate::Gpu;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub struct Capture {
    resources: Resources,
    renderer: Renderer,
    painter: Painter,
}

impl Capture {
    pub fn new(role: OutputRole) -> anyhow::Result<Self> {
        let gpu = Arc::new(Gpu::headless()?);
        let resources = Resources { gpu: gpu.clone(), fonts: Fonts::load(), media: MediaTextures::new(gpu.clone()) };
        let painter = Painter::new(&gpu.device, FORMAT);
        Ok(Self { resources, renderer: Renderer::for_role(role), painter })
    }

    pub fn gpu(&self) -> &Arc<Gpu> {
        &self.resources.gpu
    }

    pub fn media(&self) -> &Arc<MediaTextures> {
        &self.resources.media
    }

    pub fn fonts(&self) -> &Arc<Fonts> {
        &self.resources.fonts
    }

    pub fn set_scene(&mut self, scene: Scene, now: Instant) {
        self.renderer.set_scene(scene, &self.resources.media, now);
    }

    /// Whether a transition or loading media is still in progress at `now`.
    pub fn busy(&self, now: Instant) -> bool {
        !self.renderer.activity(now).settled
    }

    /// Advances time from `start` until fades end and media has loaded (up to
    /// `limit` of scene time), then renders. Returns the frame and its time.
    pub fn settle(&mut self, size: (u32, u32), start: Instant, limit: Duration) -> (Vec<u8>, Instant) {
        let mut now = start;
        loop {
            self.render(size, now);
            if !self.busy(now) || now - start >= limit {
                return (self.render(size, now), now);
            }
            // Let image loads progress in real time while scene time advances.
            std::thread::sleep(Duration::from_millis(5));
            now += Duration::from_millis(50);
        }
    }

    /// Renders the scene at `now` and reads it back as tightly packed RGBA.
    pub fn render(&mut self, size: (u32, u32), now: Instant) -> Vec<u8> {
        let list = self.renderer.frame(&self.resources.fonts, size, now);
        paint_to_rgba(&self.resources.gpu, &mut self.painter, &self.resources.fonts, &list, size, FORMAT)
    }
}

/// Paints `list` offscreen in `format` (the painter's) and reads it back as
/// tightly packed RGBA.
pub(crate) fn paint_to_rgba(
    gpu: &Gpu,
    painter: &mut Painter,
    fonts: &Fonts,
    list: &DrawList,
    size: (u32, u32),
    format: wgpu::TextureFormat,
) -> Vec<u8> {
    {
        let (width, height) = size;
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Capture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Capture") });
        painter.paint(&gpu.device, &gpu.queue, fonts, list, &view, size, &mut encoder);
        let row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Capture readback"),
            size: (row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        gpu.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        let mapped = slice.get_mapped_range().expect("capture readback is mapped");
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height as usize {
            let start = y * row as usize;
            pixels.extend_from_slice(&mapped[start..start + width as usize * 4]);
        }
        drop(mapped);
        buffer.unmap();
        if matches!(format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        pixels
    }
}

impl OutputRole {
    /// The role whose renderer draws `scene`.
    pub fn for_scene(scene: &Scene) -> Self {
        match scene {
            Scene::Main(_) => OutputRole::Projection,
            Scene::Confidence(_) => OutputRole::Confidence,
        }
    }
}
