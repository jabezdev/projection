//! Text shaping, layout and glyph rasterization.
//!
//! cosmic-text shapes (with font fallback and bidi); layout here adds CSS
//! letter spacing before wrapping, which cosmic-text's `Buffer` cannot do.
//! Glyphs are rasterized with swash into a per-output mask atlas.
//!
//! Icons: `:name:` in text (a Lucide icon name, such as `:heart:` or
//! `:book-open:`) is drawn as that icon from the bundled Lucide font, in the
//! text's color and effects. Unknown names stay as written.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use cosmic_text::FontSystem;

mod atlas;
mod fonts;
mod icons;
mod kerning;
mod layout;
mod sdf;

pub use atlas::*;
pub use icons::expand_icons;
pub use layout::*;
pub use sdf::*;
use fonts::*;
use icons::*;
use kerning::*;

/// The process-wide font database. Loading system fonts is slow, so it starts
/// on a background thread when the engine starts.
pub struct Fonts {
    system: OnceLock<Mutex<FontSystem>>,
    loading: Mutex<Option<std::thread::JoinHandle<FontSystem>>>,
    /// Requested family (lowercase) to what the platform substitutes.
    substitutes: Mutex<HashMap<String, FamilyChoice>>,
    /// Primary-font line metrics in em: (ascent, descent, line gap).
    metrics: Mutex<HashMap<cosmic_text::fontdb::ID, (f32, f32, f32)>>,
    /// Kerning around a space, in em, by font and the characters either side.
    space_kerning: Mutex<HashMap<(cosmic_text::fontdb::ID, char, char), f32>>,
}

#[cfg(test)]
mod tests;
