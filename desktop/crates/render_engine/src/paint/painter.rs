use std::collections::HashMap;

use crate::text::{AtlasFull, Fonts, GlyphAtlas, SdfAtlas};

use super::*;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Instance {
    pub(super) rect: [f32; 4],
    pub(super) uv: [f32; 4],
    pub(super) color: [f32; 4],
    pub(super) transform: [f32; 4],
    pub(super) kind: [f32; 4],
}

pub(super) const SOLID: f32 = 0.0;
pub(super) const TEXTURE: f32 = 1.0;
pub(super) const MASK: f32 = 2.0;
pub(super) const SDF: f32 = 3.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Binding {
    Atlas,
    Sdf,
    Texture(u64),
}

pub(super) struct Batch {
    pub(super) binding: Binding,
    pub(super) scissor: Option<[u32; 4]>,
    pub(super) range: std::ops::Range<u32>,
}

pub struct Painter {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) globals: wgpu::Buffer,
    pub(super) globals_group: wgpu::BindGroup,
    pub(super) texture_layout: wgpu::BindGroupLayout,
    pub(super) instances: wgpu::Buffer,
    pub(super) capacity: usize,
    pub(super) atlas: GlyphAtlas,
    pub(super) atlas_group: wgpu::BindGroup,
    pub(super) sdf: SdfAtlas,
    pub(super) sdf_group: wgpu::BindGroup,
    pub(super) textures: HashMap<u64, wgpu::BindGroup>,
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
        let sdf = SdfAtlas::new(device);
        let sdf_group = texture_group(device, &texture_layout, &sdf.view);
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
            sdf,
            sdf_group,
            textures: HashMap::new(),
        }
    }

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
                self.atlas.clear();
                self.sdf.clear();
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
                Binding::Sdf => &self.sdf_group,
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
}
