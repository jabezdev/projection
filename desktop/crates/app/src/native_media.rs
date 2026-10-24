//! Connects the native media engine to the engine (directives in, reports
//! out) and to the renderer (the frame due now for each video).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use core_engine::MediaBackend;
use media_engine::{now_us, Clip, FrameFormat, MediaEngine, Phase, SessionOptions, VideoFrame, YuvMatrix};
use protocol::{MediaDirective, PlaybackPhase, PlaybackReport, PlaybackState};
use render_engine::{FramePixels, PixelFrame, VideoFrames, VideoKey, YuvLayout, YuvPixels};

use crate::engine_host::EngineHandle;

/// Reports are sent on phase changes and at least this often while a
/// session exists; the UI timeline needs no more.
const REPORT_INTERVAL: Duration = Duration::from_millis(200);
const REPORT_POLL: Duration = Duration::from_millis(50);
const TELEMETRY_INTERVAL: Duration = Duration::from_secs(5);

/// The engine's session and seek generation the backend is serving.
#[derive(Clone, Copy)]
struct Tracked {
    session_id: u64,
    generation: u64,
}

pub struct NativeMedia {
    engine: Arc<MediaEngine>,
    tracked: Arc<Mutex<Option<Tracked>>>,
    running: Arc<AtomicBool>,
    reporter: Mutex<Option<JoinHandle<()>>>,
}

impl NativeMedia {
    pub fn start(reports: EngineHandle) -> anyhow::Result<Arc<Self>> {
        let engine = Arc::new(MediaEngine::new());
        let tracked: Arc<Mutex<Option<Tracked>>> = Arc::default();
        let running = Arc::new(AtomicBool::new(true));
        let reporter = {
            let (engine, tracked, running) = (engine.clone(), tracked.clone(), running.clone());
            std::thread::Builder::new()
                .name("media-reporter".into())
                .spawn(move || report_loop(&engine, &tracked, &running, &reports))?
        };
        Ok(Arc::new(Self { engine, tracked, running, reporter: Mutex::new(Some(reporter)) }))
    }

    pub fn frames(self: &Arc<Self>) -> Arc<dyn VideoFrames> {
        Arc::new(MediaFrames(self.engine.clone()))
    }

    /// Keeps exactly these muted looping videos open: `playing` ones play,
    /// `prepared` ones wait on their first frame.
    pub fn set_ambient(&self, playing: &[String], prepared: &[String]) {
        self.engine.set_ambient(playing, prepared);
    }

    /// Opens the next foreground media paused, so its cue starts at once.
    pub fn prepare(&self, path: Option<&str>) {
        self.engine.prepare(path);
    }

    /// Stops reporting and closes the session (and its decoder).
    pub fn shutdown(&self) {
        self.running.store(false, Ordering::Release);
        if let Some(thread) = self.reporter.lock().expect("reporter poisoned").take() {
            let _ = thread.join();
        }
        self.engine.shutdown();
    }
}

fn report_loop(engine: &MediaEngine, tracked: &Mutex<Option<Tracked>>, running: &AtomicBool, reports: &EngineHandle) {
    let mut last: Option<(u64, u64, PlaybackPhase, Instant)> = None;
    let mut telemetry_at = Instant::now();
    let mut profile_at = Instant::now();
    while running.load(Ordering::Acquire) {
        std::thread::sleep(REPORT_POLL);
        if profile_at.elapsed() >= TELEMETRY_INTERVAL {
            profile_at = Instant::now();
            log_profile();
        }
        let Some(target) = *tracked.lock().expect("tracked session poisoned") else { continue };
        let Some(session) = engine.session(target.session_id) else { continue };
        let status = session.status(now_us());
        if telemetry_at.elapsed() >= TELEMETRY_INTERVAL {
            telemetry_at = Instant::now();
            log::info!(
                "media: session {} {:?} at {:.2}s; decoders {}, audio outputs {} (reopened {}), frames presented {} late {}, audio underruns {}",
                target.session_id,
                status.phase,
                status.position_us as f64 / 1e6,
                media_engine::active_decoders(),
                media_engine::opened_outputs(),
                media_engine::audio_recoveries(),
                status.frames.presented,
                status.frames.dropped_late,
                status.audio_underruns,
            );
        }
        let phase = match status.phase {
            Phase::Preparing => PlaybackPhase::Preparing,
            Phase::Ready => PlaybackPhase::Ready,
            Phase::Playing => PlaybackPhase::Playing,
            Phase::Paused => PlaybackPhase::Paused,
            Phase::Ended => PlaybackPhase::Ended,
            Phase::Failed => PlaybackPhase::Failed,
        };
        let changed = last.is_none_or(|(id, generation, previous, _)| {
            id != target.session_id || generation != target.generation || previous != phase
        });
        if !changed && last.is_some_and(|(.., at)| at.elapsed() < REPORT_INTERVAL) {
            continue;
        }
        reports.report(PlaybackReport {
            session_id: target.session_id,
            generation: target.generation,
            phase,
            position_ms: (status.position_us.max(0) / 1000) as u64,
            duration_ms: status.duration_us.map(|d| (d / 1000) as u64),
            error: status.error.or_else(|| {
                engine.audio_unavailable().then(|| "The audio output device is unavailable; reconnecting…".to_string())
            }),
        });
        last = Some((target.session_id, target.generation, phase, Instant::now()));
    }
}

/// Logs the per-frame stage timings of the last window, when anything was
/// decoded or drawn. `tools/e2e/profile.ts` reads these lines.
fn log_profile() {
    let [decode, transfer, convert] = media_engine::profile::take();
    let render = render_engine::profile::take();
    let busy = [decode, transfer, convert].iter().any(|s| s.count > 0) || render.upload.count > 0 || render.draw.iter().any(|s| s.count > 0);
    if busy {
        log::info!(
            "profile: decode {decode}; transfer {transfer}; convert {convert}; upload {}; projection draw {} gpu {}; confidence draw {} gpu {}",
            render.upload, render.draw[0], render.gpu[0], render.draw[1], render.gpu[1],
        );
    }
}

/// The engine's `MediaBackend` while outputs are native. Which outputs show
/// a session's frames is decided by the scene feed, not here.
pub struct NativeMediaBackend {
    pub media: Arc<NativeMedia>,
}

impl NativeMediaBackend {
    fn track(&self, session_id: u64, generation: u64) {
        *self.media.tracked.lock().expect("tracked session poisoned") = Some(Tracked { session_id, generation });
    }
}

fn options(playback: &PlaybackState) -> SessionOptions {
    SessionOptions {
        clip: Clip {
            start_us: playback.start_ms as i64 * 1000,
            end_us: playback.end_ms.map(|end| end as i64 * 1000),
            looping: playback.looping,
        },
        rate: f64::from(playback.rate_percent) / 100.0,
        volume: f32::from(playback.volume) / 100.0,
        muted: playback.muted,
    }
}

impl MediaBackend for NativeMediaBackend {
    fn send(&mut self, directive: MediaDirective) {
        let media = &self.media.engine;
        let now = now_us();
        match directive {
            MediaDirective::Open { playback } => {
                self.track(playback.session_id, playback.generation);
                match media.open(playback.session_id, &playback.path, options(&playback)) {
                    Ok(session) if playback.running() => session.play(now),
                    Ok(_) => {}
                    Err(e) => log::error!("Opening {}: {e:#}", playback.path),
                }
            }
            MediaDirective::Play { session_id } => {
                if let Some(session) = media.session(session_id) {
                    session.play(now);
                }
            }
            MediaDirective::Pause { session_id } => {
                if let Some(session) = media.session(session_id) {
                    session.pause(now);
                }
            }
            MediaDirective::Seek { session_id, generation, position_ms } => {
                if let Some(session) = media.session(session_id) {
                    session.seek(position_ms as i64 * 1000, now);
                    self.track(session_id, generation);
                }
            }
            MediaDirective::Configure { playback } => {
                if let Some(session) = media.session(playback.session_id) {
                    session.configure(options(&playback), now);
                }
            }
            MediaDirective::Close { session_id } => {
                let mut tracked = self.media.tracked.lock().expect("tracked session poisoned");
                if tracked.is_some_and(|t| t.session_id == session_id) {
                    *tracked = None;
                }
                drop(tracked);
                media.close(session_id);
            }
        }
    }
}

struct MediaFrames(Arc<MediaEngine>);

struct Frame(Arc<VideoFrame>);

impl PixelFrame for Frame {
    fn id(&self) -> u64 {
        self.0.id
    }
    fn size(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }
    fn pixels(&self) -> FramePixels<'_> {
        let frame = &self.0;
        let layout = match frame.format {
            FrameFormat::Rgba => {
                let (data, stride) = frame.plane(0);
                return FramePixels::Rgba { data, stride };
            }
            FrameFormat::I420 => YuvLayout::I420,
            FrameFormat::Nv12 => YuvLayout::Nv12,
        };
        let plane = |i: usize| if i < frame.format.planes() { frame.plane(i) } else { (&[][..], 0) };
        FramePixels::Yuv(YuvPixels {
            layout,
            planes: [plane(0), plane(1), plane(2)],
            matrix: match frame.matrix {
                YuvMatrix::Bt601 => render_engine::YuvMatrix::Bt601,
                YuvMatrix::Bt709 => render_engine::YuvMatrix::Bt709,
                YuvMatrix::Bt2020 => render_engine::YuvMatrix::Bt2020,
            },
            full_range: frame.full_range,
        })
    }
}

impl VideoFrames for MediaFrames {
    fn frame(&self, key: &VideoKey) -> Option<Arc<dyn PixelFrame>> {
        let frame = match key {
            VideoKey::Session(id) => self.0.session(*id)?.current_frame(now_us()),
            VideoKey::Ambient(path) => self.0.ambient_frame(path, now_us()),
            // Rendered by the frame pump, never decoded.
            VideoKey::Shader(_) => None,
        };
        frame.map(|frame| Arc::new(Frame(frame)) as Arc<dyn PixelFrame>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::PlaybackKind;

    fn playback(session_id: u64, kind: PlaybackKind) -> PlaybackState {
        PlaybackState {
            session_id,
            generation: 0,
            item_id: 1,
            path: "/nonexistent/media".into(),
            kind,
            paused: true,
            suspended: false,
            phase: PlaybackPhase::Preparing,
            position_ms: 0,
            duration_ms: None,
            position_at_ms: 0,
            volume: 100,
            muted: false,
            rate_percent: 100,
            looping: false,
            start_ms: 0,
            end_ms: None,
            confidence: false,
            error: None,
        }
    }

    #[test]
    fn reports_follow_the_open_session_until_it_closes() {
        let media = Arc::new(NativeMedia {
            engine: Arc::new(MediaEngine::with_audio(None)),
            tracked: Arc::default(),
            running: Arc::new(AtomicBool::new(false)),
            reporter: Mutex::new(None),
        });
        let mut backend = NativeMediaBackend { media: media.clone() };
        backend.send(MediaDirective::Open { playback: playback(2, PlaybackKind::Audio) });
        assert!(media.tracked.lock().unwrap().is_some_and(|t| t.session_id == 2));
        backend.send(MediaDirective::Play { session_id: 2 });
        backend.send(MediaDirective::Close { session_id: 2 });
        assert!(media.tracked.lock().unwrap().is_none());
    }
}
