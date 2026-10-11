//! Text shaping, layout and glyph rasterization.
//!
//! cosmic-text shapes (with font fallback and bidi); layout here adds CSS
//! letter spacing before wrapping, which cosmic-text's `Buffer` cannot do.
//! Glyphs are rasterized with swash into a per-output mask atlas.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use cosmic_text::{
    Align, Attrs, AttrsList, CacheKey, Family, FontSystem, Hinting, ShapeLine, Shaping, Style, SwashCache, SwashContent,
    Weight, Wrap,
};

/// The process-wide font database. Loading system fonts is slow, so it starts
/// on a background thread when the engine starts.
pub struct Fonts {
    system: OnceLock<Mutex<FontSystem>>,
    loading: Mutex<Option<std::thread::JoinHandle<FontSystem>>>,
    /// Requested family (lowercase) to what the platform substitutes.
    substitutes: Mutex<HashMap<String, FamilyChoice>>,
    /// Primary-font line metrics in em: (ascent, descent, line gap).
    metrics: Mutex<HashMap<cosmic_text::fontdb::ID, (f32, f32, f32)>>,
}

impl Fonts {
    pub fn load() -> Arc<Self> {
        let loading = std::thread::Builder::new()
            .name("font-load".into())
            .spawn(|| {
                let mut system = FontSystem::new();
                set_generic_families(&mut system);
                system
            })
            .expect("spawning font loader");
        Arc::new(Self {
            system: OnceLock::new(),
            loading: Mutex::new(Some(loading)),
            substitutes: Mutex::default(),
            metrics: Mutex::default(),
        })
    }

    pub fn with<T>(&self, f: impl FnOnce(&mut FontSystem) -> T) -> T {
        let system = self.system.get_or_init(|| {
            let handle = self.loading.lock().expect("font loader poisoned").take();
            Mutex::new(handle.and_then(|h| h.join().ok()).unwrap_or_else(FontSystem::new))
        });
        f(&mut system.lock().expect("font system poisoned"))
    }

    /// The family to use for `requested`: itself when installed, else what
    /// the platform would substitute (fontconfig on Linux).
    fn resolve(&self, system: &FontSystem, requested: &str) -> FamilyChoice {
        let key = requested.trim().to_ascii_lowercase();
        match key.as_str() {
            "" | "sans-serif" => return FamilyChoice::SansSerif,
            "serif" => return FamilyChoice::Serif,
            "monospace" => return FamilyChoice::Monospace,
            _ => {}
        }
        if let Some(choice) = self.substitutes.lock().expect("font substitutes poisoned").get(&key) {
            return choice.clone();
        }
        let choice = if installed(system, requested.trim()) {
            FamilyChoice::Named(requested.trim().to_string())
        } else {
            platform_substitute(requested.trim())
                .filter(|name| installed(system, name))
                .map(FamilyChoice::Named)
                .unwrap_or(FamilyChoice::SansSerif)
        };
        self.substitutes.lock().expect("font substitutes poisoned").insert(key, choice.clone());
        choice
    }

    /// Blink's line metrics for a font at `size`: rounded ascent, descent
    /// and line gap in pixels.
    fn line_metrics(&self, system: &mut FontSystem, id: cosmic_text::fontdb::ID, weight: cosmic_text::fontdb::Weight, size: f32) -> Option<(f32, f32, f32)> {
        let cached = self.metrics.lock().expect("font metrics poisoned").get(&id).copied();
        let em = match cached {
            Some(em) => em,
            None => {
                let font = system.get_font(id, weight)?;
                let m = font.as_swash().metrics(&[]);
                let units = m.units_per_em.max(1) as f32;
                let em = (m.ascent / units, m.descent.abs() / units, m.leading / units);
                self.metrics.lock().expect("font metrics poisoned").insert(id, em);
                em
            }
        };
        Some(((em.0 * size).round(), (em.1 * size).round(), (em.2 * size).round()))
    }
}

fn installed(system: &FontSystem, name: &str) -> bool {
    system.db().faces().any(|face| face.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
}

/// Points the generic families at what browsers use on this platform.
fn set_generic_families(system: &mut FontSystem) {
    #[cfg(target_os = "linux")]
    let generics = [
        ("sans-serif", fontconfig_match("sans-serif")),
        ("serif", fontconfig_match("serif")),
        ("monospace", fontconfig_match("monospace")),
    ];
    #[cfg(not(target_os = "linux"))]
    let generics = [
        ("sans-serif", Some("Arial".to_string())),
        ("serif", Some("Times New Roman".to_string())),
        ("monospace", Some("Consolas".to_string())),
    ];
    for (generic, family) in generics {
        let Some(family) = family.filter(|f| installed(system, f)) else { continue };
        let db = system.db_mut();
        match generic {
            "sans-serif" => db.set_sans_serif_family(family),
            "serif" => db.set_serif_family(family),
            _ => db.set_monospace_family(family),
        }
    }
}

/// Families substituted for missing ones when fontconfig is unavailable.
const ALIASES: &[(&str, &str)] = &[
    ("arial", "Liberation Sans"),
    ("helvetica", "Liberation Sans"),
    ("times new roman", "Liberation Serif"),
    ("courier new", "Liberation Mono"),
];

fn platform_substitute(requested: &str) -> Option<String> {
    #[cfg(target_os = "linux")]
    if let Some(family) = fontconfig_match(requested) {
        return Some(family);
    }
    let key = requested.to_ascii_lowercase();
    ALIASES.iter().find(|(name, _)| *name == key).map(|(_, alias)| alias.to_string())
}

#[cfg(target_os = "linux")]
fn fontconfig_match(pattern: &str) -> Option<String> {
    let output = std::process::Command::new("fc-match").args(["-f", "%{family[0]}", pattern]).output().ok()?;
    let family = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !family.is_empty()).then_some(family)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum FamilyChoice {
    Named(String),
    SansSerif,
    Serif,
    Monospace,
}

impl FamilyChoice {
    fn family(&self) -> Family<'_> {
        match self {
            FamilyChoice::Named(name) => Family::Name(name),
            FamilyChoice::SansSerif => Family::SansSerif,
            FamilyChoice::Serif => Family::Serif,
            FamilyChoice::Monospace => Family::Monospace,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Everything that changes where glyphs go. Pixel values are physical.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub family: String,
    pub size: f32,
    pub weight: u16,
    pub italic: bool,
    /// Line box height in pixels (CSS `line-height` times the size); zero or
    /// less means CSS `normal`, the font's own ascent plus descent.
    pub line_height: f32,
    /// Added after every glyph, like CSS `letter-spacing`.
    pub letter_spacing: f32,
    pub align: TextAlign,
}

impl TextStyle {
    pub fn new(family: &str, size: f32) -> Self {
        Self {
            family: family.to_string(),
            size,
            weight: 400,
            italic: false,
            line_height: size * 1.2,
            letter_spacing: 0.0,
            align: TextAlign::Left,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PlacedGlyph {
    pub key: CacheKey,
    /// Pen position in pixels relative to the layout's top-left corner.
    pub x: i32,
    pub y: i32,
}

/// Laid-out text. Lines are CSS line boxes; glyphs carry rasterization keys.
#[derive(Debug, Clone, Default)]
pub struct TextLayout {
    pub glyphs: Vec<PlacedGlyph>,
    /// Width of the widest line, in pixels.
    pub width: f32,
    pub height: f32,
    pub lines: usize,
}

/// Lays out `text` in `width` pixels (`None` for no wrapping). Newlines start
/// paragraphs; long words break anywhere (CSS `overflow-wrap: anywhere`).
pub fn layout(fonts: &Fonts, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
    fonts.with(|system| {
        let family = fonts.resolve(system, &style.family);
        layout_with(fonts, system, &family, text, style, width)
    })
}

/// The installed face the browser would choose: nearest weight by the CSS
/// font-matching rules, and the upright face when the family has no italic
/// (the operator UI disables font synthesis).
fn resolve_face(system: &FontSystem, family: &FamilyChoice, weight: u16, italic: bool) -> (String, Weight, Style) {
    let db = system.db();
    let name = db.family_name(&family.family()).to_string();
    let faces: Vec<_> = db.faces().filter(|f| f.families.iter().any(|(n, _)| *n == name)).collect();
    let style = if italic && faces.iter().any(|f| f.style == Style::Italic) {
        Style::Italic
    } else if italic && faces.iter().any(|f| f.style == Style::Oblique) {
        Style::Oblique
    } else {
        Style::Normal
    };
    let weights: Vec<u16> = faces.iter().filter(|f| f.style == style).map(|f| f.weight.0).collect();
    (name, Weight(css_weight(weight, &weights).unwrap_or(weight)), style)
}

/// CSS Fonts 4 weight matching.
fn css_weight(desired: u16, available: &[u16]) -> Option<u16> {
    let mut sorted = available.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let heavier = |from: u16| sorted.iter().copied().filter(move |w| *w >= from);
    let lighter = |below: u16| sorted.iter().rev().copied().filter(move |w| *w < below);
    match desired {
        400..=500 => heavier(desired)
            .find(|w| *w <= 500)
            .or_else(|| lighter(desired).next())
            .or_else(|| heavier(desired).find(|w| *w > 500)),
        0..=399 => sorted.iter().rev().copied().find(|w| *w <= desired).or_else(|| heavier(desired).next()),
        _ => heavier(desired).next().or_else(|| lighter(desired).next()),
    }
}

fn layout_with(fonts: &Fonts, system: &mut FontSystem, family: &FamilyChoice, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
    let (name, weight, font_style) = resolve_face(system, family, style.weight.clamp(1, 1000), style.italic);
    let attrs = Attrs::new().family(Family::Name(&name)).weight(weight).style(font_style);
    let attrs_list = AttrsList::new(&attrs);
    let align = match style.align {
        TextAlign::Left => Align::Left,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::Right,
    };
    let size = style.size.max(1.0);
    let spacing_em = style.letter_spacing / size;
    let mut out = TextLayout::default();
    let mut top = 0.0f32;
    for paragraph in text.replace("\r\n", "\n").split('\n') {
        let mut shaped = ShapeLine::new(system, paragraph, &attrs_list, Shaping::Advanced, 8);
        if spacing_em != 0.0 {
            for glyph in shaped.spans.iter_mut().flat_map(|s| s.words.iter_mut()).flat_map(|w| w.glyphs.iter_mut()) {
                glyph.x_advance += spacing_em;
            }
        }
        let lines = shaped.layout(size, width, Wrap::WordOrGlyph, Some(align), None, Hinting::Disabled);
        if lines.is_empty() {
            top += if style.line_height > 0.0 { style.line_height } else { size * 1.2 };
            out.lines += 1;
            continue;
        }
        for line in lines {
            // CSS half-leading with the primary font's metrics: its ascent and
            // descent are centered in the line box; `normal` adds the line gap.
            let primary = line.glyphs.first().and_then(|g| fonts.line_metrics(system, g.font_id, g.font_weight, size));
            let (ascent, descent, gap) = primary.unwrap_or((line.max_ascent, line.max_descent, 0.0));
            let content = ascent + descent;
            let line_height = if style.line_height > 0.0 { style.line_height } else { content + gap };
            let baseline = top + (line_height - content) / 2.0 + ascent;
            for glyph in &line.glyphs {
                let physical = glyph.physical((0.0, baseline), 1.0);
                out.glyphs.push(PlacedGlyph { key: physical.cache_key, x: physical.x, y: physical.y });
            }
            out.width = out.width.max(line.w);
            top += line_height;
            out.lines += 1;
        }
    }
    out.height = top;
    out
}

/// Where a rasterized glyph lives in the atlas and how it sits on the pen.
#[derive(Debug, Clone, Copy)]
pub struct AtlasGlyph {
    /// Texel rectangle in the atlas.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Offset of the image's top-left from the pen position (y down).
    pub left: i32,
    pub top: i32,
}

const ATLAS_SIZE: u32 = 2048;

/// A single-channel coverage atlas owned by one output.
pub struct GlyphAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    allocator: etagere::AtlasAllocator,
    glyphs: HashMap<CacheKey, Option<AtlasGlyph>>,
    swash: SwashCache,
}

impl GlyphAtlas {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph atlas"),
            size: wgpu::Extent3d { width: ATLAS_SIZE, height: ATLAS_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            allocator: etagere::AtlasAllocator::new(etagere::size2(ATLAS_SIZE as i32, ATLAS_SIZE as i32)),
            glyphs: HashMap::new(),
            swash: SwashCache::new(),
        }
    }

    /// The atlas entry for `key`, rasterizing on first use. `None` for glyphs
    /// with no pixels (spaces). Returns `Err` when the atlas is full; the
    /// caller clears it and draws the frame again.
    pub fn glyph(&mut self, fonts: &Fonts, queue: &wgpu::Queue, key: CacheKey) -> Result<Option<AtlasGlyph>, AtlasFull> {
        if let Some(entry) = self.glyphs.get(&key) {
            return Ok(*entry);
        }
        let image = fonts.with(|system| self.swash.get_image_uncached(system, key));
        let Some(image) = image.filter(|i| i.placement.width > 0 && i.placement.height > 0) else {
            self.glyphs.insert(key, None);
            return Ok(None);
        };
        let (width, height) = (image.placement.width, image.placement.height);
        let coverage: Vec<u8> = match image.content {
            SwashContent::Mask => image.data,
            // Color glyphs (emoji) keep their shape; color is not supported.
            SwashContent::Color => image.data.chunks_exact(4).map(|p| p[3]).collect(),
            SwashContent::SubpixelMask => image.data.chunks_exact(4).map(|p| p[1]).collect(),
        };
        // One texel of padding keeps linear sampling from bleeding neighbors.
        let allocation = self
            .allocator
            .allocate(etagere::size2(width as i32 + 2, height as i32 + 2))
            .ok_or(AtlasFull)?;
        let (x, y) = (allocation.rectangle.min.x as u32 + 1, allocation.rectangle.min.y as u32 + 1);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
            &coverage,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        let entry = AtlasGlyph { x, y, width, height, left: image.placement.left, top: image.placement.top };
        self.glyphs.insert(key, Some(entry));
        Ok(Some(entry))
    }

    pub fn clear(&mut self) {
        self.allocator.clear();
        self.glyphs.clear();
    }

    pub fn size(&self) -> f32 {
        ATLAS_SIZE as f32
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AtlasFull;

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Arc<Fonts> {
        static FONTS: OnceLock<Arc<Fonts>> = OnceLock::new();
        FONTS.get_or_init(Fonts::load).clone()
    }

    #[test]
    fn lines_are_css_line_boxes_and_newlines_start_paragraphs() {
        let style = TextStyle { line_height: 40.0, ..TextStyle::new("sans-serif", 32.0) };
        let laid = layout(&fonts(), "one\ntwo\n\nfour", &style, None);
        assert_eq!(laid.lines, 4);
        assert_eq!(laid.height, 160.0);
    }

    #[test]
    fn long_text_wraps_within_the_width() {
        let style = TextStyle::new("sans-serif", 32.0);
        let text = "Amazing grace how sweet the sound that saved a wretch like me";
        let laid = layout(&fonts(), text, &style, Some(400.0));
        assert!(laid.lines > 1);
        assert!(laid.width <= 400.0 + 0.5, "{}", laid.width);
    }

    #[test]
    fn letter_spacing_widens_lines() {
        let plain = TextStyle::new("sans-serif", 32.0);
        let spaced = TextStyle { letter_spacing: 10.0, ..plain.clone() };
        let a = layout(&fonts(), "spacing", &plain, None);
        let b = layout(&fonts(), "spacing", &spaced, None);
        assert!((b.width - a.width - 70.0).abs() < 1.0, "{} vs {}", a.width, b.width);
    }

    #[test]
    fn weights_match_like_css() {
        assert_eq!(css_weight(800, &[400, 700]), Some(700));
        assert_eq!(css_weight(300, &[400, 700]), Some(400));
        assert_eq!(css_weight(450, &[300, 400, 700]), Some(400));
        assert_eq!(css_weight(500, &[300, 700]), Some(300));
        assert_eq!(css_weight(600, &[400, 700, 900]), Some(700));
        assert_eq!(css_weight(400, &[]), None);
    }

    #[test]
    fn missing_families_fall_back_to_installed_aliases() {
        let fonts = fonts();
        fonts.with(|system| {
            assert_eq!(fonts.resolve(system, "sans-serif"), FamilyChoice::SansSerif);
            assert_ne!(fonts.resolve(system, "Definitely Not A Font"), FamilyChoice::Named("Definitely Not A Font".into()));
        });
    }
}
