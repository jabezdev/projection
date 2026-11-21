use crate::paint::{Rect, TextEffects};
use crate::scene::{LyricStyle, Vertical};
use crate::text::TextStyle;

#[derive(Debug, Clone, PartialEq)]
pub struct LyricGeometry {
    pub area: Rect,
    pub border: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub text_style: TextStyle,
    pub text_width: f32,
    pub effects: TextEffects,
}

impl LyricGeometry {
    pub fn new(style: &LyricStyle, size: (u32, u32)) -> Self {
        let (w, h) = (size.0 as f32, size.1 as f32);
        let cq = w / 100.0;
        let (pad_x, pad_y) = (style.margin_x * cq, style.margin_y * 9.0 / 16.0 * cq);
        let area = Rect::new(pad_x, pad_y, (w - 2.0 * pad_x).max(0.0), (h - 2.0 * pad_y).max(0.0));
        let border = style.border / 16.0 * cq;
        let (padding_x, padding_y) = (cq, 0.4 * cq);
        let font_size = style.size * cq;
        let text_style = TextStyle {
            family: style.font.clone(),
            size: font_size,
            weight: style.weight,
            italic: style.italic,
            line_height: style.line_height * font_size,
            letter_spacing: style.letter_spacing / 16.0 * cq,
            align: style.align,
        };
        let text_width = (area.w - 2.0 * border - 2.0 * padding_x).max(1.0);
        let px = cq / 16.0;
        let effects = TextEffects {
            shadow: style.shadow.map(|s| (s.color, s.x * px, s.y * px, s.blur * px)),
            glow: style.glow.map(|g| (g.color, g.size * px)),
            outline: style.outline.map(|o| (o.color, o.size * px)),
        };
        Self { area, border, padding_x, padding_y, text_style, text_width, effects }
    }

    pub fn text_box(&self, vertical: Vertical, text_height: f32) -> Rect {
        let height = text_height + 2.0 * self.padding_y + 2.0 * self.border;
        let y = match vertical {
            Vertical::Top => self.area.y,
            Vertical::Center => self.area.y + (self.area.h - height) / 2.0,
            Vertical::Bottom => self.area.bottom() - height,
        };
        Rect::new(self.area.x, y, self.area.w, height)
    }
}
