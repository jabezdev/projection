use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::convert::*;
use super::output::*;
use super::*;

fn format(rate: u32, channels: u16) -> DeviceFormat {
    DeviceFormat { rate, channels }
}

#[test]
fn conversion_resamples_and_maps_channels_without_losing_frames() {
    // 1 kHz mono session on a 2 kHz stereo device: every device frame
    // pair spans one session frame, copied to both channels.
    let mut converter = Converter::new(format(1_000, 1), format(2_000, 2));
    let mut consumed = 0;
    let mut out = vec![0.0; 2 * 8];
    converter.fill(&mut out, |input| {
        consumed += input.len();
        for (i, sample) in input.iter_mut().enumerate() {
            *sample = (i + 1) as f32;
        }
    });
    assert_eq!(consumed, 4, "8 device frames at twice the rate use 4 session frames");
    let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
    assert_eq!(left, [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5], "interpolated from the previous block's last frame (silence)");
    assert!(out.chunks(2).all(|f| f[0] == f[1]), "mono is copied to both channels");

    // Over many blocks, consumption matches the rate ratio exactly.
    let mut total = 0;
    for _ in 0..1_000 {
        converter.fill(&mut out[..2 * 7], |input| total += input.len());
    }
    assert_eq!(total, 3_500);
}

#[test]
fn stereo_mixes_down_to_a_mono_device_and_extra_channels_are_silent() {
    assert_eq!(map_channel(&[0.2, 0.6], 0, 1), 0.4);
    assert_eq!(map_channel(&[0.2, 0.6], 1, 6), 0.6);
    assert_eq!(map_channel(&[0.2, 0.6], 4, 6), 0.0);
    let mut converter = Converter::new(format(48_000, 2), format(48_000, 1));
    let mut out = vec![0.0; 4];
    converter.fill(&mut out, |input| input.copy_from_slice(&[1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]));
    assert_eq!(out, [0.0, 0.5, 0.5, 0.5], "output runs one frame behind: it starts from the previous block's last frame");
}

#[test]
fn a_failed_stream_is_reopened_with_the_original_session_format() {
    use std::sync::atomic::AtomicUsize;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let failures = Arc::new(AtomicUsize::new(0));
    let flag: Arc<Mutex<Option<Arc<AtomicBool>>>> = Arc::default();
    let output = {
        let (calls, failures, flag) = (calls.clone(), failures.clone(), flag.clone());
        AudioOutput::supervise(move |_slot, failed, session| {
            calls.lock().unwrap().push(session);
            *flag.lock().unwrap() = Some(failed);
            // The device is gone for one retry, then back with 44.1 kHz.
            if session.is_some() && failures.fetch_add(1, Ordering::AcqRel) == 0 {
                anyhow::bail!("device unavailable");
            }
            Ok(((), session.unwrap_or(format(48_000, 2))))
        })
        .unwrap()
    };
    assert_eq!(output.format, format(48_000, 2));
    flag.lock().unwrap().as_ref().unwrap().store(true, Ordering::Release);
    assert!(output.failed());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while calls.lock().unwrap().len() < 3 || output.failed() {
        assert!(std::time::Instant::now() < deadline, "not recovered: {:?}", calls.lock().unwrap());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(*calls.lock().unwrap(), [None, Some(format(48_000, 2)), Some(format(48_000, 2))]);
    assert_eq!(output.format, format(48_000, 2), "sessions keep their format");
}

#[test]
fn paused_sessions_output_silence_and_hold_the_clock() {
    let audio = SessionAudio::new(1_000, 1);
    audio.ring.push(&[0.5; 100]);
    let mut out = [1.0; 10];
    audio.render(&mut out, 0);
    assert_eq!(out, [0.0; 10]);
    assert_eq!(audio.position(1_000_000), 0);
    assert_eq!(audio.ring.len(), 100, "nothing consumed while paused");
}

#[test]
fn running_sessions_apply_gain_and_advance_the_clock_by_frames_heard() {
    let audio = SessionAudio::new(1_000, 2);
    audio.ring.push(&[0.5; 40]);
    audio.set_running(true);
    audio.set_gain(0.5);
    let mut out = [0.0; 20];
    audio.render(&mut out, 1_000_000);
    assert_eq!(out, [0.25; 20]);
    // 10 frames at 1 kHz = 10 ms, heard from t = 1 s.
    assert_eq!(audio.position(1_000_000), 0);
    assert_eq!(audio.position(1_010_000), 10_000);
    audio.render(&mut out, 1_010_000);
    assert_eq!(audio.position(1_015_000), 15_000);
}

#[test]
fn a_restart_reanchors_the_clock_once_the_consumer_flushes() {
    let audio = Arc::new(SessionAudio::new(1_000, 1));
    *audio.attached.lock().unwrap() = true;
    audio.set_running(true);
    audio.ring.push(&[0.1; 50]);
    let mut out = [0.0; 10];
    audio.render(&mut out, 0);
    let consumer = {
        let audio = audio.clone();
        std::thread::spawn(move || {
            let mut out = [0.0; 10];
            for i in 0..100 {
                audio.render(&mut out, i * 10_000);
                std::thread::sleep(Duration::from_millis(1));
            }
        })
    };
    audio.restart(60_000_000, 1.0);
    assert!(audio.ring.is_empty());
    assert_eq!(audio.position(0), 60_000_000);
    consumer.join().unwrap();
    assert_eq!(audio.position(10_000_000), 60_000_000, "no samples after the flush, so no progress");
}

#[test]
fn a_session_never_routed_to_the_device_restarts_without_waiting() {
    let audio = SessionAudio::new(1_000, 1);
    audio.ring.push(&[0.1; 50]);
    let started = std::time::Instant::now();
    audio.restart(5_000_000, 1.0);
    assert!(started.elapsed() < Duration::from_millis(100), "a prepared session must not wait for a consumer");
    assert!(audio.ring.is_empty());
    assert_eq!(audio.position(0), 5_000_000);
}
