//! A retained-mode-free 2D painter: scenes describe each frame as a list of
//! rectangles, images and text, which one instanced pipeline draws in order.
//!
//! Colors are sRGB-encoded and blended without linearization, as browsers
//! do, so fades and antialiasing match the webview outputs.

use std::collections::HashMap;
use std::sync::Arc;

use crate::text::{AtlasFull, Fonts, GlyphAtlas, TextLayout};

/// Straight-alpha sRGB color.
pub type Rgba = [f32; 4];

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn intersect(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Rect::new(x, y, (self.right().min(other.right()) - x).max(0.0), (self.bottom().min(other.bottom()) - y).max(0.0))
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right() && other.x < self.right() && self.y < other.bottom() && other.y < self.bottom()
    }
}

/// A sampled texture. `id` identifies it for bind group reuse.
pub struct GpuTexture {
    pub id: u64,
    pub view: wgpu::TextureView,
    /// Keeps the texture alive with its view when nothing else owns it.
    pub owner: Option<wgpu::Texture>,
}

impl GpuTexture {
    pub fn next_id() -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

pub enum Item {
    Fill { rect: Rect, color: Rgba },
    Image { rect: Rect, uv: [f32; 4], texture: Arc<GpuTexture>, opacity: f32 },
    /// `layout` drawn with its top-left at (x, y), rotated by `angle` radians
    /// (clockwise on screen) around that point.
    Text { layout: Arc<TextLayout>, x: f32, y: f32, color: Rgba, angle: f32 },
}

struct Entry {
    item: Item,
    clip: Option<Rect>,
}

/// One frame's drawing, back to front.
#[derive(Default)]
pub struct DrawList {
    entries: Vec<Entry>,
    clip: Option<Rect>,
}

impl DrawList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clips following items to `rect` (`None` removes clipping).
    pub fn set_clip(&mut self, rect: Option<Rect>) {
        self.clip = rect;
    }

    pub fn push(&mut self, item: Item) {
        self.entries.push(Entry { item, clip: self.clip });
    }

    pub fn fill(&mut self, rect: Rect, color: Rgba) {
        if color[3] > 0.0 && rect.w > 0.0 && rect.h > 0.0 {
            self.push(Item::Fill { rect, color });
        }
    }

    /// Four edges of width `width` inside `rect`.
    pub fn border(&mut self, rect: Rect, width: f32, color: Rgba) {
        if width <= 0.0 {
            return;
        }
        let w = width.min(rect.w / 2.0).min(rect.h / 2.0);
        self.fill(Rect::new(rect.x, rect.y, rect.w, w), color);
        self.fill(Rect::new(rect.x, rect.bottom() - w, rect.w, w), color);
        self.fill(Rect::new(rect.x, rect.y + w, w, rect.h - 2.0 * w), color);
        self.fill(Rect::new(rect.right() - w, rect.y + w, w, rect.h - 2.0 * w), color);
    }

    pub fn text(&mut self, layout: &Arc<TextLayout>, x: f32, y: f32, color: Rgba) {
        if color[3] > 0.0 && !layout.glyphs.is_empty() {
            self.push(Item::Text { layout: layout.clone(), x, y, color, angle: 0.0 });
        }
    }

    pub fn image(&mut self, rect: Rect, uv: [f32; 4], texture: &Arc<GpuTexture>, opacity: f32) {
        if opacity > 0.0 {
            self.push(Item::Image { rect, uv, texture: texture.clone(), opacity });
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    rect: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    transform: [f32; 4],
    kind: [f32; 4],
}

const SOLID: f32 = 0.0;
const TEXTURE: f32 = 1.0;
const MASK: f32 = 2.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Binding {
    Atlas,
    Texture(u64),
}

struct Batch {
    binding: Binding,
    scissor: Option<[u32; 4]>,
    range: std::ops::Range<u32>,
}

pub struct Painter {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    instances: wgpu::Buffer,
    capacity: usize,
    atlas: GlyphAtlas,
    atlas_group: wgpu::BindGroup,
    textures: HashMap<u64, wgpu::BindGroup>,
}

impl Painter {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("paint.wgsl"));
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Paint globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Paint texture"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            }],
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Paint globals"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Paint sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Paint globals"),
            layout: &globals_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Paint"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Paint"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let atlas = GlyphAtlas::new(device);
        let atlas_group = texture_group(device, &texture_layout, &atlas.view);
        let capacity = 1024;
        Self {
            pipeline,
            globals,
            globals_group,
            texture_layout,
            instances: instance_buffer(device, capacity),
            capacity,
            atlas,
            atlas_group,
            textures: HashMap::new(),
        }
    }

    /// Draws `list` into `target` (cleared to black first).
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        fonts: &Fonts,
        list: &DrawList,
        target: &wgpu::TextureView,
        size: (u32, u32),
        encoder: &mut wgpu::CommandEncoder,
    ) {
        self.paint_in(device, queue, fonts, list, target, [0, 0, size.0, size.1], encoder);
    }

    /// Draws `list`, laid out for an `area` (x, y, width, height) sized
    /// canvas, into that part of `target`; the rest stays black.
    #[allow(clippy::too_many_arguments)]
    pub fn paint_in(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        fonts: &Fonts,
        list: &DrawList,
        target: &wgpu::TextureView,
        area: [u32; 4],
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let size = (area[2], area[3]);
        let (instances, batches) = match self.build(queue, fonts, list, size) {
            Ok(built) => built,
            Err(AtlasFull) => {
                // Too many distinct glyphs: start over with an empty atlas.
                self.atlas.clear();
                self.build(queue, fonts, list, size).unwrap_or_default()
            }
        };
        if instances.len() > self.capacity {
            self.capacity = instances.len().next_power_of_two();
            self.instances = instance_buffer(device, self.capacity);
        }
        queue.write_buffer(&self.globals, 0, bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]));
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        }
        // Bind groups for textures no longer drawn are dropped.
        let mut used = HashMap::new();
        for item in &list.entries {
            if let Item::Image { texture, .. } = &item.item {
                let group = self.textures.remove(&texture.id).unwrap_or_else(|| texture_group(device, &self.texture_layout, &texture.view));
                used.insert(texture.id, group);
            }
        }
        self.textures = used;

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Paint"),
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
        pass.set_viewport(area[0] as f32, area[1] as f32, size.0 as f32, size.1 as f32, 0.0, 1.0);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        for batch in &batches {
            let group = match batch.binding {
                Binding::Atlas => &self.atlas_group,
                Binding::Texture(id) => match self.textures.get(&id) {
                    Some(group) => group,
                    None => continue,
                },
            };
            match batch.scissor {
                Some([_, _, w, h]) if w == 0 || h == 0 => continue,
                Some([x, y, w, h]) => pass.set_scissor_rect(area[0] + x, area[1] + y, w, h),
                None => pass.set_scissor_rect(area[0], area[1], size.0, size.1),
            }
            pass.set_bind_group(1, group, &[]);
            pass.draw(0..4, batch.range.clone());
        }
    }

    fn build(&mut self, queue: &wgpu::Queue, fonts: &Fonts, list: &DrawList, size: (u32, u32)) -> Result<(Vec<Instance>, Vec<Batch>), AtlasFull> {
        let mut instances = Vec::new();
        let mut batches: Vec<Batch> = Vec::new();
        let atlas_size = self.atlas.size();
        for entry in &list.entries {
            let scissor = entry.clip.map(|clip| scissor(clip, size));
            let start = instances.len() as u32;
            let binding = match &entry.item {
                Item::Fill { rect, color } => {
                    instances.push(Instance {
                        rect: [rect.x, rect.y, rect.w, rect.h],
                        uv: [0.0; 4],
                        color: *color,
                        transform: [0.0, 0.0, 1.0, 0.0],
                        kind: [SOLID, 0.0, 0.0, 0.0],
                    });
                    None
                }
                Item::Image { rect, uv, texture, opacity } => {
                    instances.push(Instance {
                        rect: [rect.x, rect.y, rect.w, rect.h],
                        uv: *uv,
                        color: [1.0, 1.0, 1.0, *opacity],
                        transform: [0.0, 0.0, 1.0, 0.0],
                        kind: [TEXTURE, 0.0, 0.0, 0.0],
                    });
                    Some(Binding::Texture(texture.id))
                }
                Item::Text { layout, x, y, color, angle } => {
                    let (sin, cos) = angle.sin_cos();
                    for glyph in &layout.glyphs {
                        let Some(g) = self.atlas.glyph(fonts, queue, glyph.key)? else { continue };
                        let local = [(glyph.x + g.left) as f32, (glyph.y - g.top) as f32, g.width as f32, g.height as f32];
                        instances.push(Instance {
                            rect: local,
                            uv: [
                                g.x as f32 / atlas_size,
                                g.y as f32 / atlas_size,
                                (g.x + g.width) as f32 / atlas_size,
                                (g.y + g.height) as f32 / atlas_size,
                            ],
                            color: *color,
                            transform: [x.round(), y.round(), cos, sin],
                            kind: [MASK, 0.0, 0.0, 0.0],
                        });
                    }
                    Some(Binding::Atlas)
                }
            };
            let end = instances.len() as u32;
            if end == start {
                continue;
            }
            // Solid fills ignore the texture, so they join any batch.
            match batches.last_mut() {
                Some(last) if last.scissor == scissor && binding.is_none_or(|b| b == last.binding) && last.range.end == start => {
                    last.range.end = end;
                }
                _ => batches.push(Batch { binding: binding.unwrap_or(Binding::Atlas), scissor, range: start..end }),
            }
        }
        Ok((instances, batches))
    }
}

fn scissor(clip: Rect, size: (u32, u32)) -> [u32; 4] {
    let x0 = clip.x.max(0.0).floor().min(size.0 as f32) as u32;
    let y0 = clip.y.max(0.0).floor().min(size.1 as f32) as u32;
    let x1 = clip.right().max(0.0).ceil().min(size.0 as f32) as u32;
    let y1 = clip.bottom().max(0.0).ceil().min(size.1 as f32) as u32;
    [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]
}

fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Paint instances"),
        size: (capacity * std::mem::size_of::<Instance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn texture_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, view: &wgpu::TextureView) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Paint texture"),
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) }],
    })
}

/// Parses `#rgb`, `#rrggbb` or `#rrggbbaa`; anything else is `fallback`.
pub fn css_color(value: &str, fallback: Rgba) -> Rgba {
    let hex = value.trim().trim_start_matches('#');
    let digits: Option<Vec<u8>> = match hex.len() {
        3 => hex.chars().map(|c| c.to_digit(16).map(|d| (d * 17) as u8)).collect(),
        6 | 8 => (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect(),
        _ => None,
    };
    match digits.as_deref() {
        Some([r, g, b]) => [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0],
        Some([r, g, b, a]) => [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, *a as f32 / 255.0],
        _ => fallback,
    }
}

pub fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    [color[0], color[1], color[2], color[3] * alpha]
}

/// `rect` and texture coordinates that place a `content`-sized image in
/// `area`: `cover` crops to fill, otherwise the image is letterboxed.
pub fn fit(content: (u32, u32), area: Rect, cover: bool) -> (Rect, [f32; 4]) {
    let (cw, ch) = (content.0.max(1) as f32, content.1.max(1) as f32);
    let scale = if cover { (area.w / cw).max(area.h / ch) } else { (area.w / cw).min(area.h / ch) };
    let (w, h) = (cw * scale, ch * scale);
    if cover {
        // Crop the overflow evenly from both sides.
        let (u, v) = ((w - area.w) / w / 2.0, (h - area.h) / h / 2.0);
        (area, [u, v, 1.0 - u, 1.0 - v])
    } else {
        (Rect::new(area.x + (area.w - w) / 2.0, area.y + (area.h - h) / 2.0, w, h), [0.0, 0.0, 1.0, 1.0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_css_hex_colors() {
        assert_eq!(css_color("#fff", [0.0; 4]), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(css_color("#000000", [1.0; 4]), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(css_color("#ff000080", [0.0; 4])[3], 128.0 / 255.0);
        assert_eq!(css_color("red", [0.5; 4]), [0.5; 4]);
    }

    #[test]
    fn cover_crops_and_contain_letterboxes() {
        let area = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let (rect, uv) = fit((1080, 1080), area, true);
        assert_eq!(rect, area);
        assert!((uv[1] - (1.0 - 1080.0 / 1920.0) / 2.0).abs() < 1e-6);
        let (rect, uv) = fit((1080, 1080), area, false);
        assert_eq!((rect.x, rect.w, rect.h), (420.0, 1080.0, 1080.0));
        assert_eq!(uv, [0.0, 0.0, 1.0, 1.0]);
    }
}
