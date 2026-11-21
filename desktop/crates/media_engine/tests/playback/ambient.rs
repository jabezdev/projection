use std::time::{Duration, Instant};

use media_engine::now_us;

use super::*;

#[test]
fn ambient_sessions_loop_silently_and_close_on_request() {
    let _serial = serial();
    let Some((_, video)) = fixtures() else { return };
    let path = video.to_str().unwrap().to_string();
    let engine = media_engine::MediaEngine::with_audio(None);
    let before = media_engine::active_decoders();
    engine.set_ambient(std::slice::from_ref(&path), &[]);
    engine.set_ambient(std::slice::from_ref(&path), &[]);
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
    assert!(shown.last().is_some_and(|&pts| pts > 2_200_000), "{:?}", shown.last());
    assert!(shown.windows(2).all(|w| w[1] > w[0]));
    engine.set_ambient(&[], &[]);
    assert_eq!(media_engine::active_decoders(), before);
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

    engine.prepare(Some(path));
    wait_for_decoders(before + 2);
    engine.prepare(None);
    wait_for_decoders(before + 1);
    assert!(engine.session(7).is_some());
    engine.shutdown();
    assert_eq!(media_engine::active_decoders(), before);
}
