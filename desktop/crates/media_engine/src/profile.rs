//! Per-frame timings of the decoding stages, summed per reporting window and
//! logged by the app with the rest of the media telemetry. Recording is a
//! few atomic adds per frame, so it is always on.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Count, total and worst time of one stage since the last [`Stage::take`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageStats {
    pub count: u64,
    pub total_us: u64,
    pub max_us: u64,
}

impl StageStats {
    pub fn mean_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total_us as f64 / self.count as f64 / 1000.0
        }
    }

    pub fn max_ms(&self) -> f64 {
        self.max_us as f64 / 1000.0
    }
}

impl std::fmt::Display for StageStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} × {:.2} ms (max {:.2})", self.count, self.mean_ms(), self.max_ms())
    }
}

pub struct Stage {
    count: AtomicU64,
    total_us: AtomicU64,
    max_us: AtomicU64,
}

impl Stage {
    pub const fn new() -> Self {
        Self { count: AtomicU64::new(0), total_us: AtomicU64::new(0), max_us: AtomicU64::new(0) }
    }

    pub fn record(&self, elapsed: Duration) {
        let us = elapsed.as_micros() as u64;
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_us.fetch_add(us, Ordering::Relaxed);
        self.max_us.fetch_max(us, Ordering::Relaxed);
    }

    /// The stats so far, starting a new window.
    pub fn take(&self) -> StageStats {
        StageStats {
            count: self.count.swap(0, Ordering::Relaxed),
            total_us: self.total_us.swap(0, Ordering::Relaxed),
            max_us: self.max_us.swap(0, Ordering::Relaxed),
        }
    }
}

impl Default for Stage {
    fn default() -> Self {
        Self::new()
    }
}

/// Wall time inside the video decoder per decoded frame (all sessions).
pub static DECODE: Stage = Stage::new();
/// Copying a hardware frame back to system memory.
pub static TRANSFER: Stage = Stage::new();
/// Packing planes for upload, including any pixel format conversion.
pub static CONVERT: Stage = Stage::new();

/// Decode, transfer and convert stats since the last call.
pub fn take() -> [StageStats; 3] {
    [DECODE.take(), TRANSFER.take(), CONVERT.take()]
}
