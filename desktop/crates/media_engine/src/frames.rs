//! Decoded video frames and the bounded queue between a decoder and the
//! renderer. Frames are chosen by presentation time, not by arrival.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// How a frame's pixels are laid out in [`VideoFrame::data`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFormat {
    /// One plane of 8-bit RGBA, for sources with transparency.
    Rgba,
    /// 8-bit Y, U and V planes, chroma at half width and height.
    I420,
    /// 8-bit Y plane and one interleaved UV plane (hardware decoders' output).
    Nv12,
}

impl FrameFormat {
    pub fn planes(self) -> usize {
        match self {
            FrameFormat::Rgba => 1,
            FrameFormat::I420 => 3,
            FrameFormat::Nv12 => 2,
        }
    }
}

/// The YUV to RGB matrix a frame was encoded with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YuvMatrix {
    Bt601,
    Bt709,
    Bt2020,
}

/// Where one plane starts in [`VideoFrame::data`] and its bytes per row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Plane {
    pub offset: usize,
    pub stride: usize,
}

/// One decoded frame, RGBA or YUV. YUV frames keep their decoded size (up
/// to 4K); the renderer converts and scales them on the GPU. `pts_us` is on
/// the session timeline, which keeps increasing across loops.
pub struct VideoFrame {
    /// Unique per decoded frame, so the renderer uploads each frame once.
    pub id: u64,
    pub epoch: u64,
    pub pts_us: i64,
    pub width: u32,
    pub height: u32,
    pub format: FrameFormat,
    pub matrix: YuvMatrix,
    /// Full (0–255) rather than limited (16–235) YUV range.
    pub full_range: bool,
    /// The first `format.planes()` entries are used.
    pub planes: [Plane; 3],
    /// Shared so the loop point can show the same pixels again.
    pub data: Arc<[u8]>,
}

impl VideoFrame {
    pub fn next_id() -> u64 {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    /// Plane `index`'s bytes (from its start to the end of the data) and its
    /// bytes per row.
    pub fn plane(&self, index: usize) -> (&[u8], usize) {
        let plane = self.planes[index];
        (&self.data[plane.offset..], plane.stride)
    }

    /// The same pixels at another time, as a new frame.
    pub fn retimed(&self, epoch: u64, pts_us: i64) -> VideoFrame {
        VideoFrame {
            id: Self::next_id(),
            epoch,
            pts_us,
            width: self.width,
            height: self.height,
            format: self.format,
            matrix: self.matrix,
            full_range: self.full_range,
            planes: self.planes,
            data: self.data.clone(),
        }
    }
}

impl std::fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoFrame")
            .field("id", &self.id)
            .field("epoch", &self.epoch)
            .field("pts_us", &self.pts_us)
            .field("size", &(self.width, self.height))
            .field("format", &self.format)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub queued: u64,
    pub presented: u64,
    /// Frames whose time passed before they were ever selected.
    pub dropped_late: u64,
}

struct State {
    frames: VecDeque<Arc<VideoFrame>>,
    current: Option<Arc<VideoFrame>>,
    epoch: u64,
    stats: FrameStats,
}

pub struct FrameQueue {
    state: Mutex<State>,
    space: Condvar,
    capacity: usize,
}

impl FrameQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(State { frames: VecDeque::new(), current: None, epoch: 0, stats: FrameStats::default() }),
            space: Condvar::new(),
            capacity: capacity.max(1),
        }
    }

    #[cfg(test)]
    /// Producer: waits for space. Returns `false` without queuing when `abort`
    /// becomes true (a seek or close arrived) or the frame belongs to an
    /// earlier epoch.
    pub fn push(&self, frame: VideoFrame, abort: impl Fn() -> bool) -> bool {
        let mut state = self.state.lock().expect("frame queue poisoned");
        loop {
            if frame.epoch != state.epoch || abort() {
                return false;
            }
            if state.frames.len() < self.capacity {
                state.frames.push_back(Arc::new(frame));
                state.stats.queued += 1;
                return true;
            }
            state = self.space.wait_timeout(state, Duration::from_millis(20)).expect("frame queue poisoned").0;
        }
    }

    /// Producer: queues without waiting. When full, first drops frames that
    /// `select(position_us)` would pass over anyway, so a renderer that
    /// stops polling cannot hold decoding (and the audio behind it) back.
    /// Hands the frame back if still full; silently drops frames from an
    /// earlier epoch.
    pub fn try_push(&self, frame: VideoFrame, position_us: i64) -> Result<(), VideoFrame> {
        let mut state = self.state.lock().expect("frame queue poisoned");
        if frame.epoch != state.epoch {
            return Ok(());
        }
        if state.frames.len() >= self.capacity {
            // Keep the latest due frame queued for the renderer to present.
            while state.frames.get(1).is_some_and(|next| next.pts_us <= position_us) {
                state.frames.pop_front();
                state.stats.dropped_late += 1;
            }
        }
        if state.frames.len() >= self.capacity {
            return Err(frame);
        }
        state.frames.push_back(Arc::new(frame));
        state.stats.queued += 1;
        Ok(())
    }

    /// Producer: waits up to `timeout` for the renderer to free a slot.
    pub fn wait_for_space(&self, timeout: Duration) {
        let state = self.state.lock().expect("frame queue poisoned");
        if state.frames.len() >= self.capacity {
            let _ = self.space.wait_timeout(state, timeout).expect("frame queue poisoned");
        }
    }

    /// Renderer: the latest frame due at `position_us`. Frames passed over are
    /// counted as late drops. The first frame of a new epoch is shown at once,
    /// so a paused seek displays its target frame.
    pub fn select(&self, position_us: i64) -> Option<Arc<VideoFrame>> {
        let mut state = self.state.lock().expect("frame queue poisoned");
        let epoch = state.epoch;
        let stale_current = state.current.as_ref().is_none_or(|c| c.epoch != epoch);
        let mut advanced = 0;
        while let Some(front) = state.frames.front() {
            let due = front.pts_us <= position_us || (stale_current && advanced == 0);
            if !due {
                break;
            }
            state.current = state.frames.pop_front();
            advanced += 1;
        }
        if advanced > 0 {
            state.stats.presented += 1;
            state.stats.dropped_late += advanced - 1;
            self.space.notify_all();
        }
        state.current.clone()
    }

    /// Starts a new epoch: discards queued frames but keeps showing the
    /// current one until a frame of the new epoch arrives.
    pub fn flush(&self, epoch: u64) {
        let mut state = self.state.lock().expect("frame queue poisoned");
        state.frames.clear();
        state.epoch = epoch;
        self.space.notify_all();
    }

    /// Releases everything, including the displayed frame.
    pub fn clear(&self) {
        let mut state = self.state.lock().expect("frame queue poisoned");
        state.frames.clear();
        state.current = None;
        self.space.notify_all();
    }

    pub fn wake(&self) {
        self.space.notify_all();
    }

    /// Presentation time of the last queued frame, or the current one.
    pub fn last_pts(&self) -> Option<i64> {
        let state = self.state.lock().expect("frame queue poisoned");
        state.frames.back().or(state.current.as_ref()).map(|f| f.pts_us)
    }

    pub fn stats(&self) -> FrameStats {
        self.state.lock().expect("frame queue poisoned").stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(epoch: u64, pts_us: i64) -> VideoFrame {
        VideoFrame {
            id: VideoFrame::next_id(),
            epoch,
            pts_us,
            width: 2,
            height: 2,
            format: FrameFormat::Rgba,
            matrix: YuvMatrix::Bt709,
            full_range: true,
            planes: [Plane { offset: 0, stride: 8 }, Plane::default(), Plane::default()],
            data: vec![0; 16].into(),
        }
    }

    fn pts(frame: Option<Arc<VideoFrame>>) -> Option<i64> {
        frame.map(|f| f.pts_us)
    }

    #[test]
    fn selects_the_latest_due_frame_and_counts_skipped_frames_as_late() {
        let queue = FrameQueue::new(8);
        for t in [0, 40_000, 80_000, 120_000] {
            assert!(queue.push(frame(0, t), || false));
        }
        assert_eq!(pts(queue.select(0)), Some(0));
        assert_eq!(pts(queue.select(39_999)), Some(0), "an early frame waits");
        assert_eq!(pts(queue.select(125_000)), Some(120_000));
        let stats = queue.stats();
        assert_eq!((stats.presented, stats.dropped_late), (2, 2));
    }

    #[test]
    fn a_seek_keeps_the_old_frame_until_the_new_epoch_delivers_one() {
        let queue = FrameQueue::new(8);
        queue.push(frame(0, 0), || false);
        assert_eq!(pts(queue.select(0)), Some(0));
        queue.flush(1);
        assert!(!queue.push(frame(0, 40_000), || false), "frames from before the seek are refused");
        assert_eq!(pts(queue.select(60_000_000)), Some(0), "no black gap while the seek decodes");
        // The target frame may be slightly after the paused clock; it still shows.
        queue.push(frame(1, 60_016_000), || false);
        assert_eq!(pts(queue.select(60_000_000)), Some(60_016_000));
    }

    #[test]
    fn a_full_queue_drops_superseded_frames_when_the_renderer_stops_polling() {
        let queue = FrameQueue::new(3);
        for t in [0, 40_000, 80_000] {
            queue.try_push(frame(0, t), 0).unwrap();
        }
        assert!(queue.try_push(frame(0, 120_000), 0).is_err(), "nothing is late yet");
        // The clock reached 90 ms without a select: 0 and 40 ms will never show.
        queue.try_push(frame(0, 120_000), 90_000).unwrap();
        assert_eq!(pts(queue.select(90_000)), Some(80_000), "the due frame is kept");
        let stats = queue.stats();
        assert_eq!((stats.presented, stats.dropped_late), (1, 2));
    }

    #[test]
    fn a_full_queue_blocks_the_producer_until_aborted() {
        let queue = Arc::new(FrameQueue::new(1));
        queue.push(frame(0, 0), || false);
        let aborted = std::sync::atomic::AtomicBool::new(false);
        let producer = {
            let queue = queue.clone();
            std::thread::spawn(move || queue.push(frame(0, 40_000), || false))
        };
        std::thread::sleep(Duration::from_millis(50));
        assert!(!producer.is_finished(), "waits for space");
        queue.select(0);
        assert!(producer.join().unwrap(), "space freed by selection");
        assert!(!queue.push(frame(0, 80_000), || {
            aborted.store(true, Ordering::Relaxed);
            true
        }));
    }
}
