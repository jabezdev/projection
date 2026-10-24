//! YUV video frames: their planes upload as they were decoded (about 40% of
//! the bytes of RGBA, with no CPU conversion) and one GPU pass converts them
//! into the frame's shared RGBA texture, scaling them down when they are
//! larger than it.

use crate::Gpu;

/// How a YUV frame's planes are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YuvLayout {
    /// Y, U and V planes; chroma at half width and height.
    I420,
    /// Y plane and one interleaved UV plane.
    Nv12,
}

/// The matrix a frame was encoded with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YuvMatrix {
    Bt601,
    Bt709,
    Bt2020,
}

/// A YUV frame's pixels: each plane's bytes (from its first row on) and
/// bytes per row. I420 uses all three; NV12 the first two.
#[derive(Debug, Clone, Copy)]
pub struct YuvPixels<'a> {
    pub layout: YuvLayout,
    pub planes: [(&'a [u8], usize); 3],
    pub matrix: YuvMatrix,
    /// Full (0–255) rather than limited (16–235) range.
    pub full_range: bool,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    r: [f32; 4],
    g: [f32; 4],
    b: [f32; 4],
    size: [f32; 2],
    flags: [f32; 2],
}

/// Rows mapping normalized (Y, U, V) to gamma-encoded R, G and B.
fn conversion(matrix: YuvMatrix, full_range: bool) -> [[f32; 4]; 3] {
    let (kr, kb) = match matrix {
        YuvMatrix::Bt601 => (0.299, 0.114),
        YuvMatrix::Bt709 => (0.2126, 0.0722),
        YuvMatrix::Bt2020 => (0.2627, 0.0593),
    };
    let kg = 1.0 - kr - kb;
    // Y' = ys * (Y - yo), C' = cs * (C - 0.5).
    let (ys, yo, cs) = if full_range { (1.0, 0.0, 1.0) } else { (255.0 / 219.0, 16.0 / 255.0, 255.0 / 224.0) };
    let rv = 2.0 * (1.0 - kr);
    let bu = 2.0 * (1.0 - kb);
    let gu = -2.0 * kb * (1.0 - kb) / kg;
    let gv = -2.0 * kr * (1.0 - kr) / kg;
    let row = |u: f32, v: f32| [ys, u * cs, v * cs, -ys * yo - (u + v) * cs * 0.5];
    [row(0.0, rv), row(gu, gv), row(bu, 0.0)]
}

/// The conversion pipeline, shared by every video.
pub(crate) struct YuvPipeline {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl YuvPipeline {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("yuv.wgsl"));
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("YUV planes"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                texture(1),
                texture(2),
                texture(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("YUV conversion"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("YUV conversion"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("YUV planes"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Self { layout, pipeline, sampler }
    }
}

impl Gpu {
    /// Created on the first YUV frame.
    pub(crate) fn yuv(&self) -> &YuvPipeline {
        self.yuv.get_or_init(|| YuvPipeline::new(&self.device))
    }
}

/// One video's plane textures, recreated when its size or layout changes.
pub(crate) struct YuvPlanes {
    key: (YuvLayout, u32, u32),
    textures: Vec<wgpu::Texture>,
    uniforms: wgpu::Buffer,
    group: wgpu::BindGroup,
    last: Option<Uniforms>,
}

fn chroma_size(width: u32, height: u32) -> (u32, u32) {
    (width.div_ceil(2), height.div_ceil(2))
}

impl YuvPlanes {
    fn new(gpu: &Gpu, layout: YuvLayout, width: u32, height: u32) -> Self {
        let (cw, ch) = chroma_size(width, height);
        let texture = |label, format, (w, h): (u32, u32)| {
            gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let textures = match layout {
            YuvLayout::I420 => vec![
                texture("Y plane", wgpu::TextureFormat::R8Unorm, (width, height)),
                texture("U plane", wgpu::TextureFormat::R8Unorm, (cw, ch)),
                texture("V plane", wgpu::TextureFormat::R8Unorm, (cw, ch)),
            ],
            YuvLayout::Nv12 => vec![
                texture("Y plane", wgpu::TextureFormat::R8Unorm, (width, height)),
                texture("UV plane", wgpu::TextureFormat::Rg8Unorm, (cw, ch)),
            ],
        };
        let views: Vec<_> = textures.iter().map(|t| t.create_view(&wgpu::TextureViewDescriptor::default())).collect();
        let uniforms = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("YUV conversion"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pipeline = gpu.yuv();
        // NV12 has no V plane; the shader ignores binding 3 then.
        let v_view = views.get(2).unwrap_or(&views[1]);
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("YUV planes"),
            layout: &pipeline.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[0]) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&views[1]) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(v_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&pipeline.sampler) },
            ],
        });
        Self { key: (layout, width, height), textures, uniforms, group, last: None }
    }

    /// Uploads the planes and converts them into the top-left `out` pixels
    /// of `target`. Returns false (uploading nothing) when a plane is too
    /// short for its size.
    pub(crate) fn convert(
        slot: &mut Option<YuvPlanes>,
        gpu: &Gpu,
        pixels: &YuvPixels,
        size: (u32, u32),
        target: &wgpu::Texture,
        out: (u32, u32),
    ) -> bool {
        let (width, height) = size;
        let (cw, ch) = chroma_size(width, height);
        let planes: &[(u32, u32, u32)] = match pixels.layout {
            // (width, height, bytes per pixel) of each plane.
            YuvLayout::I420 => &[(width, height, 1), (cw, ch, 1), (cw, ch, 1)],
            YuvLayout::Nv12 => &[(width, height, 1), (cw, ch, 2)],
        };
        let valid = planes.iter().zip(pixels.planes).all(|(&(w, h, bpp), (data, stride))| {
            let row = (w * bpp) as usize;
            stride >= row && data.len() >= stride * (h as usize - 1) + row
        });
        if !valid {
            return false;
        }
        if slot.as_ref().is_none_or(|p| p.key != (pixels.layout, width, height)) {
            *slot = Some(YuvPlanes::new(gpu, pixels.layout, width, height));
        }
        let state = slot.as_mut().expect("planes just created");
        for ((texture, &(w, h, _)), (data, stride)) in state.textures.iter().zip(planes).zip(pixels.planes) {
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride as u32), rows_per_image: Some(h) },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
        let [r, g, b] = conversion(pixels.matrix, pixels.full_range);
        let flags = [if pixels.layout == YuvLayout::Nv12 { 1.0 } else { 0.0 }, 0.0];
        let uniforms = Uniforms { r, g, b, size: [out.0 as f32, out.1 as f32], flags };
        if state.last != Some(uniforms) {
            gpu.queue.write_buffer(&state.uniforms, 0, bytemuck::bytes_of(&uniforms));
            state.last = Some(uniforms);
        }
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("YUV conversion") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("YUV conversion"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_viewport(0.0, 0.0, out.0 as f32, out.1 as f32, 0.0, 1.0);
            pass.set_pipeline(&gpu.yuv().pipeline);
            pass.set_bind_group(0, &state.group, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit(Some(encoder.finish()));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(rows: [[f32; 4]; 3], yuv: [f32; 3]) -> [f32; 3] {
        rows.map(|row| row[0] * yuv[0] + row[1] * yuv[1] + row[2] * yuv[2] + row[3])
    }

    #[test]
    fn limited_range_black_white_and_primaries_map_to_rgb() {
        let rows = conversion(YuvMatrix::Bt709, false);
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01);
        assert!(close(apply(rows, [16.0 / 255.0, 0.5, 0.5]), [0.0, 0.0, 0.0]));
        assert!(close(apply(rows, [235.0 / 255.0, 0.5, 0.5]), [1.0, 1.0, 1.0]));
        // BT.709 limited-range red is Y 63, Cb 102, Cr 240.
        assert!(close(apply(rows, [63.0 / 255.0, 102.0 / 255.0, 240.0 / 255.0]), [1.0, 0.0, 0.0]));
        let full = conversion(YuvMatrix::Bt601, true);
        assert!(close(apply(full, [1.0, 0.5, 0.5]), [1.0, 1.0, 1.0]));
        // BT.601 full-range green is Y 150, Cb 44, Cr 21.
        assert!(close(apply(full, [150.0 / 255.0, 44.0 / 255.0, 21.0 / 255.0]), [0.0, 1.0, 0.0]));
    }

    struct Frame {
        layout: YuvLayout,
        size: (u32, u32),
        planes: Vec<(Vec<u8>, usize)>,
    }

    impl crate::PixelFrame for Frame {
        fn id(&self) -> u64 {
            1
        }
        fn size(&self) -> (u32, u32) {
            self.size
        }
        fn pixels(&self) -> crate::FramePixels<'_> {
            let plane = |i: usize| self.planes.get(i).map_or((&[][..], 0), |(data, stride)| (&data[..], *stride));
            crate::FramePixels::Yuv(YuvPixels { layout: self.layout, planes: [plane(0), plane(1), plane(2)], matrix: YuvMatrix::Bt709, full_range: false })
        }
    }

    /// A solid BT.709 limited-range red frame with padded rows.
    fn red(layout: YuvLayout, (width, height): (u32, u32)) -> Frame {
        let (cw, ch) = chroma_size(width, height);
        let plane = |w: u32, h: u32, value: &[u8]| {
            let stride = (w as usize * value.len() + 63) & !63;
            let row: Vec<u8> = (0..stride).map(|i| value[i % value.len()]).collect();
            (row.repeat(h as usize), stride)
        };
        let planes = match layout {
            YuvLayout::I420 => vec![plane(width, height, &[63]), plane(cw, ch, &[102]), plane(cw, ch, &[240])],
            YuvLayout::Nv12 => vec![plane(width, height, &[63]), plane(cw, ch, &[102, 240])],
        };
        Frame { layout, size: (width, height), planes }
    }

    #[test]
    fn yuv_frames_convert_on_the_gpu_and_scale_to_fit() {
        let Ok(gpu) = Gpu::headless() else {
            eprintln!("No GPU; skipping YUV conversion test");
            return;
        };
        for (layout, size, shown) in [
            (YuvLayout::I420, (64, 36), (64, 36)),
            (YuvLayout::Nv12, (65, 37), (65, 37)),
            (YuvLayout::I420, (3840, 2160), (1920, 1080)),
        ] {
            let frame = crate::SharedFrame::new(&gpu.device, 1920, 1080);
            assert!(frame.upload(&gpu, &red(layout, size)), "{layout:?} {size:?} uploads");
            assert_eq!(frame.size(), Some(shown));
            let pixels = crate::capture::read_texture(&gpu, &frame.texture, shown);
            let center = ((shown.1 / 2 * shown.0 + shown.0 / 2) * 4) as usize;
            let rgb = &pixels[center..center + 3];
            assert!(rgb[0] > 245 && rgb[1] < 10 && rgb[2] < 10, "{layout:?} {size:?} is red: {rgb:?}");
        }
        let mut short = red(YuvLayout::I420, (64, 36));
        short.planes[2].0.truncate(10);
        assert!(!crate::SharedFrame::new(&gpu.device, 1920, 1080).upload(&gpu, &short), "missing chroma is rejected");
    }
}
