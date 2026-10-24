//! Per-frame timings of uploading and drawing, summed per reporting window
//! and logged by the app with the decoding stages. Recording is a few atomic
//! adds per frame, so it is always on.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::OutputRole;

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

/// Writing a video frame to its texture (and converting YUV), on the pump.
pub static UPLOAD: Stage = Stage::new();
/// Per output: laying out and encoding a frame on its thread.
static DRAW: [Stage; 2] = [Stage::new(), Stage::new()];
/// Per output: from submitting a frame until the GPU finished it, which
/// includes waiting behind other work queued on the GPU.
static GPU: [Stage; 2] = [Stage::new(), Stage::new()];

fn index(role: OutputRole) -> usize {
    match role {
        OutputRole::Projection => 0,
        OutputRole::Confidence => 1,
    }
}

pub(crate) fn draw(role: OutputRole) -> &'static Stage {
    &DRAW[index(role)]
}

pub(crate) fn gpu(role: OutputRole) -> &'static Stage {
    &GPU[index(role)]
}

/// One window of render timings.
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderTimings {
    pub upload: StageStats,
    /// Projection, then confidence.
    pub draw: [StageStats; 2],
    pub gpu: [StageStats; 2],
}

/// The timings since the last call.
pub fn take() -> RenderTimings {
    RenderTimings {
        upload: UPLOAD.take(),
        draw: [DRAW[0].take(), DRAW[1].take()],
        gpu: [GPU[0].take(), GPU[1].take()],
    }
}
