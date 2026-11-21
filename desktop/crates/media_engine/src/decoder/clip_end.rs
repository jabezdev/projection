use std::sync::atomic::Ordering;
use std::time::Duration;

use super::*;

impl Worker {
    pub(super) fn end_of_clip(&mut self) -> anyhow::Result<()> {
        if let Some(audio) = &mut self.audio {
            let _ = audio.decoder.send_eof();
        }
        if self.audio.is_some() {
            self.receive_audio()?;
            if !self.clip.looping {
                self.audio.as_ref().unwrap().out.drained.store(true, Ordering::Release);
            }
        }
        while !self.video_stash.is_empty() {
            if self.shared.interrupted() {
                return Ok(());
            }
            self.drain_video()?;
            if !self.video_stash.is_empty() {
                self.frames.wait_for_space(Duration::from_millis(20));
            }
        }
        if let Some(video) = &mut self.video {
            let _ = video.decoder.send_eof();
        }
        if self.video.is_some() {
            self.receive_video()?;
        }
        if self.shared.interrupted() {
            return Ok(());
        }
        if self.clip.looping {
            self.wrap()
        } else {
            self.shared.end_timeline_us.store(self.content_end_us + self.timeline_offset_us, Ordering::Release);
            self.shared.finished.store(true, Ordering::Release);
            Ok(())
        }
    }

    pub(super) fn wrap(&mut self) -> anyhow::Result<()> {
        let start = self.clip.start_us;
        let content_end = self.content_end_us.max(start + 1);
        let loop_len = match self.clip.end_us {
            Some(end) => end - start,
            None => content_end - start,
        }
        .max(1);
        let next_offset = if self.shared.finished.load(Ordering::Acquire) {
            self.shared.end_timeline_us.load(Ordering::Acquire) - start
        } else {
            self.timeline_offset_us + loop_len
        };
        self.seek_media(start)?;
        self.timeline_offset_us = next_offset;
        if self.capturing_head {
            self.capturing_head = false;
            self.head_complete = !self.head.is_empty();
        }
        if self.head_complete {
            for (media_us, frame) in &self.head {
                self.pending_frames.push_back(frame.retimed(self.epoch, media_us + next_offset));
            }
            self.head_pending = self.head.len();
            let last = self.head.last().map_or(start, |(media_us, _)| *media_us);
            self.video_discard_before_us = last + 1;
            self.last_video_us = Some(last);
            self.content_end_us = last + self.video_interval_us;
        } else {
            self.capturing_head = self.clip.looping;
        }
        self.shared.loop_len_us.store(loop_len, Ordering::Release);
        self.shared.finished.store(false, Ordering::Release);
        if let Some(audio) = &mut self.audio {
            audio.out.drained.store(false, Ordering::Release);
            audio.resampler = None;
        }
        Ok(())
    }
}
