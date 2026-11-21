use crate::text::{AtlasFull, Fonts, TextLayout};

use super::*;

impl Painter {
    pub(super) fn build(&mut self, queue: &wgpu::Queue, fonts: &Fonts, list: &DrawList, size: (u32, u32)) -> Result<(Vec<Instance>, Vec<Batch>), AtlasFull> {
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
                Item::Text { layout, x, y, color, angle, effects } => {
                    let (sin, cos) = angle.sin_cos();
                    let transform = [x.round(), y.round(), cos, sin];
                    if !effects.is_empty() {
                        self.effect_instances(queue, fonts, layout, effects, transform, &mut instances)?;
                        push_batch(&mut batches, Binding::Sdf, scissor, start..instances.len() as u32);
                    }
                    let fill_start = instances.len() as u32;
                    if color[3] <= 0.0 {
                        continue;
                    }
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
                            transform,
                            kind: [MASK, 0.0, 0.0, 0.0],
                        });
                    }
                    push_batch(&mut batches, Binding::Atlas, scissor, fill_start..instances.len() as u32);
                    continue;
                }
            };
            let end = instances.len() as u32;
            if end == start {
                continue;
            }
            match batches.last_mut() {
                Some(last) if last.scissor == scissor && binding.is_none_or(|b| b == last.binding) && last.range.end == start => {
                    last.range.end = end;
                }
                _ => batches.push(Batch { binding: binding.unwrap_or(Binding::Atlas), scissor, range: start..end }),
            }
        }
        Ok((instances, batches))
    }
    pub(super) fn effect_instances(&mut self, queue: &wgpu::Queue, fonts: &Fonts, layout: &TextLayout, effects: &TextEffects, transform: [f32; 4], instances: &mut Vec<Instance>) -> Result<(), AtlasFull> {
        let spread = effects.spread();
        let atlas_size = self.sdf.size();
        let mut glyphs = Vec::with_capacity(layout.glyphs.len());
        for glyph in &layout.glyphs {
            if let Some(g) = self.sdf.glyph(fonts, queue, glyph.key, spread)? {
                glyphs.push(([(glyph.x + g.left) as f32, (glyph.y - g.top) as f32, g.width as f32, g.height as f32], g));
            }
        }
        let mut layers = Vec::new();
        if let Some((color, dx, dy, blur)) = effects.shadow {
            let soft = blur.max(0.7);
            layers.push((color, (dx, dy), [-soft, soft]));
        }
        if let Some((color, size)) = effects.glow {
            layers.push((color, (0.0, 0.0), [-size.max(0.7), size.max(0.7)]));
        }
        if let Some((color, width)) = effects.outline {
            layers.push((color, (0.0, 0.0), [width - 0.6, width + 0.6]));
        }
        for (color, (dx, dy), band) in layers {
            if color[3] <= 0.0 {
                continue;
            }
            for (rect, g) in &glyphs {
                instances.push(Instance {
                    rect: [rect[0] + dx, rect[1] + dy, rect[2], rect[3]],
                    uv: [g.x as f32 / atlas_size, g.y as f32 / atlas_size, (g.x + g.width) as f32 / atlas_size, (g.y + g.height) as f32 / atlas_size],
                    color,
                    transform,
                    kind: [SDF, spread as f32, band[0], band[1]],
                });
            }
        }
        Ok(())
    }
}

pub(super) fn push_batch(batches: &mut Vec<Batch>, binding: Binding, scissor: Option<[u32; 4]>, range: std::ops::Range<u32>) {
    if range.is_empty() {
        return;
    }
    match batches.last_mut() {
        Some(last) if last.scissor == scissor && last.binding == binding && last.range.end == range.start => last.range.end = range.end,
        _ => batches.push(Batch { binding, scissor, range }),
    }
}

pub(super) fn scissor(clip: Rect, size: (u32, u32)) -> [u32; 4] {
    let x0 = clip.x.max(0.0).floor().min(size.0 as f32) as u32;
    let y0 = clip.y.max(0.0).floor().min(size.1 as f32) as u32;
    let x1 = clip.right().max(0.0).ceil().min(size.0 as f32) as u32;
    let y1 = clip.bottom().max(0.0).ceil().min(size.1 as f32) as u32;
    [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]
}

pub(super) fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Paint instances"),
        size: (capacity * std::mem::size_of::<Instance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(super) fn texture_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, view: &wgpu::TextureView) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Paint texture"),
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) }],
    })
}
