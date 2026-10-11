//! The application's single audio output and the per-session sample feed.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Context;
use crossbeam::channel::{bounded, RecvTimeoutError, Sender};

use crate::clock::{audio_position, now_us, Anchor, AudioTiming, AudioTimingCell};
use crate::ring::SampleRing;

/// Seconds of audio buffered ahead of the device.
const RING_SECONDS: f64 = 1.0;

/// One session's interleaved f32 samples at the device rate and channel count.
pub struct SessionAudio {
    pub ring: SampleRing,
    pub channels: u16,
    pub device_rate: u32,
    running: AtomicBool,
    gain: AtomicU32,
    /// Set by the decoder when no more samples will come (end of clip).
    pub drained: AtomicBool,
    anchor: Mutex<Anchor>,
    timing: AudioTimingCell,
    // Consumer-side counters.
    frames_since_flush: AtomicU64,
    epoch_seen: AtomicU64,
    pub underruns: AtomicU64,
}

impl SessionAudio {
    pub fn new(device_rate: u32, channels: u16) -> Self {
        let capacity = (device_rate as f64 * channels as f64 * RING_SECONDS) as usize;
        Self {
            ring: SampleRing::new(capacity),
            channels,
            device_rate,
            running: AtomicBool::new(false),
            gain: AtomicU32::new(1f32.to_bits()),
            drained: AtomicBool::new(false),
            anchor: Mutex::new(Anchor { epoch: 0, position_us: 0, rate: 1.0 }),
            timing: AudioTimingCell::default(),
            frames_since_flush: AtomicU64::new(0),
            epoch_seen: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
        }
    }

    pub fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::Release);
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain.store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Release);
    }

    /// Producer: discards queued audio and anchors the clock at `position_us`.
    pub fn restart(&self, position_us: i64, rate: f64) {
        let epoch = self.ring.flush(Duration::from_millis(250));
        self.drained.store(false, Ordering::Release);
        *self.anchor.lock().expect("audio anchor poisoned") = Anchor { epoch, position_us, rate };
    }

    /// Timeline position being heard at `now_us`.
    pub fn position(&self, now_us: i64) -> i64 {
        let anchor = *self.anchor.lock().expect("audio anchor poisoned");
        audio_position(anchor, self.timing.get(), self.device_rate, now_us)
    }

    /// Device callback: fills `out` (interleaved, device channel count) whose
    /// first frame is heard at `heard_at_us`. Never blocks or allocates.
    pub fn render(&self, out: &mut [f32], heard_at_us: i64) {
        self.ring.service_flush();
        let epoch = self.ring.epoch();
        if self.epoch_seen.swap(epoch, Ordering::AcqRel) != epoch {
            self.frames_since_flush.store(0, Ordering::Release);
        }
        let read = if self.running.load(Ordering::Acquire) { self.ring.pop(out) } else { 0 };
        let gain = f32::from_bits(self.gain.load(Ordering::Acquire));
        for sample in &mut out[..read] {
            *sample *= gain;
        }
        out[read..].fill(0.0);
        if self.running.load(Ordering::Acquire) && read < out.len() && !self.drained.load(Ordering::Acquire) {
            self.underruns.fetch_add(1, Ordering::Relaxed);
        }
        let frames = (read / self.channels.max(1) as usize) as u64;
        let before = self.frames_since_flush.fetch_add(frames, Ordering::AcqRel);
        self.timing.publish(AudioTiming { epoch, frames_before_block: before, block_frames: frames, block_heard_at_us: heard_at_us });
    }
}

/// The device format sessions must produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceFormat {
    pub rate: u32,
    pub channels: u16,
}

/// Exactly one output stream on the system default device, open for the
/// application's lifetime. Idle, it plays silence. If the device fails (for
/// example, it is unplugged), the stream is reopened on whatever the default
/// device is then.
pub struct AudioOutput {
    /// The format sessions produce. It never changes: a replacement device
    /// with another format is fed through a converter.
    pub format: DeviceFormat,
    slot: Arc<Mutex<Option<Arc<SessionAudio>>>>,
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
    failed: Arc<AtomicBool>,
}

static OPENED_OUTPUTS: AtomicU64 = AtomicU64::new(0);
static RECOVERIES: AtomicU64 = AtomicU64::new(0);

/// How many audio outputs this process has opened (should be one).
pub fn opened_outputs() -> u64 {
    OPENED_OUTPUTS.load(Ordering::Relaxed)
}

/// How many times a failed device stream has been reopened.
pub fn audio_recoveries() -> u64 {
    RECOVERIES.load(Ordering::Relaxed)
}

/// How often a failed device is retried.
const RECOVERY_POLL: Duration = Duration::from_millis(500);

type Slot = Arc<Mutex<Option<Arc<SessionAudio>>>>;

impl AudioOutput {
    pub fn open_default() -> anyhow::Result<Self> {
        Self::supervise(build_stream)
    }

    /// Opens a stream with `build` and keeps one open until dropped. `build`
    /// receives the session format to convert from once one is established.
    fn supervise<S>(
        mut build: impl FnMut(Slot, Arc<AtomicBool>, Option<DeviceFormat>) -> anyhow::Result<(S, DeviceFormat)> + Send + 'static,
    ) -> anyhow::Result<Self> {
        let slot: Slot = Arc::default();
        let failed = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = bounded::<anyhow::Result<DeviceFormat>>(1);
        let (stop, stop_rx) = bounded::<()>(1);
        let thread = {
            let slot = slot.clone();
            let failed = failed.clone();
            // cpal streams are not Send; this thread owns the stream until stopped.
            std::thread::Builder::new().name("audio-output".into()).spawn(move || {
                let (stream, format) = match build(slot.clone(), failed.clone(), None) {
                    Ok(opened) => opened,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(format));
                let mut stream = Some(stream);
                let mut retrying = false;
                loop {
                    match stop_rx.recv_timeout(RECOVERY_POLL) {
                        Err(RecvTimeoutError::Timeout) => {}
                        Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                    }
                    if !failed.load(Ordering::Acquire) {
                        continue;
                    }
                    // Release the failed stream before opening its replacement.
                    stream = None;
                    failed.store(false, Ordering::Release);
                    match build(slot.clone(), failed.clone(), Some(format)) {
                        Ok((replacement, _)) => {
                            stream = Some(replacement);
                            retrying = false;
                            RECOVERIES.fetch_add(1, Ordering::Relaxed);
                            log::info!("Audio output reopened on the default device");
                        }
                        Err(e) => {
                            failed.store(true, Ordering::Release);
                            if !retrying {
                                log::warn!("Audio output unavailable, retrying: {e:#}");
                                retrying = true;
                            }
                        }
                    }
                }
                drop(stream);
            })?
        };
        let format = ready_rx.recv().context("Audio output thread exited")??;
        OPENED_OUTPUTS.fetch_add(1, Ordering::Relaxed);
        log::info!("Audio output: {} Hz, {} channels", format.rate, format.channels);
        Ok(Self { format, slot, stop, thread: Some(thread), failed })
    }

    /// Routes `audio` to the device, replacing any previous session.
    pub fn attach(&self, audio: Arc<SessionAudio>) {
        *self.slot.lock().expect("audio slot poisoned") = Some(audio);
    }

    /// Stops feeding `audio` if it is the attached session.
    pub fn detach(&self, audio: &Arc<SessionAudio>) {
        let mut slot = self.slot.lock().expect("audio slot poisoned");
        if slot.as_ref().is_some_and(|current| Arc::ptr_eq(current, audio)) {
            *slot = None;
        }
    }

    /// The device stream failed and has not been reopened yet.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Opens the default device. Returns the stream and the session format:
/// `session` if given (converting when the device differs), else the device's.
fn build_stream(slot: Slot, failed: Arc<AtomicBool>, session: Option<DeviceFormat>) -> anyhow::Result<(cpal::Stream, DeviceFormat)> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let device = cpal::default_host().default_output_device().context("No audio output device")?;
    let supported = device.default_output_config()?;
    let config: cpal::StreamConfig = supported.config();
    let device_format = DeviceFormat { rate: config.sample_rate, channels: config.channels };
    let session = session.unwrap_or(device_format);
    if session != device_format {
        log::warn!("Audio device format {device_format:?} differs from {session:?}; converting");
    }
    let on_error = move |e: cpal::Error| match e.kind() {
        // The stream keeps playing; only a glitch or lower priority results.
        cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied => log::debug!("Audio output: {e}"),
        // Includes a rerouted default device: reopening reads its new format.
        _ => {
            log::error!("Audio output error: {e}");
            failed.store(true, Ordering::Release);
        }
    };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(config, feeder::<f32>(slot, session, device_format), on_error, None)?,
        cpal::SampleFormat::I16 => device.build_output_stream(config, feeder::<i16>(slot, session, device_format), on_error, None)?,
        cpal::SampleFormat::U16 => device.build_output_stream(config, feeder::<u16>(slot, session, device_format), on_error, None)?,
        other => anyhow::bail!("Unsupported audio sample format {other:?}"),
    };
    stream.play()?;
    Ok((stream, session))
}

/// The device callback: pulls from the attached session or plays silence.
fn feeder<T>(slot: Slot, session: DeviceFormat, device: DeviceFormat) -> impl FnMut(&mut [T], &cpal::OutputCallbackInfo) + Send + 'static
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    // Grown only if the device asks for a larger block than ever before.
    let mut scratch = vec![0f32; 16_384];
    let mut converter = (session != device).then(|| Converter::new(session, device));
    move |data: &mut [T], info| {
        let stamp = info.timestamp();
        let latency = stamp.playback.saturating_duration_since(stamp.callback);
        let heard_at = now_us() + latency.as_micros() as i64;
        if scratch.len() < data.len() {
            scratch.resize(data.len(), 0.0);
        }
        let block = &mut scratch[..data.len()];
        match (slot.try_lock().ok().as_deref().and_then(|s| s.clone()), &mut converter) {
            (Some(audio), Some(converter)) => converter.fill(block, |input| audio.render(input, heard_at)),
            (Some(audio), None) => audio.render(block, heard_at),
            (None, _) => block.fill(0.0),
        }
        for (out, &sample) in data.iter_mut().zip(block.iter()) {
            *out = T::from_sample(sample);
        }
    }
}

/// Feeds a device whose format differs from the sessions' (after reopening
/// on another device): linear resampling and channel mapping. Sessions keep
/// counting frames in their own format, so the audio clock stays correct.
struct Converter {
    session: DeviceFormat,
    device: DeviceFormat,
    /// Session frames per device frame.
    step: f64,
    /// Offset of the next device frame past `input[0]`, in session frames.
    frac: f64,
    /// The previous block's last session frame, then this block's frames.
    input: Vec<f32>,
}

impl Converter {
    fn new(session: DeviceFormat, device: DeviceFormat) -> Self {
        let channels = session.channels.max(1) as usize;
        Self {
            session,
            device,
            step: f64::from(session.rate) / f64::from(device.rate.max(1)),
            frac: 0.0,
            input: vec![0.0; channels * 8_192],
        }
    }

    /// Fills `out` (device format) from `render`, which fills a block of
    /// session frames.
    fn fill(&mut self, out: &mut [f32], render: impl FnOnce(&mut [f32])) {
        let session_channels = self.session.channels.max(1) as usize;
        let device_channels = self.device.channels.max(1) as usize;
        let frames = out.len() / device_channels;
        let end = self.frac + frames as f64 * self.step;
        let needed = end.floor() as usize;
        let len = (needed + 1) * session_channels;
        if self.input.len() < len {
            self.input.resize(len, 0.0);
        }
        render(&mut self.input[session_channels..len]);
        let frame = |index: usize| &self.input[index * session_channels..(index + 1) * session_channels];
        for k in 0..frames {
            let position = self.frac + k as f64 * self.step;
            let i = (position.floor() as usize).min(needed);
            let j = (i + 1).min(needed);
            let weight = (position - i as f64).clamp(0.0, 1.0) as f32;
            for c in 0..device_channels {
                let (a, b) = (map_channel(frame(i), c, device_channels), map_channel(frame(j), c, device_channels));
                out[k * device_channels + c] = a + (b - a) * weight;
            }
        }
        out[frames * device_channels..].fill(0.0);
        self.input.copy_within(needed * session_channels..len, 0);
        self.frac = end - needed as f64;
    }
}

/// Device channel `channel` of `frame`: mono is copied to every channel,
/// many channels mix down to mono, and extra device channels stay silent.
fn map_channel(frame: &[f32], channel: usize, device_channels: usize) -> f32 {
    match (frame.len(), device_channels) {
        (session, device) if session == device => frame[channel],
        (session, 1) => frame.iter().sum::<f32>() / session as f32,
        (1, _) => frame[0],
        (session, _) if channel < session => frame[channel],
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
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
}
