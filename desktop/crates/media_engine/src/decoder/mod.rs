//! One worker thread per session: demux, decode, convert, and feed the frame
//! queue and audio ring. Restarts (seek, rate change) flush both outputs so no
//! obsolete frame or sample is ever presented.

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
