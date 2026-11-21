use std::time::Instant;

use anyhow::Context;
use ffmpeg_next as ffmpeg;

use crate::frames::VideoFrame;
use crate::hwaccel::{self};
use crate::profile;

use super::*;

impl Worker {
    pub(super) fn decode_packet(&mut self, packet: ffmpeg::Packet) -> anyhow::Result<()> {
        let index = packet.stream();
        if self.video.as_ref().is_some_and(|v| v.index == index) {
            self.stash_bytes += packet.size();
            self.video_stash.push_back(packet);
            self.drain_video()?;
        } else if self.audio.as_ref().is_some_and(|a| a.index == index) {
            self.audio.as_mut().unwrap().decoder.send_packet(&packet)?;
            self.receive_audio()?;
        }
        Ok(())
    }

    pub(super) fn drain_video(&mut self) -> anyhow::Result<()> {
        loop {
            while let Some(frame) = self.pending_frames.pop_front() {
                if let Err(frame) = self.frames.try_push(frame, (self.clock)()) {
                    self.pending_frames.push_front(frame);
                    break;
                }
                self.head_pending = self.head_pending.saturating_sub(1);
            }
            if self.pending_frames.len() > self.head_pending || self.shared.interrupted() || self.hw_failure.is_some() {
                return Ok(());
            }
            let Some(packet) = self.video_stash.pop_front() else { return Ok(()) };
            self.stash_bytes -= packet.size();
            let started = Instant::now();
            let video = self.video.as_mut().unwrap();
            let sent = video.decoder.send_packet(&packet);
            self.decode_time += started.elapsed();
            match sent {
                Ok(()) => {}
                Err(e) if video.hw.is_some() => {
                    self.hw_failure = Some(e.to_string());
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            }
            self.receive_video()?;
        }
    }

    pub(super) fn receive_video(&mut self) -> anyhow::Result<()> {
        let mut decoded = ffmpeg::frame::Video::empty();
        while !self.shared.interrupted() && self.hw_failure.is_none() {
            let started = Instant::now();
            let video = self.video.as_mut().unwrap();
            let received = video.decoder.receive_frame(&mut decoded);
            self.decode_time += started.elapsed();
            match received {
                Ok(()) => {
                    profile::DECODE.record(std::mem::take(&mut self.decode_time));
                    self.handle_video(&decoded)?;
                }
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::util::error::EAGAIN => break,
                Err(ffmpeg::Error::Eof) => break,
                Err(e) if video.hw.is_some() => self.hw_failure = Some(e.to_string()),
                Err(_) => break,
            }
        }
        Ok(())
    }

    pub(super) fn fall_back_to_software(&mut self, reason: &str) -> anyhow::Result<()> {
        let Some(index) = self.video.as_ref().filter(|v| v.hw.is_some()).map(|v| v.index) else { return Ok(()) };
        log::warn!("Hardware decoding failed ({reason}); continuing in software");
        let (decoder, _) = open_video(&self.input, index, false)?;
        let video = self.video.as_mut().unwrap();
        video.decoder = decoder;
        video.hw = None;
        video.scaler = None;
        let resume = self.last_video_us.map_or(self.video_discard_before_us, |last| last + 1);
        let target = resume + self.origin_us;
        self.input.seek(target, ..target).context("Seeking")?;
        if let Some(audio) = &mut self.audio {
            audio.decoder.flush();
        }
        self.video_discard_before_us = resume;
        self.audio_discard_before_us = self.audio_discard_before_us.max(self.audio_end_us);
        self.video_stash.clear();
        self.stash_bytes = 0;
        self.clip_end_reached = false;
        Ok(())
    }

    pub(super) fn forget_head(&mut self) {
        self.head.clear();
        self.head_complete = false;
        self.capturing_head = false;
    }

    pub(super) fn media_us(&self, timestamp: Option<i64>, time_base: f64) -> Option<i64> {
        timestamp.map(|ts| (ts as f64 * time_base * 1e6) as i64 - self.origin_us)
    }

    pub(super) fn handle_video(&mut self, decoded: &ffmpeg::frame::Video) -> anyhow::Result<()> {
        let time_base = self.video.as_ref().unwrap().time_base;
        let Some(media_us) = self.media_us(decoded.timestamp().or(decoded.pts()), time_base) else {
            return Ok(());
        };
        if self.clip.end_us.is_some_and(|end| media_us >= end) {
            self.clip_end_reached = true;
            return Ok(());
        }
        if media_us < self.video_discard_before_us {
            return Ok(());
        }
        let video = self.video.as_mut().unwrap();
        let on_gpu = video.hw.is_some_and(|hw| ffmpeg::ffi::AVPixelFormat::from(decoded.format()) == hw.format);
        if !video.reported {
            video.reported = true;
            match video.hw {
                Some(hw) if on_gpu => log::info!("Decoding {}x{} video with {}", decoded.width(), decoded.height(), hw.name),
                _ => log::info!("Decoding {}x{} {:?} video in software", decoded.width(), decoded.height(), decoded.format()),
            }
        }
        let downloaded;
        let source = if on_gpu {
            let started = Instant::now();
            match hwaccel::download(decoded) {
                Ok(frame) => {
                    profile::TRANSFER.record(started.elapsed());
                    downloaded = frame;
                    &downloaded
                }
                Err(e) => {
                    self.hw_failure = Some(format!("copying a frame from the GPU: {e}"));
                    return Ok(());
                }
            }
        } else {
            decoded
        };
        let started = Instant::now();
        let packed = pack(video, source)?;
        profile::CONVERT.record(started.elapsed());
        let frame = VideoFrame {
            id: VideoFrame::next_id(),
            epoch: self.epoch,
            pts_us: media_us + self.timeline_offset_us,
            width: packed.width,
            height: packed.height,
            format: packed.format,
            matrix: packed.matrix,
            full_range: packed.full_range,
            planes: packed.planes,
            data: packed.data,
        };
        if self.capturing_head {
            if self.head.len() < LOOP_HEAD_FRAMES && media_us - self.clip.start_us < LOOP_HEAD_US {
                self.head.push((media_us, frame.retimed(0, 0)));
            } else {
                self.capturing_head = false;
                self.head_complete = !self.head.is_empty();
            }
        }
        if let Some(previous) = self.last_video_us.filter(|&p| media_us > p) {
            self.video_interval_us = media_us - previous;
        }
        self.last_video_us = Some(media_us);
        self.content_end_us = self.content_end_us.max(media_us + self.video_interval_us);
        self.pending_frames.push_back(frame);
        Ok(())
    }

}
