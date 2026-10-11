//! GPU textures for everything a scene can show: videos (one texture per
//! source, written by the frame pump) and still images (decoded off the
//! output threads). Shared by all outputs.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crossbeam::channel::{unbounded, Sender};

use crate::frame::{SharedFrame, VideoKey};
use crate::paint::GpuTexture;
use crate::{Gpu, FRAME_CAPACITY};

/// Stills larger than this are scaled down on load; outputs are 1080p and
/// presentations are shown full screen.
const MAX_IMAGE: u32 = 3840;

pub enum ImageState {
    Loading,
    Ready { texture: Arc<GpuTexture>, size: (u32, u32) },
    Failed,
}

pub struct ImageSlot {
    state: Mutex<ImageState>,
}

impl ImageSlot {
    /// The texture and its size once loaded.
    pub fn ready(&self) -> Option<(Arc<GpuTexture>, (u32, u32))> {
        match &*self.state.lock().expect("image slot poisoned") {
            ImageState::Ready { texture, size } => Some((texture.clone(), *size)),
            _ => None,
        }
    }

    pub fn failed(&self) -> bool {
        matches!(&*self.state.lock().expect("image slot poisoned"), ImageState::Failed)
    }
}

pub struct MediaTextures {
    gpu: Arc<Gpu>,
    videos: Mutex<HashMap<VideoKey, Arc<SharedFrame>>>,
    images: Mutex<HashMap<String, Arc<ImageSlot>>>,
    loads: Sender<(String, Arc<ImageSlot>)>,
}

impl MediaTextures {
    pub fn new(gpu: Arc<Gpu>) -> Arc<Self> {
        let (loads, requests) = unbounded::<(String, Arc<ImageSlot>)>();
        let loader_gpu = gpu.clone();
        std::thread::Builder::new()
            .name("image-loader".into())
            .spawn(move || {
                for (path, slot) in requests {
                    let state = match load_image(&loader_gpu, &path) {
                        Ok((texture, size)) => ImageState::Ready { texture, size },
                        Err(e) => {
                            log::error!("Loading image {path}: {e:#}");
                            ImageState::Failed
                        }
                    };
                    *slot.state.lock().expect("image slot poisoned") = state;
                }
            })
            .expect("spawning image loader");
        Arc::new(Self { gpu, videos: Mutex::default(), images: Mutex::default(), loads })
    }

    /// The texture following `key`, created on first use.
    pub fn video(&self, key: &VideoKey) -> Arc<SharedFrame> {
        self.videos
            .lock()
            .expect("video textures poisoned")
            .entry(key.clone())
            .or_insert_with(|| Arc::new(SharedFrame::new(&self.gpu.device, FRAME_CAPACITY.0, FRAME_CAPACITY.1)))
            .clone()
    }

    /// The still at `path`, loading in the background on first use.
    pub fn image(&self, path: &str) -> Arc<ImageSlot> {
        let mut images = self.images.lock().expect("image textures poisoned");
        if let Some(slot) = images.get(path) {
            return slot.clone();
        }
        let slot = Arc::new(ImageSlot { state: Mutex::new(ImageState::Loading) });
        images.insert(path.to_string(), slot.clone());
        let _ = self.loads.send((path.to_string(), slot.clone()));
        slot
    }

    /// Videos the pump should keep fed.
    pub fn videos(&self) -> Vec<(VideoKey, Arc<SharedFrame>)> {
        self.videos.lock().expect("video textures poisoned").iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Forgets textures no scene wants. Outputs still fading one out keep
    /// their own reference until the fade ends.
    pub fn retain(&self, videos: &HashSet<VideoKey>, images: &HashSet<String>) {
        self.videos.lock().expect("video textures poisoned").retain(|key, _| videos.contains(key));
        self.images.lock().expect("image textures poisoned").retain(|path, _| images.contains(path));
    }
}

fn load_image(gpu: &Gpu, path: &str) -> anyhow::Result<(Arc<GpuTexture>, (u32, u32))> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::open(path)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut decoded = image::DynamicImage::from_decoder(decoder)?;
    // Browsers honor EXIF orientation for <img>; so do the native outputs.
    decoded.apply_orientation(orientation);
    let limit = MAX_IMAGE.min(gpu.device.limits().max_texture_dimension_2d);
    if decoded.width() > limit || decoded.height() > limit {
        decoded = decoded.resize(limit, limit, image::imageops::FilterType::Triangle);
    }
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Image"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    gpu.queue.submit(None);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Ok((Arc::new(GpuTexture { id: GpuTexture::next_id(), view, owner: Some(texture) }), (width, height)))
}
