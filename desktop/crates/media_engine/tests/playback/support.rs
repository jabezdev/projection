use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use media_engine::{now_us, Clip, MediaSession, Phase, SessionAudio, SessionOptions};

pub(super) const DEVICE_RATE: u32 = 48_000;
pub(super) const CHANNELS: u16 = 2;

pub(super) fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) fn fixtures() -> Option<&'static (PathBuf, PathBuf)> {
    static FIXTURES: OnceLock<Option<(PathBuf, PathBuf)>> = OnceLock::new();
    FIXTURES
        .get_or_init(|| {
            let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("media-fixtures");
            std::fs::create_dir_all(&dir).ok()?;
            let av = dir.join("av.mp4");
            let video = dir.join("video.mp4");
            let make = |args: &[&str], out: &PathBuf| {
                out.exists()
                    || std::process::Command::new("ffmpeg")
                        .args(["-nostdin", "-v", "error", "-y"])
                        .args(args)
                        .arg(out)
                        .status()
                        .is_ok_and(|s| s.success())
            };
            let ok = make(
                &[
                    "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=30:duration=4",
                    "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=44100:duration=4",
                    "-c:v", "libx264", "-g", "30", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest",
                ],
                &av,
            ) && make(
                &["-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=2", "-c:v", "libx264", "-g", "25", "-pix_fmt", "yuv420p"],
                &video,
            );
            if !ok {
                eprintln!("FFmpeg CLI unavailable; skipping decode tests");
            }
            ok.then_some((av, video))
        })
        .as_ref()
}

pub(super) struct FakeDevice {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl FakeDevice {
    pub(super) fn start(audio: Arc<SessionAudio>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut block = vec![0f32; (DEVICE_RATE / 100) as usize * CHANNELS as usize];
                let start = Instant::now();
                let mut blocks = 0u32;
                while !stop.load(Ordering::Relaxed) {
                    audio.render(&mut block, now_us() + 20_000);
                    blocks += 1;
                    let due = start + Duration::from_millis(10) * blocks;
                    std::thread::sleep(due.saturating_duration_since(Instant::now()));
                }
            })
        };
        Self { stop, thread: Some(thread) }
    }
}

impl Drop for FakeDevice {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) fn options(clip: Clip) -> SessionOptions {
    SessionOptions { clip, rate: 1.0, volume: 1.0, muted: false }
}

pub(super) fn whole() -> Clip {
    Clip { start_us: 0, end_us: None, looping: false }
}

pub(super) fn open(path: &PathBuf, options: SessionOptions) -> (MediaSession, Arc<SessionAudio>, FakeDevice) {
    let audio = Arc::new(SessionAudio::new(DEVICE_RATE, CHANNELS));
    let device = FakeDevice::start(audio.clone());
    let session = MediaSession::open(path.to_str().unwrap(), options, Some(audio.clone())).expect("open");
    (session, audio, device)
}

pub(super) fn wait_for(session: &MediaSession, phase: Phase, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while session.status(now_us()).phase != phase {
        assert!(Instant::now() < deadline, "timed out waiting for {phase:?}: {:?}", session.status(now_us()));
        session.current_frame(now_us());
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn run_for(session: &MediaSession, duration: Duration) -> Vec<i64> {
    let deadline = Instant::now() + duration;
    let mut shown = Vec::new();
    while Instant::now() < deadline {
        if let Some(frame) = session.current_frame(now_us()) {
            if shown.last() != Some(&frame.pts_us) {
                shown.push(frame.pts_us);
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    shown
}

pub(super) fn wait_for_decoders(count: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while media_engine::active_decoders() != count && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn audio_fixture() -> Option<&'static PathBuf> {
    static FIXTURE: OnceLock<Option<PathBuf>> = OnceLock::new();
    FIXTURE
        .get_or_init(|| {
            let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("media-fixtures");
            std::fs::create_dir_all(&dir).ok()?;
            let out = dir.join("audio.mp3");
            let ok = out.exists()
                || std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=330:sample_rate=44100:duration=3", "-c:a", "libmp3lame"])
                    .arg(&out)
                    .status()
                    .is_ok_and(|s| s.success());
            ok.then_some(out)
        })
        .as_ref()
}
