use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use media_engine::{active_decoders, now_us, Clip, MediaSession, Phase, SessionAudio, SessionOptions};

use super::*;

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
