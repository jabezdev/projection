//! The audience output: background media, Black/Logo overrides and the
//! lyric box. Margins are inset inside the output, so they always hold.



mod geometry;
mod lyrics;
mod media_stage;
mod renderer;

pub use geometry::LyricGeometry;
pub(crate) use media_stage::MediaStage;
pub use renderer::MainRenderer;
use lyrics::*;
use media_stage::*;

#[cfg(test)]
mod tests;
