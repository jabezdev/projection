
use cosmic_text::FontSystem;

use super::*;

impl Fonts {
    /// Kerning lost between words: cosmic-text shapes each word on its own,
    /// so pairs with a space (`T` then space, space then `A`) never apply.
    /// The difference, in em, between shaping `before after` whole and in pieces.
    pub(super) fn space_kerning(&self, system: &mut FontSystem, id: cosmic_text::fontdb::ID, weight: cosmic_text::fontdb::Weight, before: char, after: char) -> f32 {
        if let Some(kern) = self.space_kerning.lock().expect("kerning poisoned").get(&(id, before, after)) {
            return *kern;
        }
        let kern = system.get_font(id, weight).map_or(0.0, |font| {
            let units = font.as_swash().metrics(&[]).units_per_em.max(1) as f32;
            let advance = |text: &str| {
                let mut buffer = harfrust::UnicodeBuffer::new();
                buffer.push_str(text);
                buffer.guess_segment_properties();
                font.shaper().shape(buffer, &[]).glyph_positions().iter().map(|p| p.x_advance).sum::<i32>() as f32
            };
            let whole = advance(&format!("{before} {after}"));
            let pieces = advance(&before.to_string()) + advance(" ") + advance(&after.to_string());
            (whole - pieces) / units
        });
        self.space_kerning.lock().expect("kerning poisoned").insert((id, before, after), kern);
        kern
    }
}

/// Applies kerning around the spaces of one left-to-right line in place,
/// keeping its alignment. Returns the change in width.
pub(super) fn kern_spaces(fonts: &Fonts, system: &mut FontSystem, paragraph: &str, glyphs: &mut [cosmic_text::LayoutGlyph], size: f32, align: TextAlign) -> f32 {
    if glyphs.iter().any(|g| g.level.is_rtl()) {
        return 0.0;
    }
    let text = |g: &cosmic_text::LayoutGlyph| paragraph.get(g.start..g.end).unwrap_or("");
    let mut shift = 0.0;
    for i in 0..glyphs.len() {
        glyphs[i].x += shift;
        if i == 0 || i + 1 >= glyphs.len() || text(&glyphs[i]) != " " {
            continue;
        }
        let (before, after) = (&glyphs[i - 1], &glyphs[i + 1]);
        if before.font_id != glyphs[i].font_id || after.font_id != glyphs[i].font_id {
            continue;
        }
        let (Some(b), Some(a)) = (text(before).chars().next_back(), text(after).chars().next()) else { continue };
        if b.is_whitespace() || a.is_whitespace() {
            continue;
        }
        shift += fonts.space_kerning(system, glyphs[i].font_id, glyphs[i].font_weight, b, a) * size;
    }
    let realign = match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => -shift / 2.0,
        TextAlign::Right => -shift,
    };
    if realign != 0.0 {
        for glyph in glyphs.iter_mut() {
            glyph.x += realign;
        }
    }
    shift
}
