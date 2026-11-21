

use crate::clock::now_us;

use super::*;

/// The device callback: pulls from the attached session or plays silence.
pub(super) fn feeder<T>(slot: Slot, session: DeviceFormat, device: DeviceFormat) -> impl FnMut(&mut [T], &cpal::OutputCallbackInfo) + Send + 'static
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
pub(super) struct Converter {
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
    pub(super) fn new(session: DeviceFormat, device: DeviceFormat) -> Self {
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
    pub(super) fn fill(&mut self, out: &mut [f32], render: impl FnOnce(&mut [f32])) {
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
pub(super) fn map_channel(frame: &[f32], channel: usize, device_channels: usize) -> f32 {
    match (frame.len(), device_channels) {
        (session, device) if session == device => frame[channel],
        (session, 1) => frame.iter().sum::<f32>() / session as f32,
        (1, _) => frame[0],
        (session, _) if channel < session => frame[channel],
        _ => 0.0,
    }
}
