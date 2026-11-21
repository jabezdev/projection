use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::media::MediaTextures;
use crate::output::{OutputRole, Renderer};
use crate::paint::Painter;
use crate::scene::{LyricStyle, MainScene, MediaLayer, MediaSource, Scene, Shadow, Stroke};
use crate::shader_bg::ShaderBackgrounds;
use crate::text::Fonts;
use crate::{css_color, Gpu, ShaderSpec, VideoKey};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const FRAME: Duration = Duration::from_micros(16_667);

#[derive(Debug, Clone)]
pub struct RenderBench {
    pub adapter: String,
    pub frames: u32,
    pub mean_ms: f64,
    pub worst_ms: f64,
}

pub fn adapters() -> Vec<String> {
    let instance = crate::gpu::new_instance();
    pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()))
        .into_iter()
        .map(|adapter| {
            let info = adapter.get_info();
            format!("{} ({:?}, {:?}, driver {} {})", info.name, info.backend, info.device_type, info.driver, info.driver_info).trim_end().to_string()
        })
        .collect()
}

fn heavy_scene() -> Scene {
    let shader = ShaderSpec::encode("aurora", "#14213d", "#47c6bd", 1.0, true);
    Scene::Main(MainScene {
        text: "Amazing grace how sweet the sound\nThat saved a wretch like me".into(),
        background: Some(MediaLayer { source: MediaSource::Video(VideoKey::Shader(shader)), cover: true }),
        transition_ms: 250.0,
        style: LyricStyle {
            shadow: Some(Shadow { color: [0.0, 0.0, 0.0, 0.8], x: 0.0, y: 6.0, blur: 10.0 }),
            outline: Some(Stroke { color: css_color("#14213d", [1.0; 4]), size: 3.0 }),
            glow: Some(Stroke { color: [1.0, 0.9, 0.5, 0.6], size: 14.0 }),
            ..LyricStyle::default()
        },
        ..Default::default()
    })
}

pub fn effects_and_shader(size: (u32, u32), frames: u32) -> anyhow::Result<RenderBench> {
    let gpu = Arc::new(Gpu::headless()?);
    let fonts = Fonts::load();
    let media = MediaTextures::new(gpu.clone());
    let mut renderer = Renderer::for_role(OutputRole::Projection);
    let mut painter = Painter::new(&gpu.device, FORMAT);
    let mut shaders = ShaderBackgrounds::new(&gpu.device);
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Benchmark"),
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let start = Instant::now();
    renderer.set_scene(heavy_scene(), &media, start);
    let mut times = Vec::with_capacity(frames as usize);
    for index in 0..frames + 10 {
        let now = start + FRAME * index;
        let began = Instant::now();
        for (key, frame) in media.videos() {
            if let VideoKey::Shader(spec) = key {
                shaders.tick(&gpu, &spec, &frame, now);
            }
        }
        let list = renderer.frame(&fonts, size, now);
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Benchmark") });
        painter.paint(&gpu.device, &gpu.queue, &fonts, &list, &view, size, &mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        if index >= 10 {
            times.push(began.elapsed());
        }
    }
    let total: Duration = times.iter().sum();
    Ok(RenderBench {
        adapter: gpu.adapter.get_info().name,
        frames,
        mean_ms: total.as_secs_f64() * 1e3 / frames.max(1) as f64,
        worst_ms: times.iter().max().map_or(0.0, |t| t.as_secs_f64() * 1e3),
    })
}
