//! Built-in animated backgrounds. Each active preset renders into its own
//! shared frame on the frame pump, like a decoded video, so both outputs and
//! the previews sample one texture and no output does extra work.
//!
//! Hard limits keep them from taking over the GPU: a fixed render size
//! (scaled up when drawn), a frame rate cap, never more than one frame in
//! flight, and a fall back to a still frame when the GPU keeps falling
//! behind. With animation off a preset renders once, as a still.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::frame::SharedFrame;
use crate::paint::{css_color, Rgba};
use crate::Gpu;

/// Render size; outputs scale it to fill. Smooth gradients lose nothing.
pub const SHADER_SIZE: (u32, u32) = (960, 540);
/// Frame rate cap.
const FRAME_INTERVAL: Duration = Duration::from_millis(33);
/// A preset falls back to a still when more than half its frames in this
/// window were due while the previous one was still on the GPU.
const BUDGET_WINDOW: Duration = Duration::from_secs(5);
const PRESETS: [&str; 3] = ["gradient", "aurora", "bokeh"];

/// A preset and its parameters, carried in `VideoKey::Shader`.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderSpec {
    pub preset: String,
    pub color_a: Rgba,
    pub color_b: Rgba,
    pub speed: f32,
    pub animate: bool,
}

impl ShaderSpec {
    /// `preset|#colorA|#colorB|speed|animate`.
    pub fn encode(preset: &str, color_a: &str, color_b: &str, speed: f32, animate: bool) -> String {
        format!("{preset}|{color_a}|{color_b}|{speed}|{}", if animate { 1 } else { 0 })
    }

    pub fn parse(key: &str) -> Option<Self> {
        let mut parts = key.split('|');
        let preset = parts.next()?.to_string();
        let color_a = css_color(parts.next()?, [0.1, 0.1, 0.2, 1.0]);
        let color_b = css_color(parts.next()?, [0.3, 0.8, 0.7, 1.0]);
        let speed = parts.next()?.parse::<f32>().ok()?.clamp(0.05, 5.0);
        let animate = parts.next()? == "1";
        Some(Self { preset, color_a, color_b, speed, animate })
    }

    fn preset_index(&self) -> u32 {
        PRESETS.iter().position(|p| *p == self.preset).unwrap_or(0) as u32
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    color_a: [f32; 4],
    color_b: [f32; 4],
    resolution: [f32; 2],
    time: f32,
    preset: u32,
}

struct State {
    start: Instant,
    next_due: Instant,
    rendered: bool,
    frozen: bool,
    /// Cleared when the GPU finishes the last submitted frame.
    in_flight: Arc<AtomicBool>,
    window: (Instant, u32, u32),
}

pub struct ShaderBackgrounds {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    group: wgpu::BindGroup,
    states: HashMap<String, State>,
}

impl ShaderBackgrounds {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader_bg.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shader background"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shader background uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shader background"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shader background"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shader background"),
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
        Self { pipeline, uniforms, group, states: HashMap::new() }
    }

    /// Forgets presets no longer shown.
    pub fn retain(&mut self, keys: &[&String]) {
        self.states.retain(|key, _| keys.contains(&key));
    }

    /// Renders `key` into `frame` when a frame is due and the GPU has room.
    /// Returns whether work was submitted.
    pub fn tick(&mut self, gpu: &Gpu, key: &str, frame: &SharedFrame, now: Instant) -> bool {
        let Some(spec) = ShaderSpec::parse(key) else { return false };
        let state = self.states.entry(key.to_string()).or_insert_with(|| State {
            start: now,
            next_due: now,
            rendered: false,
            frozen: false,
            in_flight: Arc::new(AtomicBool::new(false)),
            window: (now, 0, 0),
        });
        let still = !spec.animate || state.frozen;
        if (still && state.rendered) || now < state.next_due {
            return false;
        }
        state.next_due = now + FRAME_INTERVAL;
        let (window_start, due, late) = &mut state.window;
        *due += 1;
        if state.in_flight.load(Ordering::Acquire) {
            *late += 1;
            if now - *window_start >= BUDGET_WINDOW {
                if *due >= 10 && *late * 2 > *due {
                    state.frozen = true;
                    log::warn!("Shader background {} cannot keep up with the GPU; showing a still", spec.preset);
                }
                state.window = (now, 0, 0);
            }
            return false;
        }
        if now - *window_start >= BUDGET_WINDOW {
            state.window = (now, 0, 0);
        }
        let time = if spec.animate { (now - state.start).as_secs_f32() * spec.speed } else { 12.0 };
        let uniforms = Uniforms {
            color_a: spec.color_a,
            color_b: spec.color_b,
            resolution: [SHADER_SIZE.0 as f32, SHADER_SIZE.1 as f32],
            time,
            preset: spec.preset_index(),
        };
        gpu.queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Shader background") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Shader background"),
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
            pass.set_viewport(0.0, 0.0, SHADER_SIZE.0 as f32, SHADER_SIZE.1 as f32, 0.0, 1.0);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.draw(0..3, 0..1);
        }
        state.in_flight.store(true, Ordering::Release);
        let in_flight = state.in_flight.clone();
        gpu.queue.submit(Some(encoder.finish()));
        gpu.queue.on_submitted_work_done(move || in_flight.store(false, Ordering::Release));
        frame.rendered(SHADER_SIZE);
        state.rendered = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_round_trip_through_their_key() {
        let key = ShaderSpec::encode("aurora", "#102030", "#47c6bd", 1.5, true);
        let spec = ShaderSpec::parse(&key).unwrap();
        assert_eq!((spec.preset.as_str(), spec.speed, spec.animate), ("aurora", 1.5, true));
        assert!((spec.color_a[0] - 16.0 / 255.0).abs() < 1e-6);
        assert_eq!(ShaderSpec::parse("bokeh|#000|#fff|0|0").unwrap().speed, 0.05, "speed is clamped");
        assert!(ShaderSpec::parse("nonsense").is_none());
    }

    #[test]
    fn every_preset_renders_colors_into_its_frame() {
        let Ok(gpu) = Gpu::headless() else {
            eprintln!("No GPU; skipping shader background test");
            return;
        };
        let mut shaders = ShaderBackgrounds::new(&gpu.device);
        for preset in PRESETS {
            let frame = SharedFrame::new(&gpu.device, 1920, 1080);
            let key = ShaderSpec::encode(preset, "#203060", "#47c6bd", 1.0, false);
            assert!(shaders.tick(&gpu, &key, &frame, Instant::now()), "{preset} renders once");
            assert_eq!(frame.size(), Some(SHADER_SIZE));
            assert!(!shaders.tick(&gpu, &key, &frame, Instant::now() + Duration::from_secs(1)), "a still renders only once");
            let pixels = crate::capture::read_texture(&gpu, &frame.texture, SHADER_SIZE);
            let lit = pixels.chunks_exact(4).filter(|p| p[0] as u32 + p[1] as u32 + p[2] as u32 > 30).count();
            assert!(lit > pixels.len() / 8, "{preset} draws visible color ({lit} lit pixels)");
        }
    }
}
