use wgpu::util::DeviceExt;
use crate::render::Vertex;
use super::{RenderContext, SceneNode, Transform, UpdateContext};
use bytemuck::{Pod, Zeroable};

// Simple quad for the video surface.
const VERTICES: &[Vertex] = &[
    // bottom left
    Vertex { position: [-1.0, -1.0, 0.0], tex_coords: [0.0, 1.0] },
    // top left
    Vertex { position: [-1.0,  1.0, 0.0], tex_coords: [0.0, 0.0] },
    // top right
    Vertex { position: [ 1.0,  1.0, 0.0], tex_coords: [1.0, 0.0] },
    // bottom right
    Vertex { position: [ 1.0, -1.0, 0.0], tex_coords: [1.0, 1.0] },
];

const INDICES: &[u16] = &[
    0, 2, 1,
    0, 3, 2,
];

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
struct TransformUniform {
    model_matrix: [[f32; 4]; 4],
    opacity: f32,
    _padding: [u32; 3], // 16-byte alignment requirements
}

pub struct VideoNode {
    pub transform: Transform,
    pub opacity: f32,
    
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    
    transform_buffer: wgpu::Buffer,
    transform_bind_group: wgpu::BindGroup,
    
    texture_bind_group: Option<wgpu::BindGroup>,
    pub player: Option<crate::video::VideoPlayer>,
    
    // Cached texture state to avoid recreation
    current_texture: Option<wgpu::Texture>,
    current_dimensions: (u32, u32),
}

impl VideoNode {
    pub fn new(device: &wgpu::Device, transform_layout: &wgpu::BindGroupLayout) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Video Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Video Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let initial_transform = TransformUniform {
            model_matrix: glam::Mat4::IDENTITY.to_cols_array_2d(),
            opacity: 1.0,
            _padding: [0; 3],
        };

        let transform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Video Transform Buffer"),
            contents: bytemuck::cast_slice(&[initial_transform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let transform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Video Transform Bind Group"),
            layout: transform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: transform_buffer.as_entire_binding(),
            }],
        });

        Self {
            transform: Transform::default(),
            opacity: 1.0,
            vertex_buffer,
            index_buffer,
            transform_buffer,
            transform_bind_group,
            texture_bind_group: None,
            player: None,
            current_texture: None,
            current_dimensions: (0, 0),
        }
    }

    pub fn set_player(&mut self, player: crate::video::VideoPlayer) {
        self.player = Some(player);
    }

    pub fn update_from_player(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
    ) {
        if let Some(player) = &self.player {
            if let Some(frame) = player.next_frame() {
                let dims = (frame.width, frame.height);
                
                // Only recreate texture if dimensions changed or first frame
                if self.current_texture.is_none() || self.current_dimensions != dims {
                    let texture_size = wgpu::Extent3d {
                        width: frame.width,
                        height: frame.height,
                        depth_or_array_layers: 1,
                    };

                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        size: texture_size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        label: Some("VideoNode Player Texture"),
                        view_formats: &[],
                    });

                    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                        address_mode_u: wgpu::AddressMode::ClampToEdge,
                        address_mode_v: wgpu::AddressMode::ClampToEdge,
                        address_mode_w: wgpu::AddressMode::ClampToEdge,
                        mag_filter: wgpu::FilterMode::Linear,
                        min_filter: wgpu::FilterMode::Linear,
                        ..Default::default()
                    });

                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&texture_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&sampler),
                            },
                        ],
                        label: Some("VideoNode Player Bind Group"),
                    });

                    self.texture_bind_group = Some(bind_group);
                    self.current_texture = Some(texture);
                    self.current_dimensions = dims;
                }

                // Update texture content
                if let Some(texture) = &self.current_texture {
                    queue.write_texture(
                        wgpu::ImageCopyTexture {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &frame.data,
                        wgpu::ImageDataLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * frame.width),
                            rows_per_image: Some(frame.height),
                        },
                        wgpu::Extent3d {
                            width: frame.width,
                            height: frame.height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
        }
    }

    pub fn update_texture(&mut self, bind_group: wgpu::BindGroup) {
        self.texture_bind_group = Some(bind_group);
        self.player = None; // Disable player if manual texture is set
    }

    /// Load an image from memory, create a wgpu Texture, and generate a bind group.
    pub fn load_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        image_data: &[u8],
    ) -> anyhow::Result<()> {
        self.player = None; // Disable player if image is loaded
        let img = image::load_from_memory(image_data)?.into_rgba8();
        let dimensions = img.dimensions();

        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("VideoNode Image Texture"),
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &img,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            texture_size,
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("VideoNode Texture Bind Group"),
        });

        self.texture_bind_group = Some(bind_group);
        Ok(())
    }
}

impl SceneNode for VideoNode {
    fn update(&mut self, _ctx: &UpdateContext) {
        // Animation updates could go here if tracks were attached.
    }

    fn render<'a>(&'a self, ctx: &mut RenderContext<'a>) {
        if let Some(tex_bind_group) = &self.texture_bind_group {
            // Update uniforms
            let uniform = TransformUniform {
                model_matrix: glam::Mat4::from_scale_rotation_translation(
                    self.transform.scale,
                    self.transform.rotation,
                    self.transform.position,
                ).to_cols_array_2d(),
                opacity: self.opacity,
                _padding: [0; 3],
            };
            
            ctx.queue.write_buffer(&self.transform_buffer, 0, bytemuck::cast_slice(&[uniform]));

            // Draw
            ctx.render_pass.set_pipeline(ctx.quad_pipeline);
            ctx.render_pass.set_bind_group(0, &self.transform_bind_group, &[]);
            ctx.render_pass.set_bind_group(1, tex_bind_group, &[]);
            ctx.render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            ctx.render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            ctx.render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
        }
    }
}
