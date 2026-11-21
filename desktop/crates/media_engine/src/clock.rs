use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub fn now_us() -> i64 {
    static BASE: OnceLock<Instant> = OnceLock::new();
    BASE.get_or_init(Instant::now).elapsed().as_micros() as i64
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub epoch: u64,
    pub position_us: i64,
    pub rate: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AudioTiming {
    pub epoch: u64,
    pub frames_before_block: u64,
    pub block_frames: u64,
    pub block_heard_at_us: i64,
}

pub fn audio_position(anchor: Anchor, timing: AudioTiming, device_rate: u32, now_us: i64) -> i64 {
    if timing.epoch != anchor.epoch || device_rate == 0 {
        return anchor.position_us;
    }
    let into_block = ((now_us - timing.block_heard_at_us).max(0) as f64 * device_rate as f64 / 1e6)
        .min(timing.block_frames as f64);
    let frames = timing.frames_before_block as f64 + into_block;
    anchor.position_us + (frames * 1e6 / device_rate as f64 * anchor.rate) as i64
}

#[derive(Default)]
pub struct AudioTimingCell(Mutex<AudioTiming>);

impl AudioTimingCell {
    pub fn publish(&self, timing: AudioTiming) {
        if let Ok(mut slot) = self.0.try_lock() {
            *slot = timing;
        }
    }

    pub fn get(&self) -> AudioTiming {
        *self.0.lock().expect("audio timing lock poisoned")
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonotonicClock {
    position_us: i64,
    since_us: Option<i64>,
    rate: f64,
}

impl MonotonicClock {
    pub fn new(position_us: i64, rate: f64) -> Self {
        Self { position_us, since_us: None, rate }
    }

    pub fn position(&self, now_us: i64) -> i64 {
        match self.since_us {
            Some(since) => self.position_us + ((now_us - since).max(0) as f64 * self.rate) as i64,
            None => self.position_us,
        }
    }

    pub fn running(&self) -> bool {
        self.since_us.is_some()
    }

    pub fn set_running(&mut self, running: bool, now_us: i64) {
        if running == self.running() {
            return;
        }
        self.position_us = self.position(now_us);
        self.since_us = running.then_some(now_us);
    }

    pub fn reset(&mut self, position_us: i64, rate: f64, now_us: i64) {
        self.position_us = position_us;
        self.rate = rate;
        if self.since_us.is_some() {
            self.since_us = Some(now_us);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn anchor(rate: f64) -> Anchor {
        Anchor { epoch: 3, position_us: 10_000_000, rate }
    }

    #[test]
    fn audio_position_counts_frames_the_device_has_been_handed() {
        let timing = AudioTiming { epoch: 3, frames_before_block: 48_000, block_frames: 480, block_heard_at_us: 5_000_000 };
        assert_eq!(audio_position(anchor(1.0), timing, RATE, 4_000_000), 11_000_000, "block not yet audible");
        assert_eq!(audio_position(anchor(1.0), timing, RATE, 5_005_000), 11_005_000, "halfway through the block");
        assert_eq!(audio_position(anchor(1.0), timing, RATE, 9_000_000), 11_010_000, "never past what was handed over");
    }

    #[test]
    fn audio_position_scales_with_playback_rate() {
        let timing = AudioTiming { epoch: 3, frames_before_block: 48_000, block_frames: 0, block_heard_at_us: 0 };
        assert_eq!(audio_position(anchor(2.0), timing, RATE, 0), 12_000_000);
        assert_eq!(audio_position(anchor(0.5), timing, RATE, 0), 10_500_000);
    }

    #[test]
    fn audio_position_holds_at_the_anchor_until_the_flush_is_acknowledged() {
        let stale = AudioTiming { epoch: 2, frames_before_block: 480_000, block_frames: 480, block_heard_at_us: 0 };
        assert_eq!(audio_position(anchor(1.0), stale, RATE, 1_000_000), 10_000_000);
    }

    #[test]
    fn monotonic_clock_freezes_while_paused_and_resumes_without_jumping() {
        let mut clock = MonotonicClock::new(1_000_000, 1.0);
        assert_eq!(clock.position(50_000_000), 1_000_000);
        clock.set_running(true, 10_000_000);
        assert_eq!(clock.position(10_500_000), 1_500_000);
        clock.set_running(false, 11_000_000);
        assert_eq!(clock.position(20_000_000), 2_000_000);
        clock.set_running(true, 30_000_000);
        assert_eq!(clock.position(30_250_000), 2_250_000);
    }

    #[test]
    fn monotonic_clock_reset_applies_a_new_rate_from_the_new_position() {
        let mut clock = MonotonicClock::new(0, 1.0);
        clock.set_running(true, 0);
        clock.reset(5_000_000, 1.5, 1_000_000);
        assert_eq!(clock.position(2_000_000), 6_500_000);
    }
}
