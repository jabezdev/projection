use super::*;

pub fn css_color(value: &str, fallback: Rgba) -> Rgba {
    let hex = value.trim().trim_start_matches('#');
    let digits: Option<Vec<u8>> = match hex.len() {
        3 => hex.chars().map(|c| c.to_digit(16).map(|d| (d * 17) as u8)).collect(),
        6 | 8 => (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect(),
        _ => None,
    };
    match digits.as_deref() {
        Some([r, g, b]) => [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0],
        Some([r, g, b, a]) => [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, *a as f32 / 255.0],
        _ => fallback,
    }
}

pub fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    [color[0], color[1], color[2], color[3] * alpha]
}

pub fn fit(content: (u32, u32), area: Rect, cover: bool) -> (Rect, [f32; 4]) {
    let (cw, ch) = (content.0.max(1) as f32, content.1.max(1) as f32);
    let scale = if cover { (area.w / cw).max(area.h / ch) } else { (area.w / cw).min(area.h / ch) };
    let (w, h) = (cw * scale, ch * scale);
    if cover {
        let (u, v) = ((w - area.w) / w / 2.0, (h - area.h) / h / 2.0);
        (area, [u, v, 1.0 - u, 1.0 - v])
    } else {
        (Rect::new(area.x + (area.w - w) / 2.0, area.y + (area.h - h) / 2.0, w, h), [0.0, 0.0, 1.0, 1.0])
    }
}
