use glyphon::{Attrs, Buffer, Color, Family, Metrics, Shaping, TextArea, TextBounds, TextRenderer};

use super::{PrepareContext, RenderContext, SceneNode, UpdateContext};
use crate::render::TextResources;

/// A block of text laid out with glyphon and drawn in the main render pass.
pub struct TextNode {
    /// Top-left corner, in pixels.
    pub left: f32,
    pub top: f32,
    pub opacity: f32,

    buffer: Buffer,
    renderer: TextRenderer,
}

impl TextNode {
    pub fn new(
        device: &wgpu::Device,
        text: &mut TextResources,
        width: u32,
        height: u32,
    ) -> Self {
        let renderer = TextRenderer::new(
            &mut text.atlas,
            device,
            wgpu::MultisampleState::default(),
            None,
        );
        let mut buffer = Buffer::new(&mut text.font_system, Metrics::new(64.0, 72.0));
        buffer.set_size(&mut text.font_system, Some(width as f32), Some(height as f32));

        Self {
            left: 100.0,
            top: 100.0,
            opacity: 1.0,
            buffer,
            renderer,
        }
    }

    pub fn set_text(&mut self, text: &mut TextResources, content: &str) {
        self.buffer.set_text(
            &mut text.font_system,
            content,
            Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
        );
        self.buffer.shape_until_scroll(&mut text.font_system, false);
    }

    pub fn resize(&mut self, text: &mut TextResources, width: u32, height: u32) {
        self.buffer
            .set_size(&mut text.font_system, Some(width as f32), Some(height as f32));
    }
}

impl SceneNode for TextNode {
    fn update(&mut self, _ctx: &UpdateContext) {
        // Animations
    }

    fn prepare(&mut self, ctx: &mut PrepareContext) {
        let alpha = (self.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let area = TextArea {
            buffer: &self.buffer,
            left: self.left,
            top: self.top,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: ctx.width as i32,
                bottom: ctx.height as i32,
            },
            default_color: Color::rgba(255, 255, 255, alpha),
            custom_glyphs: &[],
        };
        let text = &mut *ctx.text;
        if let Err(e) = self.renderer.prepare(
            ctx.device,
            ctx.queue,
            &mut text.font_system,
            &mut text.atlas,
            &text.viewport,
            [area],
            &mut text.swash_cache,
        ) {
            log::error!("Text prepare failed: {e}");
        }
    }

    fn render<'pass>(&'pass self, ctx: &mut RenderContext<'pass>) {
        if let Err(e) = self.renderer.render(ctx.text_atlas, ctx.viewport, ctx.render_pass) {
            log::error!("Text render failed: {e}");
        }
    }
}
