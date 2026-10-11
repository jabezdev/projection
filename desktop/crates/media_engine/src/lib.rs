//! Native media playback: one decoder per session, one audio output for the
//! application, and clocks that decide which decoded frame every output shows.
//!
//! The renderer asks [`MediaEngine::current_frame`] for the frame due now and
//! uploads it once for all outputs; no output owns a decoder or audio stream.

mod audio;
mod clock;
mod decoder;
mod frames;
mod poster;
mod ring;
mod session;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub use audio::{audio_recoveries, opened_outputs, AudioOutput, DeviceFormat, SessionAudio};
pub use clock::now_us;
pub use decoder::{active_decoders, fit_within, Clip, MAX_FRAME};
pub use frames::{FrameStats, VideoFrame};
pub use poster::{poster, Poster};
pub use session::{MediaSession, Phase, SessionOptions, SessionStatus};

/// Owns the audio output, the foreground session and muted ambient loops. A
/// new foreground session closes the previous one first, so at most one is
/// ever audible; ambient sessions never decode audio.
pub struct MediaEngine {
    audio: Option<AudioOutput>,
    foreground: Mutex<Option<(u64, Arc<MediaSession>)>>,
    /// Song backgrounds and logo videos, by file.
    ambient: Mutex<HashMap<String, Arc<MediaSession>>>,
}

impl MediaEngine {
    /// Opens the default audio device; without one, video still plays on a
    /// wall-clock timeline.
    pub fn new() -> Self {
        let audio = match AudioOutput::open_default() {
            Ok(output) => Some(output),
            Err(e) => {
                log::error!("No audio output; media will play silently: {e:#}");
                None
            }
        };
        Self::with_audio(audio)
    }

    pub fn with_audio(audio: Option<AudioOutput>) -> Self {
        Self { audio, foreground: Mutex::new(None), ambient: Mutex::default() }
    }

    /// The audio device failed and has not been reopened yet.
    pub fn audio_unavailable(&self) -> bool {
        self.audio.as_ref().is_some_and(AudioOutput::failed)
    }

    pub fn audio_format(&self) -> Option<DeviceFormat> {
        self.audio.as_ref().map(|a| a.format)
    }

    /// Replaces the foreground session with `id`, preloading it silently.
    pub fn open(&self, id: u64, path: &str, options: SessionOptions) -> anyhow::Result<Arc<MediaSession>> {
        self.close_foreground();
        let feed = self.audio.as_ref().map(|output| {
            let feed = Arc::new(SessionAudio::new(output.format.rate, output.format.channels));
            output.attach(feed.clone());
            feed
        });
        let session = match MediaSession::open(path, options, feed.clone()) {
            Ok(session) => Arc::new(session),
            Err(e) => {
                if let (Some(output), Some(feed)) = (&self.audio, &feed) {
                    output.detach(feed);
                }
                return Err(e);
            }
        };
        *self.foreground.lock().expect("foreground poisoned") = Some((id, session.clone()));
        Ok(session)
    }

    /// The foreground session if it is `id`.
    pub fn session(&self, id: u64) -> Option<Arc<MediaSession>> {
        match &*self.foreground.lock().expect("foreground poisoned") {
            Some((current, session)) if *current == id => Some(session.clone()),
            _ => None,
        }
    }

    pub fn foreground(&self) -> Option<(u64, Arc<MediaSession>)> {
        self.foreground.lock().expect("foreground poisoned").clone()
    }

    /// Closes `id` if it is the foreground session.
    pub fn close(&self, id: u64) {
        let session = {
            let mut foreground = self.foreground.lock().expect("foreground poisoned");
            match &*foreground {
                Some((current, _)) if *current == id => foreground.take().map(|(_, s)| s),
                _ => None,
            }
        };
        if let Some(session) = session {
            self.release(&session);
        }
    }

    fn close_foreground(&self) {
        let session = self.foreground.lock().expect("foreground poisoned").take();
        if let Some((_, session)) = session {
            self.release(&session);
        }
    }

    fn release(&self, session: &Arc<MediaSession>) {
        if let (Some(output), Some(feed)) = (&self.audio, session.audio()) {
            output.detach(feed);
        }
        session.close();
    }

    /// The foreground frame due now, if any.
    pub fn current_frame(&self, now_us: i64) -> Option<Arc<VideoFrame>> {
        let session = self.foreground().map(|(_, s)| s)?;
        session.current_frame(now_us)
    }

    /// Keeps exactly `paths` open as silent, looping, playing sessions.
    /// Sessions already open continue uninterrupted.
    pub fn set_ambient(&self, paths: &[String]) {
        let mut ambient = self.ambient.lock().expect("ambient poisoned");
        let closing: Vec<String> = ambient.keys().filter(|p| !paths.contains(p)).cloned().collect();
        for path in closing {
            if let Some(session) = ambient.remove(&path) {
                session.close();
            }
        }
        for path in paths {
            if ambient.contains_key(path) {
                continue;
            }
            let options = SessionOptions {
                clip: Clip { start_us: 0, end_us: None, looping: true },
                rate: 1.0,
                volume: 0.0,
                muted: true,
            };
            match MediaSession::open(path, options, None) {
                Ok(session) => {
                    session.play(now_us());
                    ambient.insert(path.clone(), Arc::new(session));
                }
                Err(e) => log::error!("Opening ambient video {path}: {e:#}"),
            }
        }
    }

    /// The ambient frame due now for `path`. Only the renderer's pump should
    /// call this: it consumes due frames.
    pub fn ambient_frame(&self, path: &str, now_us: i64) -> Option<Arc<VideoFrame>> {
        let session = self.ambient.lock().expect("ambient poisoned").get(path).cloned()?;
        session.current_frame(now_us)
    }

    pub fn shutdown(&self) {
        self.close_foreground();
        self.set_ambient(&[]);
    }
}

impl Default for MediaEngine {
    fn default() -> Self {
        Self::new()
    }
}
