

use crate::scene::ConfidenceStyle;

use super::*;

#[test]
fn metrics_match_the_parity_reference_at_1080p() {
    // Values from the parity references in tools/parity.
    let m = Metrics::new((1920, 1080), &ConfidenceStyle::default());
    assert!((m.scroll_top() - 116.2).abs() < 0.05);
    assert!((m.tape_left() - 250.6).abs() < 0.05);
    assert!((m.card_width() - 345.6).abs() < 0.05);
    assert!((m.text_x() - 288.0).abs() < 0.1);
    assert!((m.text_width() - 1593.6).abs() < 0.1);
}

#[test]
fn saturate_and_mix_follow_css() {
    let gray = saturate([1.0, 0.0, 0.0, 1.0], 0.0);
    assert!((gray[0] - 0.213).abs() < 1e-6 && (gray[1] - 0.213).abs() < 1e-6);
    assert_eq!(mix([1.0, 1.0, 1.0, 1.0], 0.25, [0.0, 0.0, 0.0, 1.0])[0], 0.25);
}
