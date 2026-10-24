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
use std::sync::{Arc, Mutex, OnceLock};

use cosmic_text::{
    Align, Attrs, AttrsList, CacheKey, Family, FontSystem, Hinting, ShapeLine, Shaping, Style, SwashCache, SwashContent,
    Weight, Wrap,
};

/// Lucide (https://lucide.dev, ISC license) as a font, and its icon names
/// with their code points; see `assets/lucide/`.
const ICON_FONT: &[u8] = include_bytes!("../assets/lucide/lucide.ttf");
const ICON_FAMILY: &str = "lucide";
const ICON_NAMES: &str = include_str!("../assets/lucide/names.txt");

fn icon_char(name: &str) -> Option<char> {
    static ICONS: OnceLock<HashMap<&'static str, char>> = OnceLock::new();
    let icons = ICONS.get_or_init(|| {
        ICON_NAMES
            .lines()
            .filter_map(|line| line.split_once(' '))
            .filter_map(|(name, code)| Some((name, char::from_u32(u32::from_str_radix(code, 16).ok()?)?)))
            .collect()
    });
    icons.get(name).copied()
}

/// Replaces `:name:` icon codes with the icon's character and returns the
/// byte ranges of those characters, to be shaped with the icon font.
pub fn expand_icons(text: &str) -> (String, Vec<std::ops::Range<usize>>) {
    if !text.contains(':') {
        return (text.to_string(), Vec::new());
    }
    let mut out = String::with_capacity(text.len());
    let mut icons = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let name_len = after.find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')).unwrap_or(after.len());
        let icon = (name_len > 0 && after[name_len..].starts_with(':')).then(|| icon_char(&after[..name_len])).flatten();
        match icon {
            Some(icon) => {
                icons.push(out.len()..out.len() + icon.len_utf8());
                out.push(icon);
                rest = &after[name_len + 1..];
            }
            None => {
                out.push(':');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    (out, icons)
}

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

impl Fonts {
    pub fn load() -> Arc<Self> {
        let loading = std::thread::Builder::new()
            .name("font-load".into())
            .spawn(|| {
                let mut system = FontSystem::new();
                set_generic_families(&mut system);
                system.db_mut().load_font_data(ICON_FONT.to_vec());
                system
            })
            .expect("spawning font loader");
        Arc::new(Self {
            system: OnceLock::new(),
            loading: Mutex::new(Some(loading)),
            substitutes: Mutex::default(),
            metrics: Mutex::default(),
            space_kerning: Mutex::default(),
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

impl Fonts {
    /// Kerning lost between words: cosmic-text shapes each word on its own,
    /// so pairs with a space (`T` then space, space then `A`) never apply.
    /// The difference, in em, between shaping `before after` whole and in pieces.
    fn space_kerning(&self, system: &mut FontSystem, id: cosmic_text::fontdb::ID, weight: cosmic_text::fontdb::Weight, before: char, after: char) -> f32 {
        if let Some(kern) = self.space_kerning.lock().expect("kerning poisoned").get(&(id, before, after)) {
            return *kern;
        }
        let kern = system.get_font(id, weight).map_or(0.0, |font| {
            let units = font.as_swash().metrics(&[]).units_per_em.max(1) as f32;
            let advance = |text: &str| {
                let mut buffer = harfrust::UnicodeBuffer::new();
                buffer.push_str(text);
                buffer.guess_segment_properties();
                font.shaper().shape(buffer, &[]).glyph_positions().iter().map(|p| p.x_advance).sum::<i32>() as f32
            };
            let whole = advance(&format!("{before} {after}"));
            let pieces = advance(&before.to_string()) + advance(" ") + advance(&after.to_string());
            (whole - pieces) / units
        });
        self.space_kerning.lock().expect("kerning poisoned").insert((id, before, after), kern);
        kern
    }
}

/// Applies kerning around the spaces of one left-to-right line in place,
/// keeping its alignment. Returns the change in width.
fn kern_spaces(fonts: &Fonts, system: &mut FontSystem, paragraph: &str, glyphs: &mut [cosmic_text::LayoutGlyph], size: f32, align: TextAlign) -> f32 {
    if glyphs.iter().any(|g| g.level.is_rtl()) {
        return 0.0;
    }
    let text = |g: &cosmic_text::LayoutGlyph| paragraph.get(g.start..g.end).unwrap_or("");
    let mut shift = 0.0;
    for i in 0..glyphs.len() {
        glyphs[i].x += shift;
        if i == 0 || i + 1 >= glyphs.len() || text(&glyphs[i]) != " " {
            continue;
        }
        let (before, after) = (&glyphs[i - 1], &glyphs[i + 1]);
        if before.font_id != glyphs[i].font_id || after.font_id != glyphs[i].font_id {
            continue;
        }
        let (Some(b), Some(a)) = (text(before).chars().next_back(), text(after).chars().next()) else { continue };
        if b.is_whitespace() || a.is_whitespace() {
            continue;
        }
        shift += fonts.space_kerning(system, glyphs[i].font_id, glyphs[i].font_weight, b, a) * size;
    }
    let realign = match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => -shift / 2.0,
        TextAlign::Right => -shift,
    };
    if realign != 0.0 {
        for glyph in glyphs.iter_mut() {
            glyph.x += realign;
        }
    }
    shift
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
    let icon_attrs = attrs.clone().family(Family::Name(ICON_FAMILY)).weight(Weight::NORMAL).style(Style::Normal);
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
        let (expanded, icons) = expand_icons(paragraph);
        let paragraph = expanded.as_str();
        let mut attrs_list = AttrsList::new(&attrs);
        for range in &icons {
            attrs_list.add_span(range.clone(), &icon_attrs);
        }
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
        for mut line in lines {
            let kerned = kern_spaces(fonts, system, paragraph, &mut line.glyphs, size, style.align);
            line.w += kerned;
            // CSS half-leading with the primary font's metrics: its ascent and
            // descent are centered in the line box; `normal` adds the line gap.
            // An icon at the start of a line does not set the line's metrics.
            let first = line.glyphs.iter().find(|g| !icons.iter().any(|r| r.contains(&g.start))).or(line.glyphs.first());
            let primary = first.and_then(|g| fonts.line_metrics(system, g.font_id, g.font_weight, size));
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

/// The largest distance an SDF glyph encodes, in pixels. Effects reaching
/// further are cut off at this distance.
pub const MAX_SPREAD: u32 = 96;

/// Signed-distance glyphs for text effects (outline, glow, shadow). Each
/// glyph is rasterized at its drawn size and padded by `spread` pixels; a
/// texel stores `0.5 + distance / (2 * spread)`, distance positive outside.
pub struct SdfAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    allocator: etagere::AtlasAllocator,
    glyphs: HashMap<(CacheKey, u32), Option<AtlasGlyph>>,
    swash: SwashCache,
}

impl SdfAtlas {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SDF glyph atlas"),
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

    /// The padded distance field of `key` (its `left`/`top` already include
    /// the padding). `None` for glyphs with no pixels.
    pub fn glyph(&mut self, fonts: &Fonts, queue: &wgpu::Queue, key: CacheKey, spread: u32) -> Result<Option<AtlasGlyph>, AtlasFull> {
        if let Some(entry) = self.glyphs.get(&(key, spread)) {
            return Ok(*entry);
        }
        let image = fonts.with(|system| self.swash.get_image_uncached(system, key));
        let Some(image) = image.filter(|i| i.placement.width > 0 && i.placement.height > 0) else {
            self.glyphs.insert((key, spread), None);
            return Ok(None);
        };
        let coverage: Vec<u8> = match image.content {
            SwashContent::Mask => image.data,
            SwashContent::Color => image.data.chunks_exact(4).map(|p| p[3]).collect(),
            SwashContent::SubpixelMask => image.data.chunks_exact(4).map(|p| p[1]).collect(),
        };
        let (field, width, height) = signed_distance(&coverage, image.placement.width, image.placement.height, spread);
        let allocation = self
            .allocator
            .allocate(etagere::size2(width as i32 + 2, height as i32 + 2))
            .ok_or(AtlasFull)?;
        let (x, y) = (allocation.rectangle.min.x as u32 + 1, allocation.rectangle.min.y as u32 + 1);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
            &field,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        let entry = AtlasGlyph { x, y, width, height, left: image.placement.left - spread as i32, top: image.placement.top + spread as i32 };
        self.glyphs.insert((key, spread), Some(entry));
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

/// The signed distance field of a coverage mask, padded by `spread` on every
/// side and encoded as `0.5 + d / (2 * spread)` (d in pixels, positive
/// outside). Exact Euclidean distances (Felzenszwalb–Huttenlocher); pixels
/// on the antialiased edge use their coverage for sub-pixel accuracy.
pub fn signed_distance(coverage: &[u8], width: u32, height: u32, spread: u32) -> (Vec<u8>, u32, u32) {
    let pad = spread as usize;
    let (w, h) = (width as usize + 2 * pad, height as usize + 2 * pad);
    let cover = |x: usize, y: usize| -> u8 {
        if x < pad || y < pad || x >= pad + width as usize || y >= pad + height as usize {
            0
        } else {
            coverage[(y - pad) * width as usize + (x - pad)]
        }
    };
    const FAR: f32 = 1e20;
    let mut to_inside = vec![FAR; w * h];
    let mut to_outside = vec![FAR; w * h];
    for y in 0..h {
        for x in 0..w {
            if cover(x, y) >= 128 {
                to_inside[y * w + x] = 0.0;
            } else {
                to_outside[y * w + x] = 0.0;
            }
        }
    }
    distance_transform(&mut to_inside, w, h);
    distance_transform(&mut to_outside, w, h);
    let scale = 1.0 / (2.0 * spread.max(1) as f32);
    let mut field = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let c = cover(x, y);
            let d = if c > 0 && c < 255 {
                0.5 - c as f32 / 255.0
            } else if c >= 128 {
                -(to_outside[i].sqrt() - 0.5)
            } else {
                to_inside[i].sqrt() - 0.5
            };
            field.push(((0.5 + d * scale).clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    (field, w as u32, h as u32)
}

/// Squared Euclidean distance to the nearest zero, in place: columns, then rows.
fn distance_transform(grid: &mut [f32], w: usize, h: usize) {
    let n = w.max(h);
    let (mut f, mut d, mut v, mut z) = (vec![0.0; n], vec![0.0; n], vec![0usize; n], vec![0.0; n + 1]);
    for x in 0..w {
        for y in 0..h {
            f[y] = grid[y * w + x];
        }
        transform_1d(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            grid[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&grid[y * w..(y + 1) * w]);
        transform_1d(&f[..w], &mut d[..w], &mut v, &mut z);
        grid[y * w..(y + 1) * w].copy_from_slice(&d[..w]);
    }
}

/// The lower envelope of parabolas rooted at each sample (squared distances).
fn transform_1d(f: &[f32], d: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        let intersect = |p: usize| ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
        let mut s = intersect(v[k]);
        while s <= z[k] {
            k -= 1;
            s = intersect(v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        *out = (q as f32 - p as f32).powi(2) + f[p];
    }
}

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
    fn icon_codes_become_lucide_glyphs() {
        let (expanded, icons) = expand_icons("Pray :hand-heart: at 10:30: or :nope:");
        assert_eq!(icons.len(), 1);
        assert_eq!(&expanded[icons[0].clone()], "\u{e5bd}");
        assert!(expanded.starts_with("Pray \u{e5bd} at 10:30: or :nope:"), "{expanded:?}");
        let style = TextStyle::new("sans-serif", 48.0);
        let fonts = fonts();
        let laid = layout(&fonts, ":heart:", &style, None);
        assert_eq!(laid.glyphs.len(), 1);
        let font = laid.glyphs[0].key.font_id;
        let family = fonts.with(|system| system.db().face(font).map(|f| f.families[0].0.clone()));
        assert_eq!(family.as_deref(), Some("lucide"));
        assert!(laid.width > 30.0, "an icon is about one em wide: {}", laid.width);
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
    fn distance_fields_measure_from_the_glyph_edge() {
        // A 4×4 solid square padded by 8: the center is inside, far pixels outside.
        let (field, w, h) = signed_distance(&[255; 16], 4, 4, 8);
        assert_eq!((w, h), (20, 20));
        let at = |x: usize, y: usize| (field[y * 20 + x] as f32 / 255.0 - 0.5) * 16.0;
        assert!(at(10, 10) < -1.0, "inside is negative: {}", at(10, 10));
        // The square covers pixels 8..12; pixel centers are at +0.5.
        assert!((at(14, 10) - 2.5).abs() < 0.1, "2.5 px right of the edge: {}", at(14, 10));
        assert!((at(19, 10) - 7.5).abs() < 0.1, "{}", at(19, 10));
        assert!((at(0, 0) - 8.0).abs() < 0.1, "clamped at the spread: {}", at(0, 0));
    }

    #[test]
    fn kerning_around_spaces_is_applied_and_keeps_centering() {
        let fonts = fonts();
        // Fonts kern pairs like "T"–space or space–"A" differently; whatever the
        // font, shaping must stay within a pixel of the per-word result and
        // centered lines must stay centered.
        let style = TextStyle { align: TextAlign::Center, ..TextStyle::new("sans-serif", 64.0) };
        let laid = layout(&fonts, "T A", &style, Some(1000.0));
        let left = laid.glyphs.first().map(|g| g.x).unwrap_or_default() as f32;
        assert!((left - (1000.0 - laid.width) / 2.0).abs() < 2.0, "centered: left {left}, width {}", laid.width);
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
