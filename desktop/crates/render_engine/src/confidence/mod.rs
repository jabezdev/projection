//! The confidence output: clock, arrangement tape and the scrolling
//! teleprompter, or the foreground video when its confidence option is on.

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

use layout::*;
use metrics::*;
use palette::*;
use smooth::*;

pub struct ConfidenceRenderer {
    scene: ConfidenceScene,
    revision: u64,
    /// The layout on screen; replaced when a rebuild finishes.
    layout: Option<Arc<Layout>>,
    /// The active row within `layout`, which may lag the scene while a
    /// rebuild for new groups is in flight.
    active: Option<(usize, usize)>,
    building: Option<(LayoutKey, Receiver<Layout>)>,
    cache: Arc<Mutex<TextCache>>,
    video: MediaStage,
    scroll: Smooth,
    tape: Smooth,
    /// Scroll targets were computed for this revision and size.
    focused: Option<(u64, (u32, u32))>,
    clock: Option<(String, u32, Arc<TextLayout>)>,
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
        }
    }
}

#[cfg(test)]
mod tests;
