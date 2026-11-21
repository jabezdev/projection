use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use render_engine::{FramePixels, PixelFrame};

pub(super) const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(33);

pub struct CaptureFrame {
    pub(super) id: u64,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) stride: usize,
    pub(super) data: Vec<u8>,
}

impl PixelFrame for CaptureFrame {
    fn id(&self) -> u64 {
        self.id
    }
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    fn pixels(&self) -> FramePixels<'_> {
        FramePixels::Bgra { data: &self.data, stride: self.stride }
    }
}

pub(super) fn next_frame_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1 << 62);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[derive(Default)]
pub(super) struct Slot {
    pub(super) latest: Option<Arc<CaptureFrame>>,
    pub(super) shown: Option<(Arc<CaptureFrame>, Instant)>,
    pub(super) lost: Option<String>,
}

#[derive(Clone, Default)]
pub struct FrameSlot(Arc<Mutex<Slot>>);

impl FrameSlot {
    pub fn publish(&self, width: u32, height: u32, stride: usize, data: Vec<u8>) {
        if width == 0 || height == 0 || data.len() < stride * (height as usize - 1) + width as usize * 4 {
            return;
        }
        let frame = Arc::new(CaptureFrame { id: next_frame_id(), width, height, stride, data });
        self.0.lock().expect("capture slot poisoned").latest = Some(frame);
    }

    pub fn lose(&self, reason: String) {
        let mut slot = self.0.lock().expect("capture slot poisoned");
        if slot.lost.is_some() {
            return;
        }
        let (width, height) = slot.latest.as_ref().map_or((1280, 720), |f| (f.width, f.height));
        log::warn!("Capture lost: {reason}");
        slot.latest = Some(Arc::new(CaptureFrame { id: next_frame_id(), width, height, stride: width as usize * 4, data: vec![0; width as usize * height as usize * 4] }));
        slot.shown = None;
        slot.lost = Some(reason);
    }

    pub fn lost(&self) -> Option<String> {
        self.0.lock().expect("capture slot poisoned").lost.clone()
    }

    pub(super) fn frame(&self, now: Instant) -> Option<Arc<CaptureFrame>> {
        let mut slot = self.0.lock().expect("capture slot poisoned");
        let latest = slot.latest.clone()?;
        match &slot.shown {
            Some((shown, at)) if shown.id != latest.id && now.duration_since(*at) < MIN_FRAME_INTERVAL => Some(shown.clone()),
            Some((shown, _)) if shown.id == latest.id => Some(latest),
            _ => {
                slot.shown = Some((latest.clone(), now));
                Some(latest)
            }
        }
    }
}
