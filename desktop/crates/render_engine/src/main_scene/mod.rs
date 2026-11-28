mod fit;
mod geometry;
mod lyrics;
mod media_stage;
mod renderer;

pub use fit::{split_to_fit, SlideText};
pub use geometry::LyricGeometry;
pub(crate) use media_stage::MediaStage;
pub use renderer::MainRenderer;
use lyrics::*;
use media_stage::*;

#[cfg(test)]
mod tests;
