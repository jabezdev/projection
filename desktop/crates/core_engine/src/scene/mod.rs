pub mod video;
pub mod text;

pub use video::VideoNode;
pub use text::TextNode;

use crate::render::TextResources;

pub struct UpdateContext {
    pub audio_time: f64,
    pub delta_time: f64,
}

/// Per-frame setup that happens before the render pass begins (buffer uploads,
/// glyph layout and atlas updates).
pub struct PrepareContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub text: &'a mut TextResources,
    pub width: u32,
    pub height: u32,
}

pub struct RenderContext<'pass> {
    pub device: &'pass wgpu::Device,
    pub queue: &'pass wgpu::Queue,
    pub render_pass: &'pass mut wgpu::RenderPass<'pass>,
    pub quad_pipeline: &'pass wgpu::RenderPipeline,
    pub transform_bind_group_layout: &'pass wgpu::BindGroupLayout,
    pub texture_bind_group_layout: &'pass wgpu::BindGroupLayout,
    pub text_atlas: &'pass glyphon::TextAtlas,
    pub viewport: &'pass glyphon::Viewport,
}

pub trait SceneNode: Send {
    fn update(&mut self, ctx: &UpdateContext);
    fn prepare(&mut self, _ctx: &mut PrepareContext) {}
    fn render<'pass>(&'pass self, ctx: &mut RenderContext<'pass>);
}

/// Transform for grouping, text, or video nodes.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub position: glam::Vec3,
    pub scale: glam::Vec3,
    pub rotation: glam::Quat,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: glam::Vec3::ZERO,
            scale: glam::Vec3::ONE,
            rotation: glam::Quat::IDENTITY,
        }
    }
}
