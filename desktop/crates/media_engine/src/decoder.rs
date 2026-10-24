//! One worker thread per session: demux, decode, convert, and feed the frame
//! queue and audio ring. Restarts (seek, rate change) flush both outputs so no
//! obsolete frame or sample is ever presented.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use crossbeam::channel::{Receiver, RecvTimeoutError};
use ffmpeg_next as ffmpeg;

use crate::audio::SessionAudio;
use crate::frames::{FrameFormat, FrameQueue, Plane, VideoFrame, YuvMatrix};
use crate::hwaccel::{self, HwDecoder};
use crate::profile;

/// Compressed video held while the frame queue is full, so audio decoding
/// (which drives the clock) never waits behind video.
const STASH_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// RGBA frames are scaled down to fit this box; outputs are 1080p.
pub const MAX_FRAME: (u32, u32) = (1920, 1080);

/// YUV frames up to this size go to the GPU as decoded, which converts and
/// scales them; larger ones are scaled down to [`MAX_FRAME`] first.
pub const MAX_YUV_FRAME: (u32, u32) = (4096, 2304);

/// A looping clip keeps its first frames (up to this many, covering up to
/// [`LOOP_HEAD_US`]) to show again at each wrap while the decoder seeks back
/// and catches up, so the loop point has no gap.
const LOOP_HEAD_FRAMES: usize = 6;
const LOOP_HEAD_US: i64 = 250_000;

static ACTIVE_DECODERS: AtomicUsize = AtomicUsize::new(0);

/// Decoder threads currently alive in this process.
pub fn active_decoders() -> usize {
    ACTIVE_DECODERS.load(Ordering::Acquire)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clip {
    pub start_us: i64,
    pub end_us: Option<i64>,
    pub looping: bool,
}

pub enum Control {
    /// Flush and continue decoding from `position_us` at `rate`.
    Restart { epoch: u64, position_us: i64, rate: f64 },
    /// New clip bounds or loop flag; takes effect without a flush.
    SetClip(Clip),
    Close,
}

/// What the worker publishes for the session.
#[derive(Default)]
pub struct DecoderShared {
    pub has_video: AtomicBool,
    pub has_audio: AtomicBool,
    pub opened: AtomicBool,
    /// Probed media duration in µs, or -1 when unknown.
    pub duration_us: AtomicI64,
    /// The last restart epoch the worker has applied.
    pub applied_epoch: AtomicU64,
    /// Restarts/closes queued but not yet applied; decoding yields while > 0.
    pub interrupts: AtomicUsize,
    /// Non-looping clip fully decoded; `end_timeline_us` is its last instant.
    pub finished: AtomicBool,
    pub end_timeline_us: AtomicI64,
    /// After the first loop wrap: media length of one loop, else -1.
    pub loop_len_us: AtomicI64,
    pub error: Mutex<Option<String>>,
}

impl DecoderShared {
    pub fn new() -> Self {
        let shared = Self::default();
        shared.duration_us.store(-1, Ordering::Relaxed);
        shared.loop_len_us.store(-1, Ordering::Relaxed);
        shared
    }

    fn fail(&self, message: String) {
        log::error!("Media decode failed: {message}");
        *self.error.lock().expect("decoder error poisoned") = Some(message);
    }

    fn interrupted(&self) -> bool {
        self.interrupts.load(Ordering::Acquire) > 0
    }
}

pub struct DecoderParams {
    pub path: String,
    pub clip: Clip,
    pub rate: f64,
    pub frames: Arc<FrameQueue>,
    pub audio: Option<Arc<SessionAudio>>,
    pub shared: Arc<DecoderShared>,
    pub controls: Receiver<Control>,
    /// The session's timeline position now.
    pub clock: Clock,
}

pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub fn spawn(params: DecoderParams) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new().name("media-decoder".into()).spawn(move || {
        ACTIVE_DECODERS.fetch_add(1, Ordering::AcqRel);
        let shared = params.shared.clone();
        if let Err(e) = Worker::open(params).and_then(Worker::run) {
            shared.fail(format!("{e:#}"));
        }
        ACTIVE_DECODERS.fetch_sub(1, Ordering::AcqRel);
    })
}

struct VideoStream {
    index: usize,
    decoder: ffmpeg::decoder::Video,
    time_base: f64,
    /// Converter and the (input format, size, output format) it was built for.
    scaler: Option<(ffmpeg::software::scaling::Context, (ffmpeg::format::Pixel, u32, u32, ffmpeg::format::Pixel))>,
    hw: Option<HwDecoder>,
    /// Whether the first frame's decoder (hardware or software) was logged.
    reported: bool,
}

/// Software decoders use up to this many threads each.
fn decode_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get()).min(4)
}

/// Opens stream `index`'s decoder, with the chosen hardware accelerator
/// when `hardware` and one applies; falls back to software if that fails.
fn open_video(input: &ffmpeg::format::context::Input, index: usize, hardware: bool) -> anyhow::Result<(ffmpeg::decoder::Video, Option<HwDecoder>)> {
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

fn has_alpha(format: ffmpeg::format::Pixel) -> bool {
    let descriptor = unsafe { ffmpeg::ffi::av_pix_fmt_desc_get(format.into()) };
    !descriptor.is_null() && unsafe { (*descriptor).flags } & ffmpeg::ffi::AV_PIX_FMT_FLAG_ALPHA as u64 != 0
}

/// The matrix a frame declares, else the usual one for its size.
fn yuv_matrix(frame: &ffmpeg::frame::Video) -> YuvMatrix {
    use ffmpeg::color::Space;
    match frame.color_space() {
        Space::BT709 => YuvMatrix::Bt709,
        Space::BT470BG | Space::SMPTE170M | Space::FCC | Space::SMPTE240M => YuvMatrix::Bt601,
        Space::BT2020NCL | Space::BT2020CL => YuvMatrix::Bt2020,
        _ if frame.height() >= 720 => YuvMatrix::Bt709,
        _ => YuvMatrix::Bt601,
    }
}

/// A frame's pixels ready for upload, before it gets an id and a time.
struct Packed {
    width: u32,
    height: u32,
    format: FrameFormat,
    matrix: YuvMatrix,
    full_range: bool,
    planes: [Plane; 3],
    data: Arc<[u8]>,
}

/// Copies the first `count` planes of `frame` into one shared buffer.
fn copy_planes(frame: &ffmpeg::frame::Video, count: usize) -> ([Plane; 3], Arc<[u8]>) {
    let mut planes = [Plane::default(); 3];
    let mut total = 0;
    for (i, plane) in planes.iter_mut().enumerate().take(count) {
        *plane = Plane { offset: total, stride: frame.stride(i) };
        total += frame.data(i).len();
    }
    let mut data = Arc::<[u8]>::new_uninit_slice(total);
    let buffer = Arc::get_mut(&mut data).expect("new buffer is unique");
    for (i, plane) in planes.iter().enumerate().take(count) {
        let source = frame.data(i);
        // SAFETY: the planes were laid out back to back within `total` bytes.
        unsafe { std::ptr::copy_nonoverlapping(source.as_ptr(), buffer.as_mut_ptr().add(plane.offset) as *mut u8, source.len()) };
    }
    // SAFETY: every byte was written above.
    (planes, unsafe { data.assume_init() })
}

struct AudioStream {
    index: usize,
    decoder: ffmpeg::decoder::Audio,
    time_base: f64,
    out: Arc<SessionAudio>,
    /// Converter and the input (format, rate, channels) it was built for.
    resampler: Option<(ffmpeg::software::resampling::Context, (ffmpeg::format::Sample, u32, u16))>,
}

struct Worker {
    input: ffmpeg::format::context::Input,
    video: Option<VideoStream>,
    audio: Option<AudioStream>,
    /// Container start time in µs, subtracted from every timestamp.
    origin_us: i64,
    clip: Clip,
    rate: f64,
    epoch: u64,
    frames: Arc<FrameQueue>,
    shared: Arc<DecoderShared>,
    controls: Receiver<Control>,
    clock: Clock,
    /// Media positions before these are decoded but not presented.
    video_discard_before_us: i64,
    audio_discard_before_us: i64,
    /// End of the audio sent to the ring in this pass, media µs.
    audio_end_us: i64,
    /// Added to media time to get timeline time; grows by one loop per wrap.
    timeline_offset_us: i64,
    /// End of the furthest content decoded in this pass, media µs.
    content_end_us: i64,
    clip_end_reached: bool,
    /// Previous presented video pts and spacing, to find where the last frame ends.
    last_video_us: Option<i64>,
    video_interval_us: i64,
    /// Video packets read but not yet decoded, and decoded frames not yet queued.
    video_stash: VecDeque<ffmpeg::Packet>,
    stash_bytes: usize,
    pending_frames: VecDeque<VideoFrame>,
    /// The clip's first frames (media µs, pixels) for the next loop wrap.
    head: Vec<(i64, VideoFrame)>,
    /// `head` holds the clip's start; while capturing it is still filling.
    head_complete: bool,
    capturing_head: bool,
    /// Loop-head frames still in `pending_frames`; decoding continues
    /// behind them instead of waiting for the queue.
    head_pending: usize,
    /// Set when the hardware decoder fails; the next step continues in software.
    hw_failure: Option<String>,
    /// Time spent in the video decoder since the last decoded frame.
    decode_time: Duration,
}

enum Flow {
    Continue,
    Close,
}

impl Worker {
    fn open(params: DecoderParams) -> anyhow::Result<Self> {
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

    fn run(mut self) -> anyhow::Result<()> {
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
                // Idle at the end of the clip, presenting what is left.
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
                // A control is on its way; wait for it rather than decode stale data.
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            self.drain_video()?;
            if self.clip_end_reached {
                self.end_of_clip()?;
                continue;
            }
            if self.stash_bytes > STASH_LIMIT_BYTES {
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

    fn apply(&mut self, control: Control) -> anyhow::Result<Flow> {
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
                // Turning looping on at the end resumes from the clip start.
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

    /// Flushes everything queued and decodes from `position_us` again.
    fn restart(&mut self, epoch: u64, position_us: i64, rate: f64) -> anyhow::Result<()> {
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

    fn seek_media(&mut self, position_us: i64) -> anyhow::Result<()> {
        let target = position_us + self.origin_us;
        // Land on the keyframe at or before the target, then decode forward.
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

    fn decode_packet(&mut self, packet: ffmpeg::Packet) -> anyhow::Result<()> {
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

    /// Moves decoded frames into the queue and decodes stashed video while it
    /// has room. Never waits.
    fn drain_video(&mut self) -> anyhow::Result<()> {
        loop {
            while let Some(frame) = self.pending_frames.pop_front() {
                if let Err(frame) = self.frames.try_push(frame, (self.clock)()) {
                    self.pending_frames.push_front(frame);
                    break;
                }
                self.head_pending = self.head_pending.saturating_sub(1);
            }
            // Only loop-head frames waiting: keep decoding what follows them.
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

    fn receive_video(&mut self) -> anyhow::Result<()> {
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

    /// Replaces a failed hardware decoder with a software one and continues
    /// after the last frame it produced. Audio already queued is not
    /// decoded again, so playback continues without a jump.
    fn fall_back_to_software(&mut self, reason: &str) -> anyhow::Result<()> {
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

    fn forget_head(&mut self) {
        self.head.clear();
        self.head_complete = false;
        self.capturing_head = false;
    }

    fn receive_audio(&mut self) -> anyhow::Result<()> {
        let mut decoded = ffmpeg::frame::Audio::empty();
        while !self.shared.interrupted() && self.audio.as_mut().unwrap().decoder.receive_frame(&mut decoded).is_ok() {
            self.handle_audio(&decoded)?;
        }
        Ok(())
    }

    fn media_us(&self, timestamp: Option<i64>, time_base: f64) -> Option<i64> {
        timestamp.map(|ts| (ts as f64 * time_base * 1e6) as i64 - self.origin_us)
    }

    fn handle_video(&mut self, decoded: &ffmpeg::frame::Video) -> anyhow::Result<()> {
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
        // A frame lasts until the next one, so the content runs one interval past it.
        self.content_end_us = self.content_end_us.max(media_us + self.video_interval_us);
        self.pending_frames.push_back(frame);
        Ok(())
    }

    fn handle_audio(&mut self, decoded: &ffmpeg::frame::Audio) -> anyhow::Result<()> {
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
        // Resampling to `device_rate / rate` and playing at `device_rate` runs
        // `rate` times faster; the pitch follows the speed.
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
        // Packed f32: one plane of interleaved samples, aligned by FFmpeg's allocator.
        let data = converted.data(0);
        let all: &[f32] = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, frames * channels) };

        // Output frame k carries media time start + k / output_rate.
        let us_per_frame = 1e6 / output_rate as f64;
        let skip = (((self.audio_discard_before_us - start_us) as f64 / us_per_frame).ceil().max(0.0) as usize).min(frames);
        let keep = match self.clip.end_us {
            Some(end) => {
                let until = (((end - start_us) as f64 / us_per_frame).floor().max(0.0) as usize).min(frames);
                // With video, the video stream decides where the clip ends.
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
                // Keep video moving while audio waits for the device.
                self.drain_video()?;
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        Ok(())
    }

    /// The clip's end (or the file's): drain decoders, then loop or finish.
    fn end_of_clip(&mut self) -> anyhow::Result<()> {
        if let Some(audio) = &mut self.audio {
            let _ = audio.decoder.send_eof();
        }
        if self.audio.is_some() {
            self.receive_audio()?;
            if !self.clip.looping {
                // No more audio: the session clock continues on wall time.
                self.audio.as_ref().unwrap().out.drained.store(true, Ordering::Release);
            }
        }
        // Decode every stashed packet before flushing the video decoder.
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

    /// Continues at the clip start on a timeline one loop later. Audio is not
    /// flushed, so it plays straight through the wrap.
    fn wrap(&mut self) -> anyhow::Result<()> {
        let start = self.clip.start_us;
        let content_end = self.content_end_us.max(start + 1);
        let loop_len = match self.clip.end_us {
            Some(end) => end - start,
            None => content_end - start,
        }
        .max(1);
        let next_offset = if self.shared.finished.load(Ordering::Acquire) {
            // Resuming from a finished clip: continue after its end.
            self.shared.end_timeline_us.load(Ordering::Acquire) - start
        } else {
            self.timeline_offset_us + loop_len
        };
        self.seek_media(start)?;
        self.timeline_offset_us = next_offset;
        if self.capturing_head {
            // The whole clip fit in the head.
            self.capturing_head = false;
            self.head_complete = !self.head.is_empty();
        }
        if self.head_complete {
            // Show the prepared start at once; the decoder skips past it.
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

/// Packs `source` for upload: 8-bit 4:2:0 YUV as decoded when it fits
/// [`MAX_YUV_FRAME`], sources with transparency as RGBA, anything else
/// converted (and if needed scaled down) to 4:2:0 YUV.
fn pack(video: &mut VideoStream, source: &ffmpeg::frame::Video) -> anyhow::Result<Packed> {
    use ffmpeg::format::Pixel;
    let (width, height) = (source.width(), source.height());
    let fits = width <= MAX_YUV_FRAME.0 && height <= MAX_YUV_FRAME.1;
    let matrix = yuv_matrix(source);
    let declared_full = source.color_range() == ffmpeg::color::Range::JPEG;
    let direct = match source.format() {
        Pixel::YUV420P if fits => Some((FrameFormat::I420, declared_full)),
        Pixel::YUVJ420P if fits => Some((FrameFormat::I420, true)),
        Pixel::NV12 if fits => Some((FrameFormat::Nv12, declared_full)),
        _ => None,
    };
    if let Some((format, full_range)) = direct {
        let (planes, data) = copy_planes(source, format.planes());
        return Ok(Packed { width, height, format, matrix, full_range, planes, data });
    }
    let alpha = has_alpha(source.format());
    let (target, format, (out_width, out_height)) = if alpha {
        (Pixel::RGBA, FrameFormat::Rgba, fit_within(width, height, MAX_FRAME))
    } else if fits {
        (Pixel::YUV420P, FrameFormat::I420, (width, height))
    } else {
        (Pixel::YUV420P, FrameFormat::I420, fit_within(width, height, MAX_FRAME))
    };
    let key = (source.format(), width, height, target);
    if video.scaler.as_ref().is_none_or(|(_, k)| *k != key) {
        let scaler = ffmpeg::software::scaling::Context::get(
            source.format(),
            width,
            height,
            target,
            out_width,
            out_height,
            ffmpeg::software::scaling::Flags::BILINEAR,
        )?;
        video.scaler = Some((scaler, key));
    }
    let mut converted = ffmpeg::frame::Video::empty();
    video.scaler.as_mut().unwrap().0.run(source, &mut converted)?;
    let (planes, data) = copy_planes(&converted, format.planes());
    // The scaler maps full-range "J" formats to limited range; other inputs keep theirs.
    let jpeg_format = matches!(source.format(), Pixel::YUVJ420P | Pixel::YUVJ422P | Pixel::YUVJ444P | Pixel::YUVJ440P | Pixel::YUVJ411P);
    let full_range = alpha || (declared_full && !jpeg_format);
    Ok(Packed { width: out_width, height: out_height, format, matrix, full_range, planes, data })
}

/// Largest size within `max` with the same aspect ratio, never upscaled, even.
pub fn fit_within(width: u32, height: u32, max: (u32, u32)) -> (u32, u32) {
    let scale = (max.0 as f64 / width as f64).min(max.1 as f64 / height as f64).min(1.0);
    let even = |v: f64| ((v.round() as u32) & !1).max(2);
    (even(width as f64 * scale), even(height as f64 * scale))
}

#[cfg(test)]
mod tests {
    use super::fit_within;

    #[test]
    fn frames_fit_the_output_box_without_upscaling() {
        assert_eq!(fit_within(3840, 2160, (1920, 1080)), (1920, 1080));
        assert_eq!(fit_within(1280, 720, (1920, 1080)), (1280, 720));
        assert_eq!(fit_within(1080, 1920, (1920, 1080)), (608, 1080));
        assert_eq!(fit_within(4096, 1716, (1920, 1080)), (1920, 804));
    }
}
