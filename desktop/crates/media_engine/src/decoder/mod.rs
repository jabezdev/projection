use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam::channel::Receiver;
use ffmpeg_next as ffmpeg;

use crate::audio::SessionAudio;
use crate::frames::{FrameFormat, FrameQueue, Plane, VideoFrame, YuvMatrix};
use crate::hwaccel::HwDecoder;

mod audio;
mod clip_end;
mod packing;
mod video;
mod worker;

pub use packing::fit_within;
use packing::pack;
use worker::open_video;

const STASH_LIMIT_BYTES: usize = 64 * 1024 * 1024;

pub const MAX_FRAME: (u32, u32) = (1920, 1080);

pub const MAX_YUV_FRAME: (u32, u32) = (4096, 2304);

const LOOP_HEAD_FRAMES: usize = 6;
const LOOP_HEAD_US: i64 = 250_000;

static ACTIVE_DECODERS: AtomicUsize = AtomicUsize::new(0);

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
    Restart { epoch: u64, position_us: i64, rate: f64 },
    SetClip(Clip),
    Close,
}

#[derive(Default)]
pub struct DecoderShared {
    pub has_video: AtomicBool,
    pub has_audio: AtomicBool,
    pub opened: AtomicBool,
    pub duration_us: AtomicI64,
    pub applied_epoch: AtomicU64,
    pub interrupts: AtomicUsize,
    pub finished: AtomicBool,
    pub end_timeline_us: AtomicI64,
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
    scaler: Option<(ffmpeg::software::scaling::Context, (ffmpeg::format::Pixel, u32, u32, ffmpeg::format::Pixel))>,
    hw: Option<HwDecoder>,
    reported: bool,
}

struct Packed {
    width: u32,
    height: u32,
    format: FrameFormat,
    matrix: YuvMatrix,
    full_range: bool,
    planes: [Plane; 3],
    data: Arc<[u8]>,
}

struct AudioStream {
    index: usize,
    decoder: ffmpeg::decoder::Audio,
    time_base: f64,
    out: Arc<SessionAudio>,
    resampler: Option<(ffmpeg::software::resampling::Context, (ffmpeg::format::Sample, u32, u16))>,
}

struct Worker {
    input: ffmpeg::format::context::Input,
    video: Option<VideoStream>,
    audio: Option<AudioStream>,
    origin_us: i64,
    clip: Clip,
    rate: f64,
    epoch: u64,
    frames: Arc<FrameQueue>,
    shared: Arc<DecoderShared>,
    controls: Receiver<Control>,
    clock: Clock,
    video_discard_before_us: i64,
    audio_discard_before_us: i64,
    audio_end_us: i64,
    timeline_offset_us: i64,
    content_end_us: i64,
    clip_end_reached: bool,
    last_video_us: Option<i64>,
    video_interval_us: i64,
    video_stash: VecDeque<ffmpeg::Packet>,
    stash_bytes: usize,
    pending_frames: VecDeque<VideoFrame>,
    head: Vec<(i64, VideoFrame)>,
    head_complete: bool,
    capturing_head: bool,
    head_pending: usize,
    hw_failure: Option<String>,
    decode_time: Duration,
}

enum Flow {
    Continue,
    Close,
}
