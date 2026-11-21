mod audio;
pub mod bench;
mod clock;
mod decoder;
mod frames;
pub mod hwaccel;
mod poster;
pub mod profile;
mod ring;
mod session;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub use audio::{audio_recoveries, describe_devices, opened_outputs, AudioOutput, DeviceFormat, SessionAudio};
pub use clock::now_us;
pub use decoder::{active_decoders, fit_within, Clip, MAX_FRAME, MAX_YUV_FRAME};
pub use frames::{FrameFormat, FrameStats, Plane, VideoFrame, YuvMatrix};
pub use poster::{poster, Poster};
pub use session::{MediaSession, Phase, SessionOptions, SessionStatus};

pub struct MediaEngine {
    audio: Option<AudioOutput>,
    foreground: Mutex<Option<(u64, Arc<MediaSession>)>>,
    ambient: Mutex<HashMap<String, (Arc<MediaSession>, bool)>>,
    prepared: Mutex<Option<(String, Arc<MediaSession>)>>,
}

impl MediaEngine {
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
        Self { audio, foreground: Mutex::new(None), ambient: Mutex::default(), prepared: Mutex::default() }
    }

    pub fn audio_unavailable(&self) -> bool {
        self.audio.as_ref().is_some_and(AudioOutput::failed)
    }

    pub fn audio_format(&self) -> Option<DeviceFormat> {
        self.audio.as_ref().map(|a| a.format)
    }

    pub fn prepare(&self, path: Option<&str>) {
        let mut prepared = self.prepared.lock().expect("prepared poisoned");
        if prepared.as_ref().map(|(p, _)| p.as_str()) == path {
            return;
        }
        if let Some((_, old)) = prepared.take() {
            old.close();
        }
        let Some(path) = path else { return };
        let feed = self.audio.as_ref().map(|output| Arc::new(SessionAudio::new(output.format.rate, output.format.channels)));
        let options = SessionOptions { clip: Clip { start_us: 0, end_us: None, looping: false }, rate: 1.0, volume: 1.0, muted: false };
        match MediaSession::open(path, options, feed) {
            Ok(session) => *prepared = Some((path.to_string(), Arc::new(session))),
            Err(e) => log::warn!("Preparing {path}: {e:#}"),
        }
    }

    pub fn open(&self, id: u64, path: &str, options: SessionOptions) -> anyhow::Result<Arc<MediaSession>> {
        self.close_foreground();
        let ready = {
            let mut prepared = self.prepared.lock().expect("prepared poisoned");
            match prepared.take() {
                Some((prepared_path, session)) if prepared_path == path => Some(session),
                other => {
                    *prepared = other;
                    None
                }
            }
        };
        if let Some(session) = ready {
            session.configure(options, now_us());
            if let (Some(output), Some(feed)) = (&self.audio, session.audio()) {
                output.attach(feed.clone());
            }
            *self.foreground.lock().expect("foreground poisoned") = Some((id, session.clone()));
            return Ok(session);
        }
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

    pub fn session(&self, id: u64) -> Option<Arc<MediaSession>> {
        match &*self.foreground.lock().expect("foreground poisoned") {
            Some((current, session)) if *current == id => Some(session.clone()),
            _ => None,
        }
    }

    pub fn foreground(&self) -> Option<(u64, Arc<MediaSession>)> {
        self.foreground.lock().expect("foreground poisoned").clone()
    }

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

    pub fn current_frame(&self, now_us: i64) -> Option<Arc<VideoFrame>> {
        let session = self.foreground().map(|(_, s)| s)?;
        session.current_frame(now_us)
    }

    pub fn set_ambient(&self, playing: &[String], prepared: &[String]) {
        let mut ambient = self.ambient.lock().expect("ambient poisoned");
        let closing: Vec<String> = ambient.keys().filter(|p| !playing.contains(p) && !prepared.contains(p)).cloned().collect();
        for path in closing {
            if let Some((session, _)) = ambient.remove(&path) {
                session.close();
            }
        }
        let now = now_us();
        for path in playing.iter().chain(prepared) {
            let play = playing.contains(path);
            if let Some((session, running)) = ambient.get_mut(path) {
                if *running != play {
                    if play {
                        session.play(now);
                    } else {
                        session.pause(now);
                    }
                    *running = play;
                }
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
                    if play {
                        session.play(now);
                    }
                    ambient.insert(path.clone(), (Arc::new(session), play));
                }
                Err(e) => log::error!("Opening ambient video {path}: {e:#}"),
            }
        }
    }

    pub fn ambient_frame(&self, path: &str, now_us: i64) -> Option<Arc<VideoFrame>> {
        let session = self.ambient.lock().expect("ambient poisoned").get(path).map(|(s, _)| s.clone())?;
        session.current_frame(now_us)
    }

    pub fn shutdown(&self) {
        self.close_foreground();
        self.prepare(None);
        self.set_ambient(&[], &[]);
    }
}

impl Default for MediaEngine {
    fn default() -> Self {
        Self::new()
    }
}
