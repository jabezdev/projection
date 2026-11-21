use std::time::Duration;

use ffmpeg_next as ffmpeg;

use super::*;

impl Worker {
    pub(super) fn receive_audio(&mut self) -> anyhow::Result<()> {
        let mut decoded = ffmpeg::frame::Audio::empty();
        while !self.shared.interrupted() && self.audio.as_mut().unwrap().decoder.receive_frame(&mut decoded).is_ok() {
            self.handle_audio(&decoded)?;
        }
        Ok(())
    }

    pub(super) fn handle_audio(&mut self, decoded: &ffmpeg::frame::Audio) -> anyhow::Result<()> {
        let rate = self.rate;
        let audio = self.audio.as_mut().unwrap();
        let Some(start_us) = decoded
            .timestamp()
            .or(decoded.pts())
            .map(|ts| (ts as f64 * audio.time_base * 1e6) as i64 - self.origin_us)
        else {
            return Ok(());
        };
        let out = audio.out.clone();
        let output_rate = (out.device_rate as f64 / rate).round() as u32;
        let key = (decoded.format(), decoded.rate(), decoded.channels());
        if audio.resampler.as_ref().is_none_or(|(_, k)| *k != key) {
            let layout = if decoded.channel_layout().channels() > 0 {
                decoded.channel_layout()
            } else {
                ffmpeg::ChannelLayout::default(decoded.channels() as i32)
            };
            let context = ffmpeg::software::resampling::Context::get(
                decoded.format(),
                layout,
                decoded.rate(),
                ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
                ffmpeg::ChannelLayout::default(out.channels as i32),
                output_rate,
            )?;
            audio.resampler = Some((context, key));
        }
        let capacity = (decoded.samples() as f64 * output_rate as f64 / decoded.rate() as f64) as usize + 256;
        let mut converted = ffmpeg::frame::Audio::new(
            ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
            capacity,
            ffmpeg::ChannelLayout::default(out.channels as i32),
        );
        audio.resampler.as_mut().unwrap().0.run(decoded, &mut converted)?;
        let channels = out.channels as usize;
        let frames = converted.samples();
        let data = converted.data(0);
        let all: &[f32] = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, frames * channels) };

        let us_per_frame = 1e6 / output_rate as f64;
        let skip = (((self.audio_discard_before_us - start_us) as f64 / us_per_frame).ceil().max(0.0) as usize).min(frames);
        let keep = match self.clip.end_us {
            Some(end) => {
                let until = (((end - start_us) as f64 / us_per_frame).floor().max(0.0) as usize).min(frames);
                if until < frames && self.video.is_none() {
                    self.clip_end_reached = true;
                }
                until
            }
            None => frames,
        };
        if skip >= keep {
            return Ok(());
        }
        let end_us = start_us + (keep as f64 * us_per_frame) as i64;
        self.content_end_us = self.content_end_us.max(end_us);
        self.audio_end_us = self.audio_end_us.max(end_us);
        let mut pending = &all[skip * channels..keep * channels];
        while !pending.is_empty() {
            if self.shared.interrupted() {
                return Ok(());
            }
            let written = out.ring.push(pending);
            pending = &pending[written..];
            if !pending.is_empty() {
                self.drain_video()?;
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        Ok(())
    }

}
