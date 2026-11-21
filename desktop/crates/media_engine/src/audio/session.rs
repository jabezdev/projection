use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::clock::{audio_position, Anchor, AudioTiming, AudioTimingCell};
use crate::ring::SampleRing;

pub(super) const RING_SECONDS: f64 = 1.0;

pub struct SessionAudio {
    pub ring: SampleRing,
    pub channels: u16,
    pub device_rate: u32,
    running: AtomicBool,
    gain: AtomicU32,
    pub drained: AtomicBool,
    anchor: Mutex<Anchor>,
    timing: AudioTimingCell,
    frames_since_flush: AtomicU64,
    epoch_seen: AtomicU64,
    pub underruns: AtomicU64,
    pub(super) attached: Mutex<bool>,
}

impl SessionAudio {
    pub fn new(device_rate: u32, channels: u16) -> Self {
        let capacity = (device_rate as f64 * channels as f64 * RING_SECONDS) as usize;
        Self {
            ring: SampleRing::new(capacity),
            channels,
            device_rate,
            running: AtomicBool::new(false),
            gain: AtomicU32::new(1f32.to_bits()),
            drained: AtomicBool::new(false),
            anchor: Mutex::new(Anchor { epoch: 0, position_us: 0, rate: 1.0 }),
            timing: AudioTimingCell::default(),
            frames_since_flush: AtomicU64::new(0),
            epoch_seen: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            attached: Mutex::new(false),
        }
    }

    pub fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::Release);
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain.store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Release);
    }

    pub fn restart(&self, position_us: i64, rate: f64) {
        let epoch = {
            let attached = self.attached.lock().expect("audio attached poisoned");
            if *attached {
                drop(attached);
                self.ring.flush(Duration::from_millis(250))
            } else {
                self.ring.flush_unconsumed()
            }
        };
        self.drained.store(false, Ordering::Release);
        *self.anchor.lock().expect("audio anchor poisoned") = Anchor { epoch, position_us, rate };
    }

    pub fn position(&self, now_us: i64) -> i64 {
        let anchor = *self.anchor.lock().expect("audio anchor poisoned");
        audio_position(anchor, self.timing.get(), self.device_rate, now_us)
    }

    pub fn render(&self, out: &mut [f32], heard_at_us: i64) {
        self.ring.service_flush();
        let epoch = self.ring.epoch();
        if self.epoch_seen.swap(epoch, Ordering::AcqRel) != epoch {
            self.frames_since_flush.store(0, Ordering::Release);
        }
        let read = if self.running.load(Ordering::Acquire) { self.ring.pop(out) } else { 0 };
        let gain = f32::from_bits(self.gain.load(Ordering::Acquire));
        for sample in &mut out[..read] {
            *sample *= gain;
        }
        out[read..].fill(0.0);
        if self.running.load(Ordering::Acquire) && read < out.len() && !self.drained.load(Ordering::Acquire) {
            self.underruns.fetch_add(1, Ordering::Relaxed);
        }
        let frames = (read / self.channels.max(1) as usize) as u64;
        let before = self.frames_since_flush.fetch_add(frames, Ordering::AcqRel);
        self.timing.publish(AudioTiming { epoch, frames_before_block: before, block_frames: frames, block_heard_at_us: heard_at_us });
    }
}
