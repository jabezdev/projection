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

fn fonts() -> std::sync::Arc<crate::text::Fonts> {
    static FONTS: std::sync::OnceLock<std::sync::Arc<crate::text::Fonts>> = std::sync::OnceLock::new();
    FONTS.get_or_init(crate::text::Fonts::load).clone()
}

fn slide(text: &str) -> SlideText {
    SlideText { text: text.into(), ..Default::default() }
}

#[test]
fn short_slides_keep_their_size_and_long_ones_shrink_before_splitting() {
    let style = LyricStyle { font: "sans-serif".into(), ..LyricStyle::default() };
    let g = LyricGeometry::new(&style, (1920, 1080));
    let short = fit::fit(&fonts(), &g, false, &slide("Amazing grace"));
    assert!(short.fits && (short.text.lines == 1));
    let five = "one line here\ntwo line here\nthree line here\nfour line here\nfive line here\nsix line here";
    let shrunk = fit::fit(&fonts(), &g, false, &slide(five));
    assert!(shrunk.fits, "six short lines fit once shrunk: {}", shrunk.height);
    assert_eq!(split_to_fit(&fonts(), &style, (1920, 1080), &slide(five)).len(), 1);
    let long = (1..=14).map(|i| format!("line number {i}")).collect::<Vec<_>>().join("\n");
    let parts = split_to_fit(&fonts(), &style, (1920, 1080), &slide(&long));
    assert!(parts.len() >= 2, "{parts:?}");
    assert_eq!(parts.iter().map(|p| p.text.lines().count()).sum::<usize>(), 14);
    assert!(parts.iter().all(|p| fit::fit(&fonts(), &g, false, p).fits));
}

#[test]
fn a_long_verse_splits_at_a_sentence_and_keeps_its_caption() {
    let style = LyricStyle { font: "sans-serif".into(), ..LyricStyle::default() };
    let verse = "And the LORD God formed man of the dust of the ground, and breathed into his nostrils the breath of life. And man became a living soul, and the LORD God planted a garden eastward in Eden; and there he put the man whom he had formed, and out of the ground made the LORD God to grow every tree that is pleasant to the sight, and good for food.";
    let parts = split_to_fit(&fonts(), &style, (1920, 1080), &SlideText { text: verse.into(), parallel: String::new(), caption: "Genesis 2:7 · KJV".into() });
    assert!(parts.len() >= 2, "{parts:?}");
    assert!(parts.iter().all(|p| p.caption == "Genesis 2:7 · KJV"));
    let two = split_to_fit(&fonts(), &style, (1920, 1080), &SlideText { text: verse.into(), parallel: verse.into(), caption: String::new() });
    assert!(two.len() >= 2 && two.iter().all(|p| !p.parallel.is_empty()));
}
