use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::clock::{audio_position, Anchor, AudioTiming, AudioTimingCell};
use crate::ring::SampleRing;

pub(super) const RING_SECONDS: f64 = 1.0;
const REFILL_SECONDS: f64 = 0.05;

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
    refilling: AtomicBool,
    refill_samples: usize,
    pub underruns: AtomicU64,
    pub(super) attached: Mutex<bool>,
}

impl SessionAudio {
    pub fn new(device_rate: u32, channels: u16) -> Self {
        let capacity = (device_rate as f64 * channels as f64 * RING_SECONDS) as usize;
        let ring = SampleRing::new(capacity);
        let refill_samples = ((device_rate as f64 * channels as f64 * REFILL_SECONDS) as usize).min(ring.capacity() / 2);
        Self {
            ring,
            channels,
            device_rate,
            running: AtomicBool::new(false),
            gain: AtomicU32::new(1f32.to_bits()),
            drained: AtomicBool::new(false),
            anchor: Mutex::new(Anchor { epoch: 0, position_us: 0, rate: 1.0 }),
            timing: AudioTimingCell::default(),
            frames_since_flush: AtomicU64::new(0),
            epoch_seen: AtomicU64::new(0),
            refilling: AtomicBool::new(false),
            refill_samples,
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
            if *attached { self.ring.request_flush() } else { self.ring.flush_unconsumed() }
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
            self.refilling.store(true, Ordering::Release);
        }
        let drained = self.drained.load(Ordering::Acquire);
        if self.refilling.load(Ordering::Acquire) && (drained || self.ring.len() >= self.refill_samples) {
            self.refilling.store(false, Ordering::Release);
        }
        let refilling = self.refilling.load(Ordering::Acquire);
        let read = if self.running.load(Ordering::Acquire) && !refilling { self.ring.pop(out) } else { 0 };
        let gain = f32::from_bits(self.gain.load(Ordering::Acquire));
        for sample in &mut out[..read] {
            *sample *= gain;
        }
        out[read..].fill(0.0);
        if self.running.load(Ordering::Acquire) && !refilling && read < out.len() && !drained {
            self.underruns.fetch_add(1, Ordering::Relaxed);
        }
        let frames = (read / self.channels.max(1) as usize) as u64;
        let before = self.frames_since_flush.fetch_add(frames, Ordering::AcqRel);
        self.timing.publish(AudioTiming { epoch, frames_before_block: before, block_frames: frames, block_heard_at_us: heard_at_us });
    }
}
