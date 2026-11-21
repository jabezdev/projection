

use super::*;

#[test]
fn parses_css_hex_colors() {
    assert_eq!(css_color("#fff", [0.0; 4]), [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(css_color("#000000", [1.0; 4]), [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(css_color("#ff000080", [0.0; 4])[3], 128.0 / 255.0);
    assert_eq!(css_color("red", [0.5; 4]), [0.5; 4]);
}

#[test]
fn cover_crops_and_contain_letterboxes() {
    let area = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    let (rect, uv) = fit((1080, 1080), area, true);
    assert_eq!(rect, area);
    assert!((uv[1] - (1.0 - 1080.0 / 1920.0) / 2.0).abs() < 1e-6);
    let (rect, uv) = fit((1080, 1080), area, false);
    assert_eq!((rect.x, rect.w, rect.h), (420.0, 1080.0, 1080.0));
    assert_eq!(uv, [0.0, 0.0, 1.0, 1.0]);
}
