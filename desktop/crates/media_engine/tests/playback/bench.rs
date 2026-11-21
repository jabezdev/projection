use std::time::Duration;

use media_engine::bench::{decode_run, luma_difference};

use super::*;

#[test]
fn a_software_decode_run_counts_frames_and_samples_the_picture() {
    let _serial = serial();
    let Some((_, video)) = fixtures() else { return };
    let path = video.to_str().unwrap();
    let run = decode_run(path, None, 1_000, Duration::from_secs(10), &|| false).unwrap();
    assert_eq!(run.decoder, "software");
    assert_eq!(run.frames, 50, "2 s at 25 fps");
    assert!((run.video_seconds - 2.0).abs() < 0.05, "{}", run.video_seconds);
    assert!(run.realtime_factor() > 1.0);
    let again = decode_run(path, None, 1_000, Duration::from_secs(10), &|| false).unwrap();
    assert_eq!(luma_difference(&run, &again), Some(0.0));
}

#[test]
fn a_decode_run_stops_when_asked() {
    let _serial = serial();
    let Some((_, video)) = fixtures() else { return };
    let run = decode_run(video.to_str().unwrap(), None, 10, Duration::from_secs(10), &|| false).unwrap();
    assert!(run.frames <= 12, "{}", run.frames);
    let stopped = decode_run(video.to_str().unwrap(), None, 1_000, Duration::from_secs(10), &|| true);
    assert!(stopped.is_err(), "no frames once stopped");
}
