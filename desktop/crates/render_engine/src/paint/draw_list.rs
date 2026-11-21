use std::sync::Arc;

use crate::text::{TextLayout, MAX_SPREAD};


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

/// Effects drawn under text, from the glyphs' distance fields. Sizes in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextEffects {
    /// Color (opacity included), offset and blur radius.
    pub shadow: Option<(Rgba, f32, f32, f32)>,
    /// Color (opacity included) and radius.
    pub glow: Option<(Rgba, f32)>,
    /// Color and width, outside the glyph.
    pub outline: Option<(Rgba, f32)>,
}

impl TextEffects {
    pub fn is_empty(&self) -> bool {
        self.shadow.is_none() && self.glow.is_none() && self.outline.is_none()
    }

    /// How far from the glyph edge the effects reach: the distance field's spread.
    pub(super) fn spread(&self) -> u32 {
        let shadow = self.shadow.map_or(0.0, |(_, _, _, blur)| blur);
        let glow = self.glow.map_or(0.0, |(_, size)| size);
        let outline = self.outline.map_or(0.0, |(_, width)| width);
        (shadow.max(glow).max(outline) + 2.0).ceil().clamp(2.0, MAX_SPREAD as f32) as u32
    }

    /// Fades every effect with the text.
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        if let Some((color, ..)) = self.shadow.as_mut() { color[3] *= alpha; }
        if let Some((color, _)) = self.glow.as_mut() { color[3] *= alpha; }
        if let Some((color, _)) = self.outline.as_mut() { color[3] *= alpha; }
        self
    }
}

pub enum Item {
    Fill { rect: Rect, color: Rgba },
    Image { rect: Rect, uv: [f32; 4], texture: Arc<GpuTexture>, opacity: f32 },
    /// `layout` drawn with its top-left at (x, y), rotated by `angle` radians
    /// (clockwise on screen) around that point, over its `effects`.
    Text { layout: Arc<TextLayout>, x: f32, y: f32, color: Rgba, angle: f32, effects: TextEffects },
}

pub(super) struct Entry {
    pub(super) item: Item,
    pub(super) clip: Option<Rect>,
}

/// One frame's drawing, back to front.
#[derive(Default)]
pub struct DrawList {
    pub(super) entries: Vec<Entry>,
    pub(super) clip: Option<Rect>,
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
        self.text_with_effects(layout, x, y, color, TextEffects::default());
    }

    pub fn text_with_effects(&mut self, layout: &Arc<TextLayout>, x: f32, y: f32, color: Rgba, effects: TextEffects) {
        if (color[3] > 0.0 || !effects.is_empty()) && !layout.glyphs.is_empty() {
            self.push(Item::Text { layout: layout.clone(), x, y, color, angle: 0.0, effects });
        }
    }

    pub fn image(&mut self, rect: Rect, uv: [f32; 4], texture: &Arc<GpuTexture>, opacity: f32) {
        if opacity > 0.0 {
            self.push(Item::Image { rect, uv, texture: texture.clone(), opacity });
        }
    }
}
