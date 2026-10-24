//! Test aid (`MAGI_TEST_CAPTURE=1`): a capture method that shows a moving
//! pattern, so routing, fit and the lost-source fallback can be checked on
//! the real outputs without a screen to share. A source whose id ends in
//! `:lose` stops after two seconds, like a closed window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use protocol::{CaptureKind, CaptureSource};

use super::{Backend, CaptureStream, FrameSlot};

pub struct TestPattern;

struct Running(Arc<AtomicBool>);

impl CaptureStream for Running {}

impl Drop for Running {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Backend for TestPattern {
    fn name(&self) -> &'static str {
        "test"
    }

    fn label(&self) -> &'static str {
        "Test pattern"
    }

    fn sources(&self) -> anyhow::Result<Vec<CaptureSource>> {
        Ok(vec![
            CaptureSource { kind: CaptureKind::Screen, id: "test:pattern".into(), name: "Test pattern".into() },
            CaptureSource { kind: CaptureKind::Window, id: "test:pattern:lose".into(), name: "Test pattern that closes".into() },
        ])
    }

    fn start(&self, source: &CaptureSource, slot: FrameSlot) -> anyhow::Result<Box<dyn CaptureStream>> {
        anyhow::ensure!(source.id.starts_with("test:"), "Not a test source");
        let running = Arc::new(AtomicBool::new(true));
        let (keep, lose) = (running.clone(), source.id.ends_with(":lose"));
        std::thread::Builder::new().name("capture-test".into()).spawn(move || {
            let started = Instant::now();
            let (width, height) = (1280u32, 720u32);
            while keep.load(Ordering::Acquire) {
                if lose && started.elapsed() > Duration::from_secs(2) {
                    return slot.lose("The test window closed".into());
                }
                // Orange with a white bar sweeping across (BGRA).
                let bar = (started.elapsed().as_millis() as u32 / 8) % width;
                let mut data = vec![0u8; (width * height * 4) as usize];
                for (i, pixel) in data.chunks_exact_mut(4).enumerate() {
                    let x = i as u32 % width;
                    pixel.copy_from_slice(if x.abs_diff(bar) < 20 { &[255, 255, 255, 255] } else { &[0, 128, 255, 255] });
                }
                slot.publish(width, height, width as usize * 4, data);
                std::thread::sleep(Duration::from_millis(33));
            }
        })?;
        Ok(Box::new(Running(running)))
    }
}
