

use crate::paint::{css_color, Rgba};


pub(super) const UI_FONT: &str = "Segoe UI";
pub(super) const MONO_FONT: &str = "monospace";

pub(super) fn rgb(hex: &str) -> Rgba {
    css_color(hex, [1.0, 0.0, 1.0, 1.0])
}

/// CSS `saturate()`.
pub(super) fn saturate(c: Rgba, s: f32) -> Rgba {
    let [r, g, b, a] = c;
    [
        (0.213 + 0.787 * s) * r + (0.715 - 0.715 * s) * g + (0.072 - 0.072 * s) * b,
        (0.213 - 0.213 * s) * r + (0.715 + 0.285 * s) * g + (0.072 - 0.072 * s) * b,
        (0.213 - 0.213 * s) * r + (0.715 - 0.715 * s) * g + (0.072 + 0.928 * s) * b,
        a,
    ]
}

/// CSS `color-mix(in srgb, a p, b)`.
pub(super) fn mix(a: Rgba, p: f32, b: Rgba) -> Rgba {
    [a[0] * p + b[0] * (1.0 - p), a[1] * p + b[1] * (1.0 - p), a[2] * p + b[2] * (1.0 - p), 1.0]
}
