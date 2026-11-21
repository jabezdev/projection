use std::time::Instant;

use crate::scene::{millis, Fade, Transition};

use super::*;

pub(super) struct Lyric {
    pub(super) text: String,
    /// 0 out, 1 in place: opacity when fading, position when sliding.
    pub(super) opacity: Fade,
}

/// Port of `LyricLayers`: the new text comes in while the previous goes out
/// from wherever it was, both on one clock; the box persists across them.
#[derive(Default)]
pub(super) struct Lyrics {
    pub(super) current: Option<Lyric>,
    pub(super) previous: Option<Lyric>,
    pub(super) drop_previous_at: Option<Instant>,
}

impl Lyrics {
    pub(super) fn set(&mut self, text: &str, now: Instant, fade_ms: f32, transition: Transition) {
        if self.current.as_ref().map_or("", |c| c.text.as_str()) == text {
            return;
        }
        let duration = millis(duration(transition.kind, fade_ms));
        self.previous = self.current.take().map(|mut lyric| {
            lyric.opacity.retarget_eased(0.0, now, duration, transition.easing);
            lyric
        });
        let mut opacity = Fade::fixed(0.0, now);
        opacity.retarget_eased(1.0, now, duration, transition.easing);
        self.current = Some(Lyric { text: text.to_string(), opacity });
        self.drop_previous_at = Some(now + duration);
    }

    pub(super) fn update(&mut self, now: Instant) {
        if self.drop_previous_at.is_some_and(|at| now >= at) {
            self.previous = None;
            self.drop_previous_at = None;
        }
    }

    /// Visible texts with their progress, and whether each is the incoming one.
    pub(super) fn visible(&self) -> impl Iterator<Item = (&Lyric, bool)> {
        self.previous.iter().map(|l| (l, false)).chain(self.current.iter().map(|l| (l, true))).filter(|(l, _)| !l.text.is_empty())
    }

    pub(super) fn running(&self, now: Instant) -> bool {
        self.drop_previous_at.is_some() || self.current.as_ref().is_some_and(|c| c.opacity.running(now))
    }
}
