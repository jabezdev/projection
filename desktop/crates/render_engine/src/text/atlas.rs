use std::collections::HashMap;

use cosmic_text::{
    CacheKey, SwashCache, SwashContent,
};

use super::*;

/// Where a rasterized glyph lives in the atlas and how it sits on the pen.
#[derive(Debug, Clone, Copy)]
pub struct AtlasGlyph {
    /// Texel rectangle in the atlas.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Offset of the image's top-left from the pen position (y down).
    pub left: i32,
    pub top: i32,
}

pub(super) const ATLAS_SIZE: u32 = 2048;

/// A single-channel coverage atlas owned by one output.
pub struct GlyphAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    allocator: etagere::AtlasAllocator,
    glyphs: HashMap<CacheKey, Option<AtlasGlyph>>,
    swash: SwashCache,
}

impl GlyphAtlas {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph atlas"),
            size: wgpu::Extent3d { width: ATLAS_SIZE, height: ATLAS_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            allocator: etagere::AtlasAllocator::new(etagere::size2(ATLAS_SIZE as i32, ATLAS_SIZE as i32)),
            glyphs: HashMap::new(),
            swash: SwashCache::new(),
        }
    }

    /// The atlas entry for `key`, rasterizing on first use. `None` for glyphs
    /// with no pixels (spaces). Returns `Err` when the atlas is full; the
    /// caller clears it and draws the frame again.
    pub fn glyph(&mut self, fonts: &Fonts, queue: &wgpu::Queue, key: CacheKey) -> Result<Option<AtlasGlyph>, AtlasFull> {
        if let Some(entry) = self.glyphs.get(&key) {
            return Ok(*entry);
        }
        let image = fonts.with(|system| self.swash.get_image_uncached(system, key));
        let Some(image) = image.filter(|i| i.placement.width > 0 && i.placement.height > 0) else {
            self.glyphs.insert(key, None);
            return Ok(None);
        };
        let (width, height) = (image.placement.width, image.placement.height);
        let coverage: Vec<u8> = match image.content {
            SwashContent::Mask => image.data,
            // Color glyphs (emoji) keep their shape; color is not supported.
            SwashContent::Color => image.data.chunks_exact(4).map(|p| p[3]).collect(),
            SwashContent::SubpixelMask => image.data.chunks_exact(4).map(|p| p[1]).collect(),
        };
        // One texel of padding keeps linear sampling from bleeding neighbors.
        let allocation = self
            .allocator
            .allocate(etagere::size2(width as i32 + 2, height as i32 + 2))
            .ok_or(AtlasFull)?;
        let (x, y) = (allocation.rectangle.min.x as u32 + 1, allocation.rectangle.min.y as u32 + 1);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
            &coverage,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        let entry = AtlasGlyph { x, y, width, height, left: image.placement.left, top: image.placement.top };
        self.glyphs.insert(key, Some(entry));
        Ok(Some(entry))
    }

    pub fn clear(&mut self) {
        self.allocator.clear();
        self.glyphs.clear();
    }

    pub fn size(&self) -> f32 {
        ATLAS_SIZE as f32
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AtlasFull;
