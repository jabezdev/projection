use std::sync::{Arc, Mutex};
use std::time::Instant;

use crossbeam::channel::Receiver;

use crate::main_scene::MediaStage;
use crate::scene::ConfidenceScene;
use crate::text::TextLayout;

mod layout;
mod metrics;
mod palette;
mod renderer;
mod smooth;
mod stage;

use layout::*;
use metrics::*;
use palette::*;
use smooth::*;

pub struct ConfidenceRenderer {
    scene: ConfidenceScene,
    revision: u64,
    layout: Option<Arc<Layout>>,
    active: Option<(usize, usize)>,
    building: Option<(LayoutKey, Receiver<Layout>)>,
    cache: Arc<Mutex<TextCache>>,
    video: MediaStage,
    scroll: Smooth,
    tape: Smooth,
    focused: Option<(u64, (u32, u32))>,
    clock: Option<(String, u32, Arc<TextLayout>)>,
    timer: Option<(String, u32, Arc<TextLayout>)>,
    message: Option<(String, u32, Arc<TextLayout>)>,
}

impl Default for ConfidenceRenderer {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            scene: ConfidenceScene { size: 4.0, transition_ms: 250.0, ..Default::default() },
            revision: 0,
            layout: None,
            active: None,
            building: None,
            cache: Arc::default(),
            video: MediaStage::default(),
            scroll: Smooth::at(0.0, now),
            tape: Smooth::at(0.0, now),
            focused: None,
            clock: None,
            timer: None,
            message: None,
        }
    }
}

#[cfg(test)]
mod tests;
