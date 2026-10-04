use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use rodio::{OutputStream, OutputStreamHandle};

/// High-precision atomic time in microseconds
pub struct AudioTime {
    time_us: AtomicU64,
}

impl AudioTime {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            time_us: AtomicU64::new(0),
        })
    }

    pub fn set(&self, time_sec: f64) {
        self.time_us.store((time_sec * 1_000_000.0) as u64, Ordering::Release);
    }

    pub fn get(&self) -> f64 {
        self.time_us.load(Ordering::Acquire) as f64 / 1_000_000.0
    }
}

pub struct AudioEngine {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    time: Arc<AudioTime>,
    // TODO: Maintain active sinks
}

impl AudioEngine {
    pub fn new() -> anyhow::Result<Self> {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| anyhow::anyhow!("Failed to initialize audio stream: {:?}", e))?;
        
        Ok(Self {
            _stream,
            _stream_handle: stream_handle,
            time: AudioTime::new(),
        })
    }

    pub fn time_ref(&self) -> Arc<AudioTime> {
        self.time.clone()
    }

    /// MVP: A simple way to play audio and track time. 
    /// In production, we wrap the source to update `time` as samples are generated.
    pub fn play_dummy(&self) {
        // ...
        todo!("Implement audio playback with sample counting for master clock");
    }
}
