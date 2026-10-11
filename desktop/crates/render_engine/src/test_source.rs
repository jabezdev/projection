use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::{PixelFrame, VideoFrames, VideoKey};

/// Synthetic 30 fps frames for checking outputs without media. A sweeping bar
/// and binary frame counter make shared-frame identity visible: every output
/// must show the same bar position and counter.
pub struct TestPatternSource {
    start: Instant,
    size: (u32, u32),
    latest: Mutex<Option<Arc<PatternFrame>>>,
}

struct PatternFrame {
    index: u64,
    size: (u32, u32),
    pixels: Vec<u8>,
}

impl PixelFrame for PatternFrame {
    fn id(&self) -> u64 {
        self.index
    }
    fn size(&self) -> (u32, u32) {
        self.size
    }
    fn stride(&self) -> usize {
        self.size.0 as usize * 4
    }
    fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

impl TestPatternSource {
    pub fn new(width: u32, height: u32) -> Self {
        Self { start: Instant::now(), size: (width, height), latest: Mutex::new(None) }
    }
}

impl VideoFrames for TestPatternSource {
    /// The same pattern for every video.
    fn frame(&self, _key: &VideoKey) -> Option<Arc<dyn PixelFrame>> {
        let index = (self.start.elapsed().as_secs_f64() * 30.0) as u64 + 1;
        let mut latest = self.latest.lock().ok()?;
        if latest.as_ref().is_none_or(|f| f.index != index) {
            let (width, height) = self.size;
            let mut pixels = vec![0u8; width as usize * height as usize * 4];
            draw_pattern(&mut pixels, width, height, index);
            *latest = Some(Arc::new(PatternFrame { index, size: self.size, pixels }));
        }
        latest.clone().map(|f| f as Arc<dyn PixelFrame>)
    }
}

fn draw_pattern(pixels: &mut [u8], width: u32, height: u32, index: u64) {
    let (w, h) = (width as usize, height as usize);
    let bar_x = (index % 120) as usize * w / 120;
    let bar_width = (w / 60).max(2);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let mut rgb = [(x * 255 / w) as u8, (y * 255 / h) as u8, 96];
            if x >= bar_x && x < bar_x + bar_width {
                rgb = [255, 255, 255];
            }
            pixels[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    // 16-bit frame counter: lit blocks along the top edge.
    let block = w / 20;
    for bit in 0..16 {
        let lit = (index >> (15 - bit)) & 1 == 1;
        let color = if lit { [255, 220, 0, 255] } else { [30, 30, 30, 255] };
        for y in block / 2..block * 3 / 2 {
            for x in (2 + bit) * block + 2..(3 + bit) * block - 2 {
                let i = (y * w + x) * 4;
                pixels[i..i + 4].copy_from_slice(&color);
            }
        }
    }
}
