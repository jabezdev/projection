use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::paint::GpuTexture;
use crate::yuv::{YuvLayout, YuvPixels, YuvPlanes};
use crate::Gpu;

/// A frame's pixels as decoded.
pub enum FramePixels<'a> {
    /// 8-bit RGBA; `stride` (bytes per row) may exceed `width * 4`.
    Rgba { data: &'a [u8], stride: usize },
    /// Converted on the GPU; may be larger than the texture (it is scaled to fit).
    Yuv(YuvPixels<'a>),
    /// 8-bit BGRA or BGRx (screen captures); swizzled and scaled on the GPU.
    Bgra { data: &'a [u8], stride: usize },
}

/// A decoded frame supplied by [`VideoFrames`].
pub trait PixelFrame: Send + Sync {
    /// Unique per decoded frame; an unchanged id is not uploaded again.
    fn id(&self) -> u64;
    fn size(&self) -> (u32, u32);
    fn pixels(&self) -> FramePixels<'_>;
}

/// Which playing video a texture follows. A foreground session is addressed
/// by id so a new cue never shows the previous session's last frame; muted
/// ambient loops (song backgrounds, logos) are addressed by file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VideoKey {
    Session(u64),
    Ambient(String),
    /// A built-in animated background, rendered on the GPU rather than
    /// decoded: `shader_bg::ShaderSpec` encoded as a string.
    Shader(String),
    /// A live screen or window capture, by its source id.
    Capture(String),
}

/// Supplies the frame due now for each video. Polled by one pump thread,
/// never by outputs.
pub trait VideoFrames: Send + Sync + 'static {
    fn frame(&self, key: &VideoKey) -> Option<Arc<dyn PixelFrame>>;
}

/// One GPU texture written by the pump and sampled by every output. Frames up
/// to the texture size are uploaded into its top-left corner, so changing
/// video dimensions never recreates the texture or the outputs' bindings.
/// The last frame stays after its source stops, so outgoing layers can fade.
pub struct SharedFrame {
    pub(crate) texture: wgpu::Texture,
    gpu_texture: Arc<GpuTexture>,
    capacity: (u32, u32),
    /// Current content size packed as `width << 32 | height`; 0 when empty.
    content: AtomicU64,
    /// Incremented after each upload; outputs redraw when it changes.
    generation: AtomicU64,
    /// Id of the last uploaded frame.
    last_id: AtomicU64,
    /// Plane textures for YUV frames, written only by the frame pump.
    yuv: std::sync::Mutex<Option<YuvPlanes>>,
}

impl SharedFrame {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shared frame"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // sRGB values pass through unconverted; outputs blend in sRGB.
            format: wgpu::TextureFormat::Rgba8Unorm,
            // Rendered into by shader backgrounds; read back in tests.
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            gpu_texture: Arc::new(GpuTexture { id: GpuTexture::next_id(), view, owner: None }),
            capacity: (width, height),
            content: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            last_id: AtomicU64::new(0),
            yuv: std::sync::Mutex::new(None),
        }
    }

    pub fn capacity(&self) -> (u32, u32) {
        self.capacity
    }

    pub fn texture(&self) -> &Arc<GpuTexture> {
        &self.gpu_texture
    }

    /// Size of the frame currently in the texture, or `None` before the first.
    pub fn size(&self) -> Option<(u32, u32)> {
        match self.content.load(Ordering::Acquire) {
            0 => None,
            packed => Some(((packed >> 32) as u32, packed as u32)),
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub(crate) fn last_id(&self) -> u64 {
        self.last_id.load(Ordering::Acquire)
    }

    /// Texture coordinates of the content within the larger texture.
    pub fn uv(&self) -> [f32; 4] {
        let (w, h) = self.size().unwrap_or((0, 0));
        [0.0, 0.0, w as f32 / self.capacity.0 as f32, h as f32 / self.capacity.1 as f32]
    }

    /// Records that `size` pixels in the top-left corner were rendered on the GPU.
    pub(crate) fn rendered(&self, size: (u32, u32)) {
        self.content.store((size.0 as u64) << 32 | size.1 as u64, Ordering::Release);
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Uploads `frame` into the top-left corner. RGBA frames larger than the
    /// texture are rejected (decoders scale them to fit); YUV frames are
    /// converted on the GPU and scaled down to fit.
    pub fn upload(&self, gpu: &Gpu, frame: &dyn PixelFrame) -> bool {
        let (width, height) = frame.size();
        self.last_id.store(frame.id(), Ordering::Release);
        let shown = match frame.pixels() {
            FramePixels::Rgba { data, stride } => self.upload_rgba(&gpu.queue, (width, height), data, stride),
            FramePixels::Bgra { data, stride } => {
                let pixels = YuvPixels { layout: YuvLayout::Bgra, planes: [(data, stride), (&[][..], 0), (&[][..], 0)], matrix: crate::YuvMatrix::Bt709, full_range: true };
                let out = fit_within((width, height), self.capacity);
                let converted = width > 0
                    && height > 0
                    && YuvPlanes::convert(&mut self.yuv.lock().expect("yuv planes poisoned"), gpu, &pixels, (width, height), &self.texture, out);
                converted.then_some(out)
            }
            FramePixels::Yuv(pixels) => {
                let out = fit_within((width, height), self.capacity);
                let converted = width > 0
                    && height > 0
                    && YuvPlanes::convert(&mut self.yuv.lock().expect("yuv planes poisoned"), gpu, &pixels, (width, height), &self.texture, out);
                converted.then_some(out)
            }
        };
        let Some((width, height)) = shown else {
            log::error!("Rejected a {width}x{height} frame with missing pixels");
            return false;
        };
        self.content.store((width as u64) << 32 | height as u64, Ordering::Release);
        self.generation.fetch_add(1, Ordering::AcqRel);
        true
    }

    fn upload_rgba(&self, queue: &wgpu::Queue, (width, height): (u32, u32), pixels: &[u8], stride: usize) -> Option<(u32, u32)> {
        let fits = width > 0
            && height > 0
            && width <= self.capacity.0
            && height <= self.capacity.1
            && stride >= width as usize * 4
            && pixels.len() >= stride * (height as usize - 1) + width as usize * 4;
        if !fits {
            return None;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride as u32), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        Some((width, height))
    }
}

/// Largest size within `max` with the same aspect ratio, never scaled up.
fn fit_within((width, height): (u32, u32), max: (u32, u32)) -> (u32, u32) {
    let scale = (max.0 as f64 / width.max(1) as f64).min(max.1 as f64 / height.max(1) as f64).min(1.0);
    (((width as f64 * scale).round() as u32).clamp(1, max.0), ((height as f64 * scale).round() as u32).clamp(1, max.1))
}
