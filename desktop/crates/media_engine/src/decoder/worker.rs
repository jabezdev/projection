use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Context;
use crossbeam::channel::RecvTimeoutError;
use ffmpeg_next as ffmpeg;

use crate::hwaccel::{self, HwDecoder};

use super::*;

pub(crate) fn decode_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get()).min(4)
}

pub(super) fn open_video(input: &ffmpeg::format::context::Input, index: usize, hardware: bool) -> anyhow::Result<(ffmpeg::decoder::Video, Option<HwDecoder>)> {
    let stream = input.stream(index).context("Missing video stream")?;
    let context = || ffmpeg::codec::context::Context::from_parameters(stream.parameters());
    if hardware {
        let mut hw_context = context()?;
        let codec = ffmpeg::decoder::find(hw_context.id());
        if let Some(hw) = codec.and_then(|codec| hwaccel::attach(&mut hw_context, unsafe { codec.as_ptr() })) {
            match hw_context.decoder().video() {
                Ok(decoder) => return Ok((decoder, Some(hw))),
                Err(e) => log::warn!("Opening the {} decoder failed ({e}); decoding in software", hw.name),
            }
        }
    }
    let mut context = context()?;
    context.set_threading(ffmpeg::codec::threading::Config {
        kind: ffmpeg::codec::threading::Type::Frame,
        count: decode_threads(),
        ..Default::default()
    });
    Ok((context.decoder().video()?, None))
}

impl Worker {
    pub(super) fn open(params: DecoderParams) -> anyhow::Result<Self> {
        ffmpeg::init()?;
        let input = ffmpeg::format::input(&params.path).with_context(|| format!("Opening {}", params.path))?;
        let origin_us = unsafe { (*input.as_ptr()).start_time };
        let origin_us = if origin_us == ffmpeg::ffi::AV_NOPTS_VALUE { 0 } else { origin_us };
        let duration = input.duration();
        if duration > 0 {
            params.shared.duration_us.store(duration, Ordering::Release);
        }

        let video = match input.streams().best(ffmpeg::media::Type::Video) {
            Some(stream) => {
                let (decoder, hw) = open_video(&input, stream.index(), true)?;
                Some(VideoStream { index: stream.index(), decoder, time_base: f64::from(stream.time_base()), scaler: None, hw, reported: false })
            }
            None => None,
        };
        let audio = match (input.streams().best(ffmpeg::media::Type::Audio), &params.audio) {
            (Some(stream), Some(out)) => {
                let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?;
                Some(AudioStream {
                    index: stream.index(),
                    decoder: context.decoder().audio()?,
                    time_base: f64::from(stream.time_base()),
                    out: out.clone(),
                    resampler: None,
                })
            }
            _ => None,
        };
        anyhow::ensure!(video.is_some() || audio.is_some(), "No playable video or audio stream");
        params.shared.has_video.store(video.is_some(), Ordering::Release);
        params.shared.has_audio.store(audio.is_some(), Ordering::Release);

        let mut worker = Self {
            input,
            video,
            audio,
            origin_us,
            clip: params.clip,
            rate: params.rate,
            epoch: 0,
            frames: params.frames,
            shared: params.shared,
            controls: params.controls,
            clock: params.clock,
            video_discard_before_us: 0,
            audio_discard_before_us: 0,
            audio_end_us: 0,
            timeline_offset_us: 0,
            content_end_us: 0,
            clip_end_reached: false,
            last_video_us: None,
            video_interval_us: 33_333,
            video_stash: VecDeque::new(),
            stash_bytes: 0,
            pending_frames: VecDeque::new(),
            head: Vec::new(),
            head_complete: false,
            capturing_head: false,
            head_pending: 0,
            hw_failure: None,
            decode_time: Duration::ZERO,
        };
        worker.restart(0, params.clip.start_us, params.rate)?;
        worker.shared.opened.store(true, Ordering::Release);
        Ok(worker)
    }

    pub(super) fn run(mut self) -> anyhow::Result<()> {
        loop {
            while let Ok(control) = self.controls.try_recv() {
                if let Flow::Close = self.apply(control)? {
                    return Ok(());
                }
            }
            if let Some(reason) = self.hw_failure.take() {
                self.fall_back_to_software(&reason)?;
            }
            if self.shared.finished.load(Ordering::Acquire) {
                self.drain_video()?;
                match self.controls.recv_timeout(Duration::from_millis(20)) {
                    Ok(control) => {
                        if let Flow::Close = self.apply(control)? {
                            return Ok(());
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return Ok(()),
                }
                continue;
            }
            if self.shared.interrupted() {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            self.drain_video()?;
            if self.clip_end_reached {
                self.end_of_clip()?;
                continue;
            }
            if self.read_ahead_full() {
                self.frames.wait_for_space(Duration::from_millis(20));
                continue;
            }
            let mut packet = ffmpeg::Packet::empty();
            match packet.read(&mut self.input) {
                Ok(()) => self.decode_packet(packet)?,
                Err(ffmpeg::Error::Eof) => self.end_of_clip()?,
                Err(e) => return Err(e).context("Reading media"),
            }
        }
    }

    fn read_ahead_full(&self) -> bool {
        let audio_wants_data = self.audio.as_ref().is_some_and(|audio| audio.out.ring.len() < audio.out.ring.capacity() / 2);
        self.stash_bytes > STASH_LIMIT_BYTES || (self.stash_bytes > IDLE_STASH_BYTES && !audio_wants_data)
    }

    pub(super) fn apply(&mut self, control: Control) -> anyhow::Result<Flow> {
        let flow = match control {
            Control::Restart { epoch, position_us, rate } => {
                self.restart(epoch, position_us, rate)?;
                Flow::Continue
            }
            Control::SetClip(clip) => {
                let was_finished = self.shared.finished.load(Ordering::Acquire);
                if clip.start_us != self.clip.start_us || !clip.looping {
                    self.forget_head();
                }
                self.clip = clip;
                if was_finished && clip.looping {
                    self.wrap()?;
                }
                return Ok(Flow::Continue);
            }
            Control::Close => Flow::Close,
        };
        self.shared.interrupts.fetch_sub(1, Ordering::AcqRel);
        Ok(flow)
    }

    pub(super) fn restart(&mut self, epoch: u64, position_us: i64, rate: f64) -> anyhow::Result<()> {
        self.epoch = epoch;
        self.pending_frames.clear();
        self.rate = rate;
        self.frames.flush(epoch);
        if let Some(audio) = &mut self.audio {
            audio.out.restart(position_us, rate);
            audio.resampler = None;
        }
        self.seek_media(position_us)?;
        self.head_pending = 0;
        if !self.head_complete {
            self.head.clear();
            self.capturing_head = self.clip.looping && position_us == self.clip.start_us;
        }
        self.timeline_offset_us = 0;
        self.shared.loop_len_us.store(-1, Ordering::Release);
        self.shared.finished.store(false, Ordering::Release);
        self.shared.applied_epoch.store(epoch, Ordering::Release);
        Ok(())
    }

    pub(super) fn seek_media(&mut self, position_us: i64) -> anyhow::Result<()> {
        let target = position_us + self.origin_us;
        if position_us > 0 {
            self.input.seek(target, ..target).context("Seeking")?;
        } else {
            self.input.seek(self.origin_us, ..self.origin_us).context("Seeking")?;
        }
        if let Some(video) = &mut self.video {
            video.decoder.flush();
        }
        if let Some(audio) = &mut self.audio {
            audio.decoder.flush();
        }
        self.video_discard_before_us = position_us;
        self.audio_discard_before_us = position_us;
        self.audio_end_us = position_us;
        self.content_end_us = position_us;
        self.last_video_us = None;
        self.video_stash.clear();
        self.stash_bytes = 0;
        self.clip_end_reached = false;
        Ok(())
    }

}
