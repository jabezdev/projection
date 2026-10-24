//! Decodes real media generated with the FFmpeg CLI. A test thread plays the
//! role of the audio device, consuming samples at real-time pace.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use media_engine::{active_decoders, hwaccel, now_us, Clip, FrameFormat, MediaSession, Phase, SessionAudio, SessionOptions, YuvMatrix};

const DEVICE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

/// Tests share process-wide decoder counters, so they run one at a time.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// (audio+video 30 fps 4 s, video-only 25 fps 2 s), or None without FFmpeg.
fn fixtures() -> Option<&'static (PathBuf, PathBuf)> {
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

/// Consumes a session's audio like a device with 10 ms blocks and 20 ms latency.
struct FakeDevice {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl FakeDevice {
    fn start(audio: Arc<SessionAudio>) -> Self {
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

fn options(clip: Clip) -> SessionOptions {
    SessionOptions { clip, rate: 1.0, volume: 1.0, muted: false }
}

fn whole() -> Clip {
    Clip { start_us: 0, end_us: None, looping: false }
}

/// Opens with a fake device attached, as the engine does.
fn open(path: &PathBuf, options: SessionOptions) -> (MediaSession, Arc<SessionAudio>, FakeDevice) {
    let audio = Arc::new(SessionAudio::new(DEVICE_RATE, CHANNELS));
    let device = FakeDevice::start(audio.clone());
    let session = MediaSession::open(path.to_str().unwrap(), options, Some(audio.clone())).expect("open");
    (session, audio, device)
}

fn wait_for(session: &MediaSession, phase: Phase, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while session.status(now_us()).phase != phase {
        assert!(Instant::now() < deadline, "timed out waiting for {phase:?}: {:?}", session.status(now_us()));
        // The renderer keeps selecting frames, which drains the queue.
        session.current_frame(now_us());
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Plays for `duration` while selecting frames the way the renderer does.
fn run_for(session: &MediaSession, duration: Duration) -> Vec<i64> {
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

#[test]
fn opening_preloads_the_first_frame_silently() {
    let _serial = serial();
    let Some((av, _)) = fixtures() else { return };
    let (session, audio, _device) = open(av, options(whole()));
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(session.current_frame(now_us()).map(|f| f.pts_us), Some(0));
    assert_eq!(session.status(now_us()).position_us, 0, "the clock does not run before play");
    assert!(!audio.ring.is_empty(), "audio is decoded ahead but not played");
}

#[test]
fn audio_drives_the_clock_and_frames_follow_it() {
    let _serial = serial();
    let Some((av, _)) = fixtures() else { return };
    let (session, _audio, _device) = open(av, options(whole()));
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    let started = now_us();
    session.play(started);
    let shown = run_for(&session, Duration::from_millis(1500));
    let elapsed = now_us() - started;
    let status = session.status(now_us());
    assert_eq!(status.phase, Phase::Playing);
    assert!((status.position_us - elapsed).abs() < 150_000, "position {} vs elapsed {elapsed}", status.position_us);
    let last = *shown.last().unwrap();
    assert!((status.position_us - last).abs() < 100_000, "frame {last} lags position {}", status.position_us);
    assert!(shown.windows(2).all(|w| w[1] > w[0]), "frames advance monotonically");
    assert!(shown.len() >= 35, "about 30 fps presented, got {}", shown.len());
}

#[test]
fn seeking_discards_old_frames_and_reanchors_the_clock() {
    let _serial = serial();
    let Some((av, _)) = fixtures() else { return };
    let (session, _audio, _device) = open(av, options(whole()));
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    session.play(now_us());
    run_for(&session, Duration::from_millis(300));
    session.seek(2_500_000, now_us());
    assert_eq!(session.status(now_us()).position_us, 2_500_000, "reports the target immediately");
    let shown = run_for(&session, Duration::from_millis(600));
    let after_target: Vec<_> = shown.iter().skip_while(|&&pts| pts < 2_500_000).collect();
    assert!(!after_target.is_empty(), "frames from the target arrived: {shown:?}");
    assert!(after_target.iter().all(|&&pts| pts >= 2_500_000), "no pre-seek frame after the target: {shown:?}");
    let position = session.status(now_us()).position_us;
    assert!((2_500_000..3_400_000).contains(&position), "position {position}");
}

#[test]
fn a_clip_without_looping_ends_at_its_end_point() {
    let _serial = serial();
    let Some((av, _)) = fixtures() else { return };
    let clip = Clip { start_us: 1_000_000, end_us: Some(1_600_000), looping: false };
    let (session, _audio, _device) = open(av, options(clip));
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    assert!(session.current_frame(now_us()).unwrap().pts_us >= 1_000_000, "starts at the clip start");
    session.play(now_us());
    wait_for(&session, Phase::Ended, Duration::from_secs(3));
    let status = session.status(now_us());
    assert!((1_500_000..=1_600_000).contains(&status.position_us), "ends at the clip end: {}", status.position_us);
    let frame = session.current_frame(now_us()).unwrap().pts_us;
    assert!(frame < 1_600_000, "never shows a frame past the clip end: {frame}");
}

#[test]
fn looping_wraps_to_the_clip_start_and_keeps_presenting() {
    let _serial = serial();
    let Some((_, video)) = fixtures() else { return };
    let clip = Clip { start_us: 0, end_us: Some(800_000), looping: true };
    let session = MediaSession::open(video.to_str().unwrap(), options(clip), None).expect("open");
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    session.play(now_us());
    let shown = run_for(&session, Duration::from_millis(2_000));
    let status = session.status(now_us());
    assert_eq!(status.phase, Phase::Playing);
    assert!((0..800_000).contains(&status.position_us), "folded back into the clip: {}", status.position_us);
    assert!(*shown.last().unwrap() > 1_600_000, "the timeline continues across two wraps: {shown:?}");
    assert!(shown.windows(2).all(|w| w[1] > w[0]), "no frame repeats or jumps back at the wrap");
}

#[test]
fn rate_changes_the_clock_speed() {
    let _serial = serial();
    let Some((av, _)) = fixtures() else { return };
    let (session, _audio, _device) = open(av, options(whole()));
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    session.configure(SessionOptions { rate: 2.0, ..options(whole()) }, now_us());
    let started = now_us();
    session.play(started);
    run_for(&session, Duration::from_millis(1000));
    let position = session.status(now_us()).position_us;
    let expected = (now_us() - started) * 2;
    assert!((position - expected).abs() < 250_000, "2× playback: position {position} vs {expected}");
}

#[test]
fn muting_silences_the_output_without_stopping_the_clock() {
    let _serial = serial();
    let Some((av, _)) = fixtures() else { return };
    let audio = Arc::new(SessionAudio::new(DEVICE_RATE, CHANNELS));
    let session = MediaSession::open(av.to_str().unwrap(), SessionOptions { muted: true, ..options(whole()) }, Some(audio.clone())).unwrap();
    // Drive the consumer manually: wait for buffered audio, then render.
    let deadline = Instant::now() + Duration::from_secs(5);
    while audio.ring.len() < 9_600 {
        audio.render(&mut [0.0; 2], now_us());
        assert!(Instant::now() < deadline, "audio decoded");
        std::thread::sleep(Duration::from_millis(5));
    }
    session.play(now_us());
    let mut block = vec![1.0f32; 960];
    audio.render(&mut block, now_us());
    assert!(block.iter().all(|&s| s == 0.0), "muted output is silent");
    assert!(session.status(now_us() + 10_000).position_us > 0, "but the clock advances");
}

#[test]
fn closing_stops_the_decoder_promptly_and_frees_its_frames() {
    let _serial = serial();
    let Some((av, video)) = fixtures() else { return };
    let before = active_decoders();
    let (first, _audio, _device) = open(av, options(whole()));
    let second = MediaSession::open(video.to_str().unwrap(), options(whole()), None).unwrap();
    wait_for(&first, Phase::Ready, Duration::from_secs(5));
    wait_for(&second, Phase::Ready, Duration::from_secs(5));
    assert_eq!(active_decoders(), before + 2, "one decoder per session");
    // Closing while the decoder is blocked on a full queue must not hang.
    let started = Instant::now();
    first.close();
    second.close();
    assert!(started.elapsed() < Duration::from_millis(500), "closed in {:?}", started.elapsed());
    assert_eq!(active_decoders(), before);
    assert!(first.current_frame(now_us()).is_none());
}

#[test]
fn a_missing_file_fails_with_a_reason() {
    let _serial = serial();
    let session = MediaSession::open("/nonexistent/clip.mp4", options(whole()), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while session.status(now_us()).phase != Phase::Failed {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(session.status(now_us()).error.unwrap().contains("/nonexistent/clip.mp4"));
}


#[test]
fn playback_reaches_the_end_when_no_output_is_drawing() {
    // Hidden outputs may stop polling for frames; audio and the clock must not wait for them.
    let _serial = serial();
    let Some((av, _)) = fixtures() else { return };
    let (session, audio, _device) = open(av, options(whole()));
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    let underruns = session.status(now_us()).audio_underruns;
    session.play(now_us());
    let started = Instant::now();
    while session.status(now_us()).phase != Phase::Ended {
        assert!(started.elapsed() < Duration::from_secs(6), "stalled: {:?}", session.status(now_us()));
        std::thread::sleep(Duration::from_millis(50));
    }
    let status = session.status(now_us());
    assert!(started.elapsed() > Duration::from_millis(3800), "ended early at {:?}", started.elapsed());
    assert!(status.audio_underruns <= underruns + 1, "audio starved: {status:?}");
    assert!(status.frames.dropped_late > 90, "superseded frames are dropped: {status:?}");
    assert!(audio.ring.is_empty());
}

/// Reproduces against a real file: `MAGI_MEDIA=<path> cargo test -- --ignored paused_seek`.
#[test]
#[ignore]
fn paused_seek_then_play_keeps_presenting() {
    let _serial = serial();
    let path = PathBuf::from(std::env::var("MAGI_MEDIA").expect("MAGI_MEDIA"));
    let (session, audio, _device) = open(&path, options(whole()));
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    session.play(now_us());
    run_for(&session, Duration::from_millis(1500));
    session.pause(now_us());
    run_for(&session, Duration::from_millis(500));
    session.seek(60_000_000, now_us());
    run_for(&session, Duration::from_millis(800));
    session.play(now_us());
    for _ in 0..10 {
        let shown = run_for(&session, Duration::from_millis(300));
        let s = session.status(now_us());
        eprintln!("pos {} frames {:?} ring {} underruns {} new frames {}", s.position_us, s.frames, audio.ring.len(), s.audio_underruns, shown.len());
    }
}

#[test]
fn ambient_sessions_loop_silently_and_close_on_request() {
    let _serial = serial();
    let Some((_, video)) = fixtures() else { return };
    let path = video.to_str().unwrap().to_string();
    let engine = media_engine::MediaEngine::with_audio(None);
    let before = media_engine::active_decoders();
    engine.set_ambient(std::slice::from_ref(&path), &[]);
    engine.set_ambient(std::slice::from_ref(&path), &[]); // unchanged: no reopen
    let deadline = Instant::now() + Duration::from_secs(2);
    while media_engine::active_decoders() < before + 1 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(media_engine::active_decoders(), before + 1);
    let mut shown = Vec::new();
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(3000) {
        if let Some(frame) = engine.ambient_frame(&path, now_us()) {
            if shown.last() != Some(&frame.pts_us) {
                shown.push(frame.pts_us);
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    // The 2 s clip loops on a timeline that keeps increasing.
    assert!(shown.last().is_some_and(|&pts| pts > 2_200_000), "{:?}", shown.last());
    assert!(shown.windows(2).all(|w| w[1] > w[0]));
    engine.set_ambient(&[], &[]);
    assert_eq!(media_engine::active_decoders(), before);
}

fn wait_for_decoders(count: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while media_engine::active_decoders() != count && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_prepared_ambient_loop_waits_on_its_first_frame_until_shown() {
    let _serial = serial();
    let Some((_, video)) = fixtures() else { return };
    let path = video.to_str().unwrap().to_string();
    let engine = media_engine::MediaEngine::with_audio(None);
    let before = media_engine::active_decoders();
    engine.set_ambient(&[], std::slice::from_ref(&path));
    wait_for_decoders(before + 1);
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut first = None;
    while first.is_none() && Instant::now() < deadline {
        first = engine.ambient_frame(&path, now_us()).map(|f| f.pts_us);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(first.is_some(), "the first frame is ready while paused");
    std::thread::sleep(Duration::from_millis(300));
    let still = engine.ambient_frame(&path, now_us()).map(|f| f.pts_us).or(first);
    assert_eq!(still, first, "a prepared loop does not advance");

    // Shown: the same session plays on, no reopen.
    engine.set_ambient(std::slice::from_ref(&path), &[]);
    assert_eq!(media_engine::active_decoders(), before + 1);
    std::thread::sleep(Duration::from_millis(500));
    let later = engine.ambient_frame(&path, now_us()).map(|f| f.pts_us);
    assert!(later > first, "{later:?} after {first:?}");
    engine.shutdown();
    assert_eq!(media_engine::active_decoders(), before);
}

#[test]
fn a_cue_uses_the_session_prepared_for_its_file() {
    let _serial = serial();
    let Some((_, video)) = fixtures() else { return };
    let path = video.to_str().unwrap();
    let engine = media_engine::MediaEngine::with_audio(None);
    let before = media_engine::active_decoders();
    engine.prepare(Some(path));
    wait_for_decoders(before + 1);
    let options = media_engine::SessionOptions { clip: media_engine::Clip { start_us: 0, end_us: None, looping: false }, rate: 1.0, volume: 1.0, muted: false };
    let session = engine.open(7, path, options).unwrap();
    assert_eq!(media_engine::active_decoders(), before + 1, "the prepared decoder is reused");
    session.play(now_us());
    std::thread::sleep(Duration::from_millis(300));
    assert!(session.current_frame(now_us()).is_some());

    // Preparing another file never touches the foreground.
    engine.prepare(Some(path));
    wait_for_decoders(before + 2);
    engine.prepare(None);
    wait_for_decoders(before + 1);
    assert!(engine.session(7).is_some());
    engine.shutdown();
    assert_eq!(media_engine::active_decoders(), before);
}

/// A 3 s audio-only MP3, or None without FFmpeg.
fn audio_fixture() -> Option<&'static PathBuf> {
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

#[test]
fn standalone_audio_plays_on_the_audio_clock_and_ends() {
    let _serial = serial();
    let Some(path) = audio_fixture() else { return };
    let clip = Clip { start_us: 1_500_000, end_us: None, looping: false };
    let (session, audio, _device) = open(path, options(clip));
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    assert!(session.current_frame(now_us()).is_none(), "audio has no picture");
    assert!(!audio.ring.is_empty(), "audio is decoded ahead before play");
    let started = now_us();
    session.play(started);
    std::thread::sleep(Duration::from_millis(700));
    let status = session.status(now_us());
    let elapsed = now_us() - started;
    assert_eq!(status.phase, Phase::Playing);
    assert!((status.position_us - 1_500_000 - elapsed).abs() < 150_000, "position {} after {elapsed}", status.position_us);
    wait_for(&session, Phase::Ended, Duration::from_secs(3));
    assert_eq!(active_decoders(), 1);
    drop(session);
    assert_eq!(active_decoders(), 0, "closing the session joins its decoder");
}

#[test]
fn posters_are_the_first_frame_scaled_down_with_even_sizes() {
    let Some((av, _)) = fixtures() else { return };
    let poster = media_engine::poster(av.to_str().unwrap(), 101).expect("poster");
    assert_eq!((poster.width, poster.height), (100, 74), "320×240 scaled to fit 101 wide, even sizes");
    assert_eq!(poster.rgb.len(), 100 * 74 * 3);
    let large = media_engine::poster(av.to_str().unwrap(), 640).expect("poster");
    assert_eq!((large.width, large.height), (320, 240), "never upscaled");
    let Some(audio) = audio_fixture() else { return };
    assert!(media_engine::poster(audio.to_str().unwrap(), 640).is_err(), "audio has no poster");
}

#[test]
fn software_frames_upload_as_decoded_yuv() {
    let _serial = serial();
    let Some((_, video)) = fixtures() else { return };
    let session = MediaSession::open(video.to_str().unwrap(), options(whole()), None).expect("open");
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    let frame = session.current_frame(now_us()).expect("first frame");
    assert_eq!((frame.format, frame.width, frame.height), (FrameFormat::I420, 320, 240));
    assert_eq!(frame.matrix, YuvMatrix::Bt601, "standard definition without a declared matrix");
    assert!(!frame.full_range);
    let (y, stride) = frame.plane(0);
    assert!(stride >= 320 && y.len() >= stride * 240);
}

#[test]
fn a_loop_shows_its_prepared_start_again_at_each_wrap() {
    let _serial = serial();
    let Some((_, video)) = fixtures() else { return };
    let clip = Clip { start_us: 0, end_us: None, looping: true };
    let session = MediaSession::open(video.to_str().unwrap(), options(clip), None).expect("open");
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    let first = session.current_frame(now_us()).expect("first frame");
    session.play(now_us());
    // (wall time, timeline pts) of every newly shown frame across one wrap.
    let mut shown: Vec<(Instant, i64)> = Vec::new();
    let mut wrapped = None;
    let deadline = Instant::now() + Duration::from_millis(3_000);
    while Instant::now() < deadline {
        if let Some(frame) = session.current_frame(now_us()) {
            if shown.last().is_none_or(|&(_, pts)| pts != frame.pts_us) {
                if frame.pts_us >= 2_000_000 && wrapped.is_none() {
                    wrapped = Some(frame.clone());
                }
                shown.push((Instant::now(), frame.pts_us));
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let wrapped = wrapped.expect("the 2 s clip wrapped");
    assert_eq!(wrapped.pts_us, first.pts_us + 2_000_000, "the next loop starts one clip length later");
    assert!(std::sync::Arc::ptr_eq(&wrapped.data, &first.data), "the wrap reuses the prepared first frame");
    let worst = shown.windows(2).map(|w| w[1].0 - w[0].0).max().unwrap();
    assert!(worst < Duration::from_millis(120), "no gap at the loop point (worst {worst:?} at 25 fps)");
}

#[test]
fn hardware_decoding_is_used_when_chosen_and_available() {
    let _serial = serial();
    let Some((_, video)) = fixtures() else { return };
    let available = hwaccel::available();
    if available.is_empty() {
        eprintln!("No hardware decoder here; skipping");
        return;
    }
    hwaccel::set_choice(hwaccel::DecoderChoice::Auto);
    let session = MediaSession::open(video.to_str().unwrap(), options(whole()), None).expect("open");
    wait_for(&session, Phase::Ready, Duration::from_secs(5));
    let frame = session.current_frame(now_us()).expect("first frame");
    session.play(now_us());
    let shown = run_for(&session, Duration::from_millis(500));
    hwaccel::set_choice(hwaccel::DecoderChoice::Software);
    // 320x240 H.264 decodes on every accelerator that opened ({available:?}).
    assert_eq!(frame.format, FrameFormat::Nv12, "hardware frames arrive as NV12 ({available:?})");
    assert!(shown.len() >= 8, "and keep playing: {shown:?}");
}
