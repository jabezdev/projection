use std::collections::HashMap;

use cosmic_text::{
    CacheKey, SwashCache, SwashContent,
};

use super::*;

pub const MAX_SPREAD: u32 = 96;

pub struct SdfAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    allocator: etagere::AtlasAllocator,
    glyphs: HashMap<(CacheKey, u32), Option<AtlasGlyph>>,
    swash: SwashCache,
}

impl SdfAtlas {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SDF glyph atlas"),
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

    pub fn glyph(&mut self, fonts: &Fonts, queue: &wgpu::Queue, key: CacheKey, spread: u32) -> Result<Option<AtlasGlyph>, AtlasFull> {
        if let Some(entry) = self.glyphs.get(&(key, spread)) {
            return Ok(*entry);
        }
        let image = fonts.with(|system| self.swash.get_image_uncached(system, key));
        let Some(image) = image.filter(|i| i.placement.width > 0 && i.placement.height > 0) else {
            self.glyphs.insert((key, spread), None);
            return Ok(None);
        };
        let coverage: Vec<u8> = match image.content {
            SwashContent::Mask => image.data,
            SwashContent::Color => image.data.chunks_exact(4).map(|p| p[3]).collect(),
            SwashContent::SubpixelMask => image.data.chunks_exact(4).map(|p| p[1]).collect(),
        };
        let (field, width, height) = signed_distance(&coverage, image.placement.width, image.placement.height, spread);
        let allocation = self
            .allocator
            .allocate(etagere::size2(width as i32 + 2, height as i32 + 2))
            .ok_or(AtlasFull)?;
        let (x, y) = (allocation.rectangle.min.x as u32 + 1, allocation.rectangle.min.y as u32 + 1);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
            &field,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        let entry = AtlasGlyph { x, y, width, height, left: image.placement.left - spread as i32, top: image.placement.top + spread as i32 };
        self.glyphs.insert((key, spread), Some(entry));
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

pub fn signed_distance(coverage: &[u8], width: u32, height: u32, spread: u32) -> (Vec<u8>, u32, u32) {
    let pad = spread as usize;
    let (w, h) = (width as usize + 2 * pad, height as usize + 2 * pad);
    let cover = |x: usize, y: usize| -> u8 {
        if x < pad || y < pad || x >= pad + width as usize || y >= pad + height as usize {
            0
        } else {
            coverage[(y - pad) * width as usize + (x - pad)]
        }
    };
    const FAR: f32 = 1e20;
    let mut to_inside = vec![FAR; w * h];
    let mut to_outside = vec![FAR; w * h];
    for y in 0..h {
        for x in 0..w {
            if cover(x, y) >= 128 {
                to_inside[y * w + x] = 0.0;
            } else {
                to_outside[y * w + x] = 0.0;
            }
        }
    }
    distance_transform(&mut to_inside, w, h);
    distance_transform(&mut to_outside, w, h);
    let scale = 1.0 / (2.0 * spread.max(1) as f32);
    let mut field = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let c = cover(x, y);
            let d = if c > 0 && c < 255 {
                0.5 - c as f32 / 255.0
            } else if c >= 128 {
                -(to_outside[i].sqrt() - 0.5)
            } else {
                to_inside[i].sqrt() - 0.5
            };
            field.push(((0.5 + d * scale).clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    (field, w as u32, h as u32)
}

pub(super) fn distance_transform(grid: &mut [f32], w: usize, h: usize) {
    let n = w.max(h);
    let (mut f, mut d, mut v, mut z) = (vec![0.0; n], vec![0.0; n], vec![0usize; n], vec![0.0; n + 1]);
    for x in 0..w {
        for y in 0..h {
            f[y] = grid[y * w + x];
        }
        transform_1d(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            grid[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&grid[y * w..(y + 1) * w]);
        transform_1d(&f[..w], &mut d[..w], &mut v, &mut z);
        grid[y * w..(y + 1) * w].copy_from_slice(&d[..w]);
    }
}

pub(super) fn transform_1d(f: &[f32], d: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        let intersect = |p: usize| ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
        let mut s = intersect(v[k]);
        while s <= z[k] {
            k -= 1;
            s = intersect(v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        *out = (q as f32 - p as f32).powi(2) + f[p];
    }
}
