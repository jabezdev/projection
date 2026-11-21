use crate::scene::{LyricStyle, Vertical};

use super::*;

#[test]
fn geometry_matches_the_parity_reference_at_1080p() {
    let g = LyricGeometry::new(&LyricStyle::default(), (1920, 1080));
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(close(g.area.x, 96.0) && close(g.area.y, 108.0) && close(g.area.w, 1728.0) && close(g.area.h, 864.0), "{:?}", g.area);
    assert!((g.text_style.size - 115.2).abs() < 1e-3);
    assert!((g.text_style.line_height - 138.24).abs() < 1e-3);
    let rect = g.text_box(Vertical::Bottom, 3.0 * 138.24);
    assert!((rect.y - 541.95).abs() < 0.05, "{rect:?}");
    assert!((rect.h - 430.05).abs() < 0.05, "{rect:?}");
}

#[test]
fn geometry_scales_with_width_on_other_aspects() {
    let g = LyricGeometry::new(&LyricStyle { border: 16.0, ..LyricStyle::default() }, (1440, 1080));
    let cq = 14.4;
    assert_eq!(g.area.x, 5.0 * cq);
    assert_eq!(g.area.y, 10.0 * 9.0 / 16.0 * cq);
    assert_eq!(g.border, cq);
    assert_eq!(g.text_width, 1440.0 - 10.0 * cq - 2.0 * cq - 2.0 * cq);
}
