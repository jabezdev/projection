use std::time::Duration;

use media_engine::bench::DecodeRun;

use super::*;

fn measured(decoder: &str, fps: f64, cpu_ms: u64, luma: u8) -> Measured {
    let frames = 300;
    Measured {
        run: DecodeRun {
            decoder: decoder.into(),
            codec: "h264".into(),
            size: (1920, 1080),
            pixel_format: "YUV420P".into(),
            frames,
            video_seconds: 10.0,
            wall: Duration::from_secs_f64(frames as f64 / fps),
            luma: Some(vec![luma; 16]),
        },
        cpu: Duration::from_millis(cpu_ms),
    }
}

fn trial(software: Measured, hardware: Vec<(&'static str, Result<Measured, String>)>) -> Trial {
    Trial { video: "clip.mp4".into(), software: Ok(software), hardware }
}

#[test]
fn hardware_that_saves_processor_time_and_matches_the_picture_wins() {
    let trials = [trial(measured("software", 300.0, 4_000, 100), vec![("d3d11va", Ok(measured("d3d11va", 250.0, 900, 101))), ("cuda", Ok(measured("cuda", 400.0, 1_500, 100)))])];
    let pick = recommend(&trials).unwrap();
    assert_eq!(pick.choice, "d3d11va", "lowest processor time among the correct decoders");
}

#[test]
fn a_wrong_picture_or_a_slow_decoder_never_wins() {
    let trials = [trial(
        measured("software", 300.0, 4_000, 100),
        vec![("d3d11va", Ok(measured("d3d11va", 250.0, 500, 160))), ("cuda", Ok(measured("cuda", 10.0, 500, 100)))],
    )];
    assert_eq!(recommend(&trials).unwrap().choice, "software");
}

#[test]
fn a_small_saving_keeps_software() {
    let trials = [trial(measured("software", 300.0, 1_000, 100), vec![("d3d11va", Ok(measured("d3d11va", 300.0, 900, 100)))])];
    let pick = recommend(&trials).unwrap();
    assert_eq!(pick.choice, "software");
    assert!(pick.reason.contains("10 %"), "{}", pick.reason);
}

#[test]
fn when_software_is_too_slow_any_correct_hardware_wins() {
    let trials = [trial(measured("software", 20.0, 9_000, 100), vec![("cuda", Ok(measured("cuda", 120.0, 8_500, 100)))])];
    assert_eq!(recommend(&trials).unwrap().choice, "cuda");
}

#[test]
fn hardware_must_handle_every_tested_video() {
    let trials = [
        trial(measured("software", 300.0, 4_000, 100), vec![("d3d11va", Ok(measured("d3d11va", 250.0, 500, 100)))]),
        trial(measured("software", 300.0, 4_000, 100), vec![("d3d11va", Err("Not supported for this video".into()))]),
    ];
    assert_eq!(recommend(&trials).unwrap().choice, "software");
}

#[test]
fn without_videos_there_is_no_recommendation() {
    assert_eq!(recommend(&[]), None);
}
