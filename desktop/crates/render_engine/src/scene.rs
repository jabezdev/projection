//! What each output shows, in the operator's units. Outputs convert these to
//! pixels for their own size: `cq` below is 1% of the output width, like the
//! webview's `cqw`.

use std::time::{Duration, Instant};

use crate::frame::VideoKey;
use crate::paint::Rgba;
use crate::text::TextAlign;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Live,
    Clear,
    Black,
    Logo,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MediaSource {
    Image(String),
    Video(VideoKey),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaLayer {
    pub source: MediaSource,
    /// Crop to fill (`true`) or letterbox.
    pub cover: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Vertical {
    Top,
    Center,
    #[default]
    Bottom,
}

/// Main lyric styling (decision 8's reduced set).
#[derive(Debug, Clone, PartialEq)]
pub struct LyricStyle {
    pub font: String,
    /// Font size in `cq`.
    pub size: f32,
    pub weight: u16,
    pub italic: bool,
    pub caps: bool,
    pub color: Rgba,
    pub align: TextAlign,
    pub vertical: Vertical,
    /// Side margins in `cq`; top/bottom margins are `margin_y * 9/16` `cq`.
    pub margin_x: f32,
    pub margin_y: f32,
    /// Multiple of the font size.
    pub line_height: f32,
    /// Sixteenths of a `cq`.
    pub letter_spacing: f32,
    /// Border width in sixteenths of a `cq`.
    pub border: f32,
    pub border_color: Rgba,
    /// Box fill, opacity included.
    pub box_color: Rgba,
}

impl Default for LyricStyle {
    fn default() -> Self {
        Self {
            font: "Arial".into(),
            size: 6.0,
            weight: 800,
            italic: false,
            caps: false,
            color: [1.0, 1.0, 1.0, 1.0],
            align: TextAlign::Center,
            vertical: Vertical::Bottom,
            margin_x: 5.0,
            margin_y: 10.0,
            line_height: 1.2,
            letter_spacing: 0.0,
            border: 0.0,
            border_color: [1.0, 1.0, 1.0, 1.0],
            box_color: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MainScene {
    /// The cued text; shown only while `display` is `Live`.
    pub text: String,
    pub display: Display,
    pub background: Option<MediaLayer>,
    /// Shown letterboxed within the margins under `Display::Logo`.
    pub logo: Option<MediaSource>,
    pub style: LyricStyle,
    pub transition_ms: f32,
}

/// One arrangement occurrence (or one non-song item) in the teleprompter.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfidenceGroup {
    /// Stable identity: `(item, occurrence)`.
    pub key: String,
    pub code: String,
    pub title: String,
    pub color: Rgba,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConfidenceScene {
    pub groups: Vec<ConfidenceGroup>,
    /// Active `(group, line within the group)`.
    pub active: Option<(usize, usize)>,
    /// Lyric size in `cq`.
    pub size: f32,
    /// When set, the foreground video replaces the teleprompter.
    pub video: Option<VideoKey>,
    pub transition_ms: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Scene {
    Main(MainScene),
    Confidence(ConfidenceScene),
}

/// What an output's next frame needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Activity {
    /// Drawing now would differ from the last frame.
    pub redraw: bool,
    /// No transition or pending media remains (playing video is settled).
    pub settled: bool,
    /// Sum of the generations of visible video textures.
    pub generation: u64,
}

impl Activity {
    pub fn and(self, other: Activity) -> Activity {
        Activity {
            redraw: self.redraw || other.redraw,
            settled: self.settled && other.settled,
            generation: self.generation + other.generation,
        }
    }
}

/// A linear opacity transition that, like a CSS transition, restarts from
/// the current value when retargeted.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fade {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

impl Fade {
    pub fn fixed(value: f32, now: Instant) -> Self {
        Self { from: value, to: value, start: now, duration: Duration::ZERO }
    }

    pub fn value(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.start);
        if elapsed >= self.duration {
            return self.to;
        }
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        self.from + (self.to - self.from) * t
    }

    pub fn retarget(&mut self, to: f32, now: Instant, duration: Duration) {
        if to == self.to {
            return;
        }
        *self = Self { from: self.value(now), to, start: now, duration };
    }

    pub fn running(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) < self.duration && self.from != self.to
    }
}

pub(crate) fn millis(ms: f32) -> Duration {
    Duration::from_secs_f32(ms.max(0.0) / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retargeting_starts_from_the_current_value() {
        let t0 = Instant::now();
        let mut fade = Fade::fixed(0.0, t0);
        fade.retarget(1.0, t0, Duration::from_millis(100));
        assert!((fade.value(t0 + Duration::from_millis(50)) - 0.5).abs() < 1e-3);
        fade.retarget(0.0, t0 + Duration::from_millis(50), Duration::from_millis(100));
        assert!((fade.value(t0 + Duration::from_millis(100)) - 0.25).abs() < 1e-3);
        assert_eq!(fade.value(t0 + Duration::from_millis(200)), 0.0);
        assert!(!fade.running(t0 + Duration::from_millis(200)));
    }
}
