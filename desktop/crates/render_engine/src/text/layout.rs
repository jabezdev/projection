use cosmic_text::{
    Align, Attrs, AttrsList, CacheKey, Family, FontSystem, Hinting, ShapeLine, Shaping, Style,
    Weight, Wrap,
};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub family: String,
    pub size: f32,
    pub weight: u16,
    pub italic: bool,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub align: TextAlign,
}

impl TextStyle {
    pub fn new(family: &str, size: f32) -> Self {
        Self {
            family: family.to_string(),
            size,
            weight: 400,
            italic: false,
            line_height: size * 1.2,
            letter_spacing: 0.0,
            align: TextAlign::Left,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PlacedGlyph {
    pub key: CacheKey,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Default)]
pub struct TextLayout {
    pub glyphs: Vec<PlacedGlyph>,
    pub width: f32,
    pub height: f32,
    pub lines: usize,
}

pub fn layout(fonts: &Fonts, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
    fonts.with(|system| {
        let family = fonts.resolve(system, &style.family);
        layout_with(fonts, system, &family, text, style, width)
    })
}

pub(super) fn resolve_face(system: &FontSystem, family: &FamilyChoice, weight: u16, italic: bool) -> (String, Weight, Style) {
    let db = system.db();
    let name = db.family_name(&family.family()).to_string();
    let faces: Vec<_> = db.faces().filter(|f| f.families.iter().any(|(n, _)| *n == name)).collect();
    let style = if italic && faces.iter().any(|f| f.style == Style::Italic) {
        Style::Italic
    } else if italic && faces.iter().any(|f| f.style == Style::Oblique) {
        Style::Oblique
    } else {
        Style::Normal
    };
    let weights: Vec<u16> = faces.iter().filter(|f| f.style == style).map(|f| f.weight.0).collect();
    (name, Weight(css_weight(weight, &weights).unwrap_or(weight)), style)
}

pub(super) fn css_weight(desired: u16, available: &[u16]) -> Option<u16> {
    let mut sorted = available.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let heavier = |from: u16| sorted.iter().copied().filter(move |w| *w >= from);
    let lighter = |below: u16| sorted.iter().rev().copied().filter(move |w| *w < below);
    match desired {
        400..=500 => heavier(desired)
            .find(|w| *w <= 500)
            .or_else(|| lighter(desired).next())
            .or_else(|| heavier(desired).find(|w| *w > 500)),
        0..=399 => sorted.iter().rev().copied().find(|w| *w <= desired).or_else(|| heavier(desired).next()),
        _ => heavier(desired).next().or_else(|| lighter(desired).next()),
    }
}

pub(super) fn layout_with(fonts: &Fonts, system: &mut FontSystem, family: &FamilyChoice, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
    let (name, weight, font_style) = resolve_face(system, family, style.weight.clamp(1, 1000), style.italic);
    let attrs = Attrs::new().family(Family::Name(&name)).weight(weight).style(font_style);
    let icon_attrs = attrs.clone().family(Family::Name(ICON_FAMILY)).weight(Weight::NORMAL).style(Style::Normal);
    let align = match style.align {
        TextAlign::Left => Align::Left,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::Right,
    };
    let size = style.size.max(1.0);
    let spacing_em = style.letter_spacing / size;
    let mut out = TextLayout::default();
    let mut top = 0.0f32;
    for paragraph in text.replace("\r\n", "\n").split('\n') {
        let (expanded, icons) = expand_icons(paragraph);
        let paragraph = expanded.as_str();
        let mut attrs_list = AttrsList::new(&attrs);
        for range in &icons {
            attrs_list.add_span(range.clone(), &icon_attrs);
        }
        let mut shaped = ShapeLine::new(system, paragraph, &attrs_list, Shaping::Advanced, 8);
        if spacing_em != 0.0 {
            for glyph in shaped.spans.iter_mut().flat_map(|s| s.words.iter_mut()).flat_map(|w| w.glyphs.iter_mut()) {
                glyph.x_advance += spacing_em;
            }
        }
        let lines = shaped.layout(size, width, Wrap::WordOrGlyph, Some(align), None, Hinting::Disabled);
        if lines.is_empty() {
            top += if style.line_height > 0.0 { style.line_height } else { size * 1.2 };
            out.lines += 1;
            continue;
        }
        for mut line in lines {
            let kerned = kern_spaces(fonts, system, paragraph, &mut line.glyphs, size, style.align);
            line.w += kerned;
            let first = line.glyphs.iter().find(|g| !icons.iter().any(|r| r.contains(&g.start))).or(line.glyphs.first());
            let primary = first.and_then(|g| fonts.line_metrics(system, g.font_id, g.font_weight, size));
            let (ascent, descent, gap) = primary.unwrap_or((line.max_ascent, line.max_descent, 0.0));
            let content = ascent + descent;
            let line_height = if style.line_height > 0.0 { style.line_height } else { content + gap };
            let baseline = top + (line_height - content) / 2.0 + ascent;
            for glyph in &line.glyphs {
                let physical = glyph.physical((0.0, baseline), 1.0);
                out.glyphs.push(PlacedGlyph { key: physical.cache_key, x: physical.x, y: physical.y });
            }
            out.width = out.width.max(line.w);
            top += line_height;
            out.lines += 1;
        }
    }
    out.height = top;
    out
}
