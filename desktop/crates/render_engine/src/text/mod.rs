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

pub struct Fonts {
    system: OnceLock<Mutex<FontSystem>>,
    loading: Mutex<Option<std::thread::JoinHandle<FontSystem>>>,
    substitutes: Mutex<HashMap<String, FamilyChoice>>,
    metrics: Mutex<HashMap<cosmic_text::fontdb::ID, (f32, f32, f32)>>,
    space_kerning: Mutex<HashMap<(cosmic_text::fontdb::ID, char, char), f32>>,
}

#[cfg(test)]
mod tests;
