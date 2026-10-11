//! One playback session: a decoder thread, its frame queue and audio feed,
//! and the clock that decides which frame is due.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;

use crossbeam::channel::{unbounded, Sender};

use crate::audio::SessionAudio;
use crate::clock::{now_us, MonotonicClock};
use crate::decoder::{self, Clip, Control, DecoderParams, DecoderShared};
use crate::frames::{FrameQueue, FrameStats, VideoFrame};

/// Decoded frames held ahead of presentation (~8 MB each at 1080p).
const FRAME_QUEUE: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionOptions {
    pub clip: Clip,
    /// Media seconds per second, 0.5–2.0. Pitch follows the rate.
    pub rate: f64,
    pub volume: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Preparing,
    Ready,
    Playing,
    Paused,
    Ended,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionStatus {
    pub phase: Phase,
    /// Media position (inside the clip, loops folded back).
    pub position_us: i64,
    pub duration_us: Option<i64>,
    pub error: Option<String>,
    pub frames: FrameStats,
    pub audio_underruns: u64,
}

struct State {
    options: SessionOptions,
    running: bool,
    ever_played: bool,
    monotonic: MonotonicClock,
    requested_epoch: u64,
    /// Timeline position of the latest requested restart until it is applied.
    pending_position: i64,
    /// Once the audio has run out, video continues on wall time from here.
    audio_ended: Option<MonotonicClock>,
}

/// What the session position is derived from; the decoder reads it too.
struct SessionClock {
    audio: Option<Arc<SessionAudio>>,
    shared: Arc<DecoderShared>,
    state: Mutex<State>,
}

pub struct MediaSession {
    frames: Arc<FrameQueue>,
    clock: Arc<SessionClock>,
    controls: Sender<Control>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl SessionClock {
    fn uses_audio_clock(&self) -> bool {
        self.audio.is_some() && self.shared.has_audio.load(Ordering::Acquire)
    }

    fn position(&self, now_us: i64) -> i64 {
        let mut state = self.state.lock().expect("session state poisoned");
        self.timeline_locked(&mut state, now_us)
    }

    fn timeline_locked(&self, state: &mut State, now_us: i64) -> i64 {
        if self.shared.applied_epoch.load(Ordering::Acquire) < state.requested_epoch || !self.shared.opened.load(Ordering::Acquire) {
            return state.pending_position;
        }
        let position = match &self.audio {
            Some(audio) if self.uses_audio_clock() => {
                let exhausted = audio.drained.load(Ordering::Acquire) && audio.ring.is_empty();
                match (&mut state.audio_ended, exhausted) {
                    (Some(clock), true) => clock.position(now_us),
                    (ended, true) => {
                        // Video may run past the last sample; hand over to wall time.
                        let mut clock = MonotonicClock::new(audio.position(now_us), state.options.rate);
                        clock.set_running(state.running, now_us);
                        *ended = Some(clock);
                        clock.position(now_us)
                    }
                    (ended, false) => {
                        *ended = None;
                        audio.position(now_us)
                    }
                }
            }
            _ => state.monotonic.position(now_us),
        };
        if self.shared.finished.load(Ordering::Acquire) {
            position.min(self.shared.end_timeline_us.load(Ordering::Acquire))
        } else {
            position
        }
    }
}

impl MediaSession {
    /// Starts decoding immediately (preloading frames and audio) but stays
    /// silent until `play`. `audio` must already be attached to the output.
    pub fn open(path: &str, options: SessionOptions, audio: Option<Arc<SessionAudio>>) -> anyhow::Result<Self> {
        let frames = Arc::new(FrameQueue::new(FRAME_QUEUE));
        let shared = Arc::new(DecoderShared::new());
        let (controls, receiver) = unbounded();
        if let Some(audio) = &audio {
            audio.set_gain(gain(&options));
        }
        let start = options.clip.start_us;
        let clock = Arc::new(SessionClock {
            audio: audio.clone(),
            shared: shared.clone(),
            state: Mutex::new(State {
                options,
                running: false,
                ever_played: false,
                monotonic: MonotonicClock::new(start, options.rate),
                requested_epoch: 0,
                pending_position: start,
                audio_ended: None,
            }),
        });
        let reader = Arc::downgrade(&clock);
        let thread = decoder::spawn(DecoderParams {
            path: path.to_string(),
            clip: options.clip,
            rate: options.rate,
            frames: frames.clone(),
            audio,
            shared,
            controls: receiver,
            clock: Arc::new(move || Weak::upgrade(&reader).map_or(i64::MIN, |c| c.position(now_us()))),
        })?;
        Ok(Self { frames, clock, controls, thread: Mutex::new(Some(thread)) })
    }

    /// Timeline position at `now_us` (keeps increasing across loops).
    pub fn timeline_position(&self, now_us: i64) -> i64 {
        self.clock.position(now_us)
    }

    /// The frame to show at `now_us`. Only the renderer should call this: it
    /// consumes due frames from the queue.
    pub fn current_frame(&self, now_us: i64) -> Option<Arc<VideoFrame>> {
        self.frames.select(self.timeline_position(now_us))
    }

    pub fn play(&self, now_us: i64) {
        self.set_running(true, now_us);
    }

    pub fn pause(&self, now_us: i64) {
        self.set_running(false, now_us);
    }

    fn set_running(&self, running: bool, now_us: i64) {
        let mut state = self.clock.state.lock().expect("session state poisoned");
        state.running = running;
        state.ever_played |= running;
        state.monotonic.set_running(running, now_us);
        if let Some(clock) = &mut state.audio_ended {
            clock.set_running(running, now_us);
        }
        if let Some(audio) = &self.clock.audio {
            audio.set_running(running);
        }
    }

    /// Discards everything decoded and continues from media `position_us`.
    pub fn seek(&self, position_us: i64, now_us: i64) {
        let mut state = self.clock.state.lock().expect("session state poisoned");
        let rate = state.options.rate;
        self.restart_locked(&mut state, position_us, rate, now_us);
    }

    fn restart_locked(&self, state: &mut State, position_us: i64, rate: f64, now_us: i64) {
        state.requested_epoch += 1;
        state.pending_position = position_us;
        state.audio_ended = None;
        state.monotonic.reset(position_us, rate, now_us);
        self.clock.shared.interrupts.fetch_add(1, Ordering::AcqRel);
        let _ = self.controls.send(Control::Restart { epoch: state.requested_epoch, position_us, rate });
        self.frames.wake();
    }

    /// Applies new options: volume/mute immediately, rate by restarting at the
    /// current position, clip bounds without interrupting playback.
    pub fn configure(&self, options: SessionOptions, now_us: i64) {
        let mut state = self.clock.state.lock().expect("session state poisoned");
        let previous = state.options;
        state.options = options;
        if let Some(audio) = &self.clock.audio {
            audio.set_gain(gain(&options));
        }
        let timeline = self.clock.timeline_locked(&mut state, now_us);
        let position = self.media_position(timeline, &state.options);
        let outside = position < options.clip.start_us || options.clip.end_us.is_some_and(|end| position >= end);
        if options.rate != previous.rate || outside {
            let target = if outside { options.clip.start_us } else { position };
            self.restart_locked(&mut state, target, options.rate, now_us);
        }
        if options.clip != previous.clip {
            let _ = self.controls.send(Control::SetClip(options.clip));
        }
    }

    /// Folds a timeline position back into the clip after loop wraps.
    fn media_position(&self, timeline_us: i64, options: &SessionOptions) -> i64 {
        let loop_len = self.clock.shared.loop_len_us.load(Ordering::Acquire);
        let start = options.clip.start_us;
        if loop_len > 0 && timeline_us >= start {
            start + (timeline_us - start).rem_euclid(loop_len)
        } else {
            timeline_us
        }
    }

    pub fn status(&self, now_us: i64) -> SessionStatus {
        let mut state = self.clock.state.lock().expect("session state poisoned");
        let timeline = self.clock.timeline_locked(&mut state, now_us);
        let error = self.clock.shared.error.lock().expect("decoder error poisoned").clone();
        let finished = self.clock.shared.finished.load(Ordering::Acquire);
        let ended = finished && timeline >= self.clock.shared.end_timeline_us.load(Ordering::Acquire);
        let prepared = self.clock.shared.opened.load(Ordering::Acquire)
            && (!self.clock.shared.has_video.load(Ordering::Acquire) || self.frames.last_pts().is_some());
        let phase = if error.is_some() {
            Phase::Failed
        } else if !prepared {
            Phase::Preparing
        } else if ended {
            Phase::Ended
        } else if state.running {
            Phase::Playing
        } else if state.ever_played {
            Phase::Paused
        } else {
            Phase::Ready
        };
        let duration = self.clock.shared.duration_us.load(Ordering::Acquire);
        SessionStatus {
            phase,
            position_us: self.media_position(timeline, &state.options),
            duration_us: (duration >= 0).then_some(duration),
            error,
            frames: self.frames.stats(),
            audio_underruns: self.clock.audio.as_ref().map_or(0, |a| a.underruns.load(Ordering::Relaxed)),
        }
    }

    pub fn audio(&self) -> Option<&Arc<SessionAudio>> {
        self.clock.audio.as_ref()
    }

    /// Stops the decoder and waits for it; releases every frame.
    pub fn close(&self) {
        if let Some(audio) = &self.clock.audio {
            audio.set_running(false);
        }
        self.clock.shared.interrupts.fetch_add(1, Ordering::AcqRel);
        let _ = self.controls.send(Control::Close);
        self.frames.wake();
        if let Some(thread) = self.thread.lock().expect("session thread poisoned").take() {
            let _ = thread.join();
        }
        self.frames.clear();
    }
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        self.close();
    }
}

fn gain(options: &SessionOptions) -> f32 {
    if options.muted {
        0.0
    } else {
        options.volume.clamp(0.0, 1.0)
    }
}
