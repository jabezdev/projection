use std::sync::{Arc, OnceLock};

use super::*;

fn fonts() -> Arc<Fonts> {
    static FONTS: OnceLock<Arc<Fonts>> = OnceLock::new();
    FONTS.get_or_init(Fonts::load).clone()
}

#[test]
fn lines_are_css_line_boxes_and_newlines_start_paragraphs() {
    let style = TextStyle { line_height: 40.0, ..TextStyle::new("sans-serif", 32.0) };
    let laid = layout(&fonts(), "one\ntwo\n\nfour", &style, None);
    assert_eq!(laid.lines, 4);
    assert_eq!(laid.height, 160.0);
}

#[test]
fn long_text_wraps_within_the_width() {
    let style = TextStyle::new("sans-serif", 32.0);
    let text = "Amazing grace how sweet the sound that saved a wretch like me";
    let laid = layout(&fonts(), text, &style, Some(400.0));
    assert!(laid.lines > 1);
    assert!(laid.width <= 400.0 + 0.5, "{}", laid.width);
}

#[test]
fn icon_codes_become_lucide_glyphs() {
    let (expanded, icons) = expand_icons("Pray :hand-heart: at 10:30: or :nope:");
    assert_eq!(icons.len(), 1);
    assert_eq!(&expanded[icons[0].clone()], "\u{e5bd}");
    assert!(expanded.starts_with("Pray \u{e5bd} at 10:30: or :nope:"), "{expanded:?}");
    let style = TextStyle::new("sans-serif", 48.0);
    let fonts = fonts();
    let laid = layout(&fonts, ":heart:", &style, None);
    assert_eq!(laid.glyphs.len(), 1);
    let font = laid.glyphs[0].key.font_id;
    let family = fonts.with(|system| system.db().face(font).map(|f| f.families[0].0.clone()));
    assert_eq!(family.as_deref(), Some("lucide"));
    assert!(laid.width > 30.0, "an icon is about one em wide: {}", laid.width);
}

#[test]
fn letter_spacing_widens_lines() {
    let plain = TextStyle::new("sans-serif", 32.0);
    let spaced = TextStyle { letter_spacing: 10.0, ..plain.clone() };
    let a = layout(&fonts(), "spacing", &plain, None);
    let b = layout(&fonts(), "spacing", &spaced, None);
    assert!((b.width - a.width - 70.0).abs() < 1.0, "{} vs {}", a.width, b.width);
}

#[test]
fn distance_fields_measure_from_the_glyph_edge() {
    let (field, w, h) = signed_distance(&[255; 16], 4, 4, 8);
    assert_eq!((w, h), (20, 20));
    let at = |x: usize, y: usize| (field[y * 20 + x] as f32 / 255.0 - 0.5) * 16.0;
    assert!(at(10, 10) < -1.0, "inside is negative: {}", at(10, 10));
    assert!((at(14, 10) - 2.5).abs() < 0.1, "2.5 px right of the edge: {}", at(14, 10));
    assert!((at(19, 10) - 7.5).abs() < 0.1, "{}", at(19, 10));
    assert!((at(0, 0) - 8.0).abs() < 0.1, "clamped at the spread: {}", at(0, 0));
}

#[test]
fn kerning_around_spaces_is_applied_and_keeps_centering() {
    let fonts = fonts();
    let style = TextStyle { align: TextAlign::Center, ..TextStyle::new("sans-serif", 64.0) };
    let laid = layout(&fonts, "T A", &style, Some(1000.0));
    let left = laid.glyphs.first().map(|g| g.x).unwrap_or_default() as f32;
    assert!((left - (1000.0 - laid.width) / 2.0).abs() < 2.0, "centered: left {left}, width {}", laid.width);
}

#[test]
fn weights_match_like_css() {
    assert_eq!(css_weight(800, &[400, 700]), Some(700));
    assert_eq!(css_weight(300, &[400, 700]), Some(400));
    assert_eq!(css_weight(450, &[300, 400, 700]), Some(400));
    assert_eq!(css_weight(500, &[300, 700]), Some(300));
    assert_eq!(css_weight(600, &[400, 700, 900]), Some(700));
    assert_eq!(css_weight(400, &[]), None);
}

#[test]
fn missing_families_fall_back_to_installed_aliases() {
    let fonts = fonts();
    fonts.with(|system| {
        assert_eq!(fonts.resolve(system, "sans-serif"), FamilyChoice::SansSerif);
        assert_ne!(fonts.resolve(system, "Definitely Not A Font"), FamilyChoice::Named("Definitely Not A Font".into()));
    });
}
