use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Context;
use crossbeam::channel::{bounded, RecvTimeoutError, Sender};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceFormat {
    pub rate: u32,
    pub channels: u16,
}

pub struct AudioOutput {
    pub format: DeviceFormat,
    slot: Arc<Mutex<Option<Arc<SessionAudio>>>>,
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
    failed: Arc<AtomicBool>,
}

pub(super) static OPENED_OUTPUTS: AtomicU64 = AtomicU64::new(0);
pub(super) static RECOVERIES: AtomicU64 = AtomicU64::new(0);

pub fn opened_outputs() -> u64 {
    OPENED_OUTPUTS.load(Ordering::Relaxed)
}

pub fn audio_recoveries() -> u64 {
    RECOVERIES.load(Ordering::Relaxed)
}

pub(super) const RECOVERY_POLL: Duration = Duration::from_millis(500);

pub(super) type Slot = Arc<Mutex<Option<Arc<SessionAudio>>>>;

impl AudioOutput {
    pub fn open_default() -> anyhow::Result<Self> {
        Self::supervise(build_stream)
    }

    pub(super) fn supervise<S>(
        mut build: impl FnMut(Slot, Arc<AtomicBool>, Option<DeviceFormat>) -> anyhow::Result<(S, DeviceFormat)> + Send + 'static,
    ) -> anyhow::Result<Self> {
        let slot: Slot = Arc::default();
        let failed = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = bounded::<anyhow::Result<DeviceFormat>>(1);
        let (stop, stop_rx) = bounded::<()>(1);
        let thread = {
            let slot = slot.clone();
            let failed = failed.clone();
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

    pub fn attach(&self, audio: Arc<SessionAudio>) {
        *audio.attached.lock().expect("audio attached poisoned") = true;
        *self.slot.lock().expect("audio slot poisoned") = Some(audio);
    }

    pub fn detach(&self, audio: &Arc<SessionAudio>) {
        let mut slot = self.slot.lock().expect("audio slot poisoned");
        if slot.as_ref().is_some_and(|current| Arc::ptr_eq(current, audio)) {
            *slot = None;
        }
    }

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

pub(super) fn build_stream(slot: Slot, failed: Arc<AtomicBool>, session: Option<DeviceFormat>) -> anyhow::Result<(cpal::Stream, DeviceFormat)> {
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
        cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied => log::debug!("Audio output: {e}"),
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
