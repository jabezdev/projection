use std::time::{Duration, Instant};




/// Browser-like smooth scrolling: eases from wherever it is to the target.
#[derive(Debug, Clone, Copy)]
pub(super) struct Smooth {
    pub(super) from: f32,
    pub(super) to: f32,
    pub(super) start: Instant,
    pub(super) duration: Duration,
}

impl Smooth {
    pub(super) fn at(value: f32, now: Instant) -> Self {
        Self { from: value, to: value, start: now, duration: Duration::ZERO }
    }

    pub(super) fn value(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.start);
        if elapsed >= self.duration {
            return self.to;
        }
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        let eased = if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 };
        self.from + (self.to - self.from) * eased
    }

    pub(super) fn scroll_to(&mut self, to: f32, now: Instant, animate: bool) {
        if (to - self.to).abs() < 0.5 {
            return;
        }
        let from = self.value(now);
        let distance = (to - from).abs();
        let duration = if animate { Duration::from_millis((250.0 + distance * 0.3).min(650.0) as u64) } else { Duration::ZERO };
        *self = Self { from, to, start: now, duration };
    }

    pub(super) fn running(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) < self.duration
    }
}
