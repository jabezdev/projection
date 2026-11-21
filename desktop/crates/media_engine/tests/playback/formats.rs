use std::time::{Duration, Instant};

use media_engine::{active_decoders, hwaccel, now_us, Clip, FrameFormat, MediaSession, Phase, YuvMatrix};

use super::*;

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
fn hardware_decoding_plays_when_chosen_or_falls_back_to_software() {
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
    assert!(matches!(frame.format, FrameFormat::Nv12 | FrameFormat::I420), "hardware NV12 or the software fallback ({available:?}): {:?}", frame.format);
    assert!(shown.len() >= 8, "and keep playing: {shown:?}");
}
