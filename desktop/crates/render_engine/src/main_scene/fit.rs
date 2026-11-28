use std::sync::Arc;

use crate::scene::LyricStyle;
use crate::text::{layout, Fonts, TextLayout, TextStyle};

use super::LyricGeometry;

pub const MIN_SCALE: f32 = 0.7;
const MIN_SCALE_TWO_COLUMNS: f32 = 0.55;
const CAPTION_SCALE: f32 = 0.42;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SlideText {
    pub text: String,
    pub parallel: String,
    pub caption: String,
}

pub struct Fitted {
    pub text: Arc<TextLayout>,
    pub parallel: Option<Arc<TextLayout>>,
    pub caption: Option<Arc<TextLayout>>,
    pub body_height: f32,
    pub height: f32,
    pub fits: bool,
}

impl LyricGeometry {
    pub fn room(&self) -> f32 {
        (self.area.h - 2.0 * self.border - 2.0 * self.padding_y).max(1.0)
    }

    pub fn column_gap(&self) -> f32 {
        3.0 * self.padding_x
    }

    pub fn column_width(&self) -> f32 {
        ((self.text_width - self.column_gap()) / 2.0).max(1.0)
    }

    pub fn caption_style(&self) -> TextStyle {
        let size = self.text_style.size * CAPTION_SCALE;
        TextStyle { size, line_height: size * 1.25, weight: 600, italic: false, letter_spacing: 0.0, ..self.text_style.clone() }
    }

    pub fn caption_gap(&self) -> f32 {
        self.text_style.size * 0.25
    }
}

fn scaled(style: &TextStyle, scale: f32) -> TextStyle {
    TextStyle { size: style.size * scale, line_height: style.line_height * scale, letter_spacing: style.letter_spacing * scale, ..style.clone() }
}

fn shown(text: &str, caps: bool) -> String {
    if caps { text.to_uppercase() } else { text.to_string() }
}

fn measure(fonts: &Fonts, geometry: &LyricGeometry, caps: bool, slide: &SlideText, scale: f32) -> Fitted {
    let style = scaled(&geometry.text_style, scale);
    let two = !slide.parallel.trim().is_empty();
    let width = if two { geometry.column_width() } else { geometry.text_width };
    let text = Arc::new(layout(fonts, &shown(&slide.text, caps), &style, Some(width)));
    let parallel = two.then(|| Arc::new(layout(fonts, &shown(&slide.parallel, caps), &style, Some(width))));
    let caption = (!slide.caption.trim().is_empty()).then(|| Arc::new(layout(fonts, &slide.caption, &geometry.caption_style(), Some(geometry.text_width))));
    let body_height = text.height.max(parallel.as_ref().map_or(0.0, |p| p.height));
    let height = body_height + caption.as_ref().map_or(0.0, |c| geometry.caption_gap() + c.height);
    let fits = height <= geometry.room() + 0.5;
    Fitted { text, parallel, caption, body_height, height, fits }
}

pub fn fit(fonts: &Fonts, geometry: &LyricGeometry, caps: bool, slide: &SlideText) -> Fitted {
    let full = measure(fonts, geometry, caps, slide, 1.0);
    if full.fits || slide.text.trim().is_empty() {
        return full;
    }
    let min = if slide.parallel.trim().is_empty() { MIN_SCALE } else { MIN_SCALE_TWO_COLUMNS };
    let smallest = measure(fonts, geometry, caps, slide, min);
    if !smallest.fits {
        let mut scale = min;
        let mut last = smallest;
        for _ in 0..4 {
            scale *= (geometry.room() / last.height).sqrt().min(0.95);
            last = measure(fonts, geometry, caps, slide, scale);
            if last.fits {
                break;
            }
        }
        return Fitted { fits: false, ..last };
    }
    let (mut low, mut high, mut best) = (min, 1.0f32, smallest);
    for _ in 0..6 {
        let mid = (low + high) / 2.0;
        let candidate = measure(fonts, geometry, caps, slide, mid);
        if candidate.fits {
            low = mid;
            best = candidate;
        } else {
            high = mid;
        }
    }
    best
}

pub fn split_to_fit(fonts: &Fonts, style: &LyricStyle, size: (u32, u32), slide: &SlideText) -> Vec<SlideText> {
    let geometry = LyricGeometry::new(style, size);
    let fits = |text: &str, parallel: &str| fit(fonts, &geometry, style.caps, &SlideText { text: text.into(), parallel: parallel.into(), caption: slide.caption.clone() }).fits;
    if fits(&slide.text, &slide.parallel) {
        return vec![slide.clone()];
    }
    if slide.parallel.trim().is_empty() {
        return pieces(&slide.text, &|t| fits(t, ""), 0).into_iter().map(|text| SlideText { text, ..slide.clone() }).collect();
    }
    let needed = pieces(&slide.text, &|t| fits(t, ""), 0).len().max(pieces(&slide.parallel, &|t| fits("", t), 0).len()).max(2);
    let (texts, parallels) = (split_even(&slide.text, needed), split_even(&slide.parallel, needed));
    texts.into_iter().zip(parallels).map(|(text, parallel)| SlideText { text, parallel, caption: slide.caption.clone() }).collect()
}

fn pieces(text: &str, fits: &dyn Fn(&str) -> bool, depth: usize) -> Vec<String> {
    if depth > 6 || fits(text) {
        return vec![text.to_string()];
    }
    match halves(text) {
        Some((a, b)) => {
            let mut out = pieces(&a, fits, depth + 1);
            out.extend(pieces(&b, fits, depth + 1));
            out
        }
        None => vec![text.to_string()],
    }
}

fn halves(text: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() >= 2 {
        let total: usize = lines.iter().map(|l| l.len()).sum();
        let (mut acc, mut cut) = (0, 1);
        let mut best = usize::MAX;
        for i in 1..lines.len() {
            acc += lines[i - 1].len();
            let off = acc.abs_diff(total - acc);
            if off < best {
                best = off;
                cut = i;
            }
        }
        return Some((lines[..cut].join("\n"), lines[cut..].join("\n")));
    }
    let text = text.trim();
    let middle = text.len() / 2;
    let boundary = |marks: &[&str]| {
        marks
            .iter()
            .flat_map(|m| text.match_indices(m).map(move |(i, _)| i + m.len()))
            .filter(|&i| i > text.len() / 5 && i < text.len() * 4 / 5)
            .min_by_key(|&i| i.abs_diff(middle))
    };
    let cut = boundary(&[". ", "? ", "! ", "; ", ": "]).or_else(|| boundary(&[", "])).or_else(|| boundary(&[" "]))?;
    let (a, b) = text.split_at(cut);
    Some((a.trim().to_string(), b.trim().to_string())).filter(|(a, b)| !a.is_empty() && !b.is_empty())
}

fn split_even(text: &str, parts: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let total: usize = words.iter().map(|w| w.len() + 1).sum();
    let mut out = vec![String::new(); parts];
    let mut acc = 0;
    for word in words {
        let index = ((acc * parts) / total.max(1)).min(parts - 1);
        if !out[index].is_empty() {
            out[index].push(' ');
        }
        out[index].push_str(word);
        acc += word.len() + 1;
    }
    out
}
