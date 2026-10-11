//! The audience output: background media, Black/Logo overrides and the
//! lyric box. Margins are inset inside the output, so they always hold.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::frame::SharedFrame;
use crate::media::{ImageSlot, MediaTextures};
use crate::paint::{fit, with_alpha, DrawList, Rect};
use crate::scene::{millis, Activity, Display, Fade, LyricStyle, MainScene, MediaLayer, MediaSource, Vertical};
use crate::text::{layout, Fonts, TextAlign, TextLayout, TextStyle};

enum Handle {
    Video(Arc<SharedFrame>),
    Image(Arc<ImageSlot>),
}

impl Handle {
    fn open(source: &MediaSource, media: &MediaTextures) -> Self {
        match source {
            MediaSource::Video(key) => Handle::Video(media.video(key)),
            MediaSource::Image(path) => Handle::Image(media.image(path)),
        }
    }

    fn ready(&self) -> bool {
        match self {
            Handle::Video(frame) => frame.size().is_some(),
            Handle::Image(slot) => slot.ready().is_some(),
        }
    }

    fn generation(&self) -> u64 {
        match self {
            Handle::Video(frame) => frame.generation(),
            Handle::Image(_) => 0,
        }
    }

    fn draw(&self, list: &mut DrawList, area: Rect, cover: bool, opacity: f32) {
        match self {
            Handle::Video(frame) => {
                if let Some(size) = frame.size() {
                    let (rect, uv) = fit(size, area, cover);
                    let [u0, v0, u1, v1] = uv;
                    let [_, _, su, sv] = frame.uv();
                    list.image(rect, [u0 * su, v0 * sv, u1 * su, v1 * sv], frame.texture(), opacity);
                }
            }
            Handle::Image(slot) => {
                if let Some((texture, size)) = slot.ready() {
                    let (rect, uv) = fit(size, area, cover);
                    list.image(rect, uv, &texture, opacity);
                }
            }
        }
    }
}

struct Layer {
    source: MediaSource,
    cover: bool,
    handle: Handle,
    opacity: Fade,
}

/// A new source fades in only once it
/// can be drawn, while the previous one stays until then and fades out.
#[derive(Default)]
pub(crate) struct MediaStage {
    target: Option<MediaSource>,
    shown: Option<MediaSource>,
    layers: Vec<Layer>,
    remove_at: Option<Instant>,
}

impl MediaStage {
    pub fn set(&mut self, layer: Option<&MediaLayer>, media: &MediaTextures, now: Instant, fade_ms: f32) {
        let key = layer.map(|l| l.source.clone());
        if let Some(existing) = layer.and_then(|l| self.layers.iter_mut().find(|x| x.source == l.source)) {
            existing.cover = layer.is_some_and(|l| l.cover);
        }
        if key == self.target {
            return;
        }
        // Keep only what is on screen; an earlier target still loading is dropped.
        let shown = self.shown.clone();
        self.layers.retain(|l| Some(&l.source) == shown.as_ref());
        if let Some(layer) = layer {
            if !self.layers.iter().any(|l| l.source == layer.source) {
                self.layers.push(Layer {
                    source: layer.source.clone(),
                    cover: layer.cover,
                    handle: Handle::open(&layer.source, media),
                    opacity: Fade::fixed(0.0, now),
                });
            }
        } else {
            self.shown = None;
            for l in &mut self.layers {
                l.opacity.retarget(0.0, now, millis(fade_ms));
            }
            self.remove_at = Some(now + millis(fade_ms));
        }
        self.target = key;
    }

    /// Advances readiness and removes faded layers.
    pub fn update(&mut self, now: Instant, fade_ms: f32) {
        if let Some(target) = self.target.clone() {
            let ready = self.layers.iter().any(|l| l.source == target && l.handle.ready());
            if self.shown.as_ref() != Some(&target) && ready {
                for l in &mut self.layers {
                    l.opacity.retarget(if l.source == target { 1.0 } else { 0.0 }, now, millis(fade_ms));
                }
                self.shown = Some(target);
                self.remove_at = Some(now + millis(fade_ms));
            }
        }
        if self.remove_at.is_some_and(|at| now >= at) {
            let target = self.target.clone();
            self.layers.retain(|l| Some(&l.source) == target.as_ref());
            self.remove_at = None;
        }
    }

    pub fn draw(&self, list: &mut DrawList, area: Rect, now: Instant) {
        for layer in &self.layers {
            let opacity = layer.opacity.value(now);
            if opacity > 0.0 {
                layer.handle.draw(list, area, layer.cover, opacity);
            }
        }
    }

    /// Whether another frame would differ: a fade, a pending source or a
    /// visible video (drawn every refresh); plus the visible generations.
    pub fn activity(&self, now: Instant) -> Activity {
        let fading = self.layers.iter().any(|l| l.opacity.running(now)) || self.remove_at.is_some();
        let pending = self.target.is_some() && self.shown != self.target;
        let video = self.layers.iter().any(|l| matches!(l.handle, Handle::Video(_)) && l.opacity.value(now) > 0.0);
        let generations = self
            .layers
            .iter()
            .filter(|l| l.opacity.value(now) > 0.0)
            .map(|l| l.handle.generation())
            .sum();
        Activity { redraw: fading || pending || video, settled: !(fading || pending), generation: generations }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Highest layer opacity, for a backing drawn behind letterboxed media.
    pub fn opacity(&self, now: Instant) -> f32 {
        self.layers.iter().filter(|l| l.handle.ready()).map(|l| l.opacity.value(now)).fold(0.0, f32::max)
    }
}

struct Lyric {
    text: String,
    opacity: Fade,
}

/// Port of `LyricLayers`: the new text fades in while the previous fades out
/// from wherever it was, both on one clock; the box persists across them.
#[derive(Default)]
struct Lyrics {
    current: Option<Lyric>,
    previous: Option<Lyric>,
    drop_previous_at: Option<Instant>,
}

impl Lyrics {
    fn set(&mut self, text: &str, now: Instant, fade_ms: f32) {
        if self.current.as_ref().map_or("", |c| c.text.as_str()) == text {
            return;
        }
        let duration = millis(fade_ms);
        self.previous = self.current.take().map(|mut lyric| {
            lyric.opacity.retarget(0.0, now, duration);
            lyric
        });
        let mut opacity = Fade::fixed(0.0, now);
        opacity.retarget(1.0, now, duration);
        self.current = Some(Lyric { text: text.to_string(), opacity });
        self.drop_previous_at = Some(now + duration);
    }

    fn update(&mut self, now: Instant) {
        if self.drop_previous_at.is_some_and(|at| now >= at) {
            self.previous = None;
            self.drop_previous_at = None;
        }
    }

    fn visible(&self) -> impl Iterator<Item = &Lyric> {
        self.previous.iter().chain(self.current.iter()).filter(|l| !l.text.is_empty())
    }

    fn running(&self, now: Instant) -> bool {
        self.drop_previous_at.is_some() || self.current.as_ref().is_some_and(|c| c.opacity.running(now))
    }
}

/// Geometry of the lyric composition for one output size.
#[derive(Debug, Clone, PartialEq)]
pub struct LyricGeometry {
    /// Area inside the margins.
    pub area: Rect,
    pub border: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub text_style: TextStyle,
    /// Wrapping width for the text.
    pub text_width: f32,
}

impl LyricGeometry {
    pub fn new(style: &LyricStyle, size: (u32, u32)) -> Self {
        let (w, h) = (size.0 as f32, size.1 as f32);
        let cq = w / 100.0;
        let (pad_x, pad_y) = (style.margin_x * cq, style.margin_y * 9.0 / 16.0 * cq);
        let area = Rect::new(pad_x, pad_y, (w - 2.0 * pad_x).max(0.0), (h - 2.0 * pad_y).max(0.0));
        let border = style.border / 16.0 * cq;
        let (padding_x, padding_y) = (cq, 0.4 * cq);
        let font_size = style.size * cq;
        let text_style = TextStyle {
            family: style.font.clone(),
            size: font_size,
            weight: style.weight,
            italic: style.italic,
            line_height: style.line_height * font_size,
            letter_spacing: style.letter_spacing / 16.0 * cq,
            align: style.align,
        };
        let text_width = (area.w - 2.0 * border - 2.0 * padding_x).max(1.0);
        Self { area, border, padding_x, padding_y, text_style, text_width }
    }

    /// The box around text `text_height` tall.
    pub fn text_box(&self, vertical: Vertical, text_height: f32) -> Rect {
        let height = text_height + 2.0 * self.padding_y + 2.0 * self.border;
        let y = match vertical {
            Vertical::Top => self.area.y,
            Vertical::Center => self.area.y + (self.area.h - height) / 2.0,
            Vertical::Bottom => self.area.bottom() - height,
        };
        Rect::new(self.area.x, y, self.area.w, height)
    }
}

pub struct MainRenderer {
    scene: MainScene,
    background: MediaStage,
    logo: MediaStage,
    lyrics: Lyrics,
    layouts: HashMap<String, Arc<TextLayout>>,
    layout_key: Option<(LyricGeometry, bool)>,
    logo_text: Option<(u32, Arc<TextLayout>)>,
}

impl Default for MainRenderer {
    fn default() -> Self {
        Self {
            scene: MainScene { transition_ms: 250.0, ..Default::default() },
            background: MediaStage::default(),
            logo: MediaStage::default(),
            lyrics: Lyrics::default(),
            layouts: HashMap::new(),
            layout_key: None,
            logo_text: None,
        }
    }
}

impl MainRenderer {
    pub fn set_scene(&mut self, scene: MainScene, media: &MediaTextures, now: Instant) {
        let fade = scene.transition_ms;
        self.background.set(scene.background.as_ref(), media, now, fade);
        if scene.display == Display::Logo {
            let logo = scene.logo.clone().map(|source| MediaLayer { source, cover: false });
            self.logo.set(logo.as_ref(), media, now, fade);
        } else {
            // The logo composition unmounts at once when leaving Logo.
            self.logo.clear();
        }
        let text = if scene.display == Display::Live { scene.text.as_str() } else { "" };
        self.lyrics.set(text, now, fade);
        self.scene = scene;
    }

    /// Whether drawing now would differ from the last frame, given the video
    /// generations it showed.
    pub fn activity(&self, now: Instant) -> Activity {
        let lyrics = self.lyrics.running(now);
        self.background
            .activity(now)
            .and(self.logo.activity(now))
            .and(Activity { redraw: lyrics, settled: !lyrics, generation: 0 })
    }

    pub fn frame(&mut self, fonts: &Arc<Fonts>, size: (u32, u32), now: Instant) -> DrawList {
        let fade = self.scene.transition_ms;
        self.background.update(now, fade);
        self.logo.update(now, fade);
        self.lyrics.update(now);
        let (w, h) = (size.0 as f32, size.1 as f32);
        let full = Rect::new(0.0, 0.0, w, h);
        let mut list = DrawList::new();
        self.background.draw(&mut list, full, now);
        if matches!(self.scene.display, Display::Black | Display::Logo) {
            list.fill(full, [0.0, 0.0, 0.0, 1.0]);
        }

        let style = self.scene.style.clone();
        let geometry = LyricGeometry::new(&style, size);
        let key = (geometry.clone(), style.caps);
        if self.layout_key.as_ref() != Some(&key) {
            self.layouts.clear();
            self.layout_key = Some(key);
        }
        let visible: Vec<(String, f32)> = self.lyrics.visible().map(|l| (l.text.clone(), l.opacity.value(now))).collect();
        if !visible.is_empty() {
            let layouts: Vec<(Arc<TextLayout>, f32)> =
                visible.iter().map(|(text, opacity)| (self.layout(fonts, text, &geometry, style.caps), *opacity)).collect();
            let text_height = layouts.iter().map(|(l, _)| l.height).fold(0.0, f32::max);
            let rect = geometry.text_box(style.vertical, text_height);
            list.fill(rect, style.box_color);
            list.border(rect, geometry.border, style.border_color);
            let (x, y) = (rect.x + geometry.border + geometry.padding_x, rect.y + geometry.border + geometry.padding_y);
            for (laid, opacity) in &layouts {
                list.text(laid, x, y, with_alpha(style.color, *opacity));
            }
        }
        self.layouts.retain(|text, _| visible.iter().any(|(t, _)| t == text));

        if self.scene.display == Display::Logo {
            if self.scene.logo.is_some() {
                self.logo.draw(&mut list, geometry.area, now);
            } else {
                // No logo configured: show "MAGI" in the root font.
                let cq = w / 100.0;
                let laid = match &self.logo_text {
                    Some((width, laid)) if *width == size.0 => laid.clone(),
                    _ => {
                        let style = TextStyle { weight: 800, line_height: 15.0 * cq, align: TextAlign::Center, ..TextStyle::new("Segoe UI", 10.0 * cq) };
                        let laid = Arc::new(layout(fonts, "MAGI", &style, Some(geometry.area.w)));
                        self.logo_text = Some((size.0, laid.clone()));
                        laid
                    }
                };
                let y = geometry.area.y + (geometry.area.h - laid.height) / 2.0;
                list.text(&laid, geometry.area.x, y, [1.0, 1.0, 1.0, 1.0]);
            }
        }
        list
    }

    fn layout(&mut self, fonts: &Fonts, text: &str, geometry: &LyricGeometry, caps: bool) -> Arc<TextLayout> {
        if let Some(laid) = self.layouts.get(text) {
            return laid.clone();
        }
        let shown = if caps { text.to_uppercase() } else { text.to_string() };
        let laid = Arc::new(layout(fonts, &shown, &geometry.text_style, Some(geometry.text_width)));
        self.layouts.insert(text.to_string(), laid.clone());
        laid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_matches_the_parity_reference_at_1080p() {
        // Values from the parity references in tools/parity.
        let g = LyricGeometry::new(&LyricStyle::default(), (1920, 1080));
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(close(g.area.x, 96.0) && close(g.area.y, 108.0) && close(g.area.w, 1728.0) && close(g.area.h, 864.0), "{:?}", g.area);
        assert!((g.text_style.size - 115.2).abs() < 1e-3);
        assert!((g.text_style.line_height - 138.24).abs() < 1e-3);
        let rect = g.text_box(Vertical::Bottom, 3.0 * 138.24);
        assert!((rect.y - 541.95).abs() < 0.05, "{rect:?}");
        assert!((rect.h - 430.05).abs() < 0.05, "{rect:?}");
    }

    #[test]
    fn geometry_scales_with_width_on_other_aspects() {
        let g = LyricGeometry::new(&LyricStyle { border: 16.0, ..LyricStyle::default() }, (1440, 1080));
        let cq = 14.4;
        assert_eq!(g.area.x, 5.0 * cq);
        assert_eq!(g.area.y, 10.0 * 9.0 / 16.0 * cq);
        assert_eq!(g.border, cq);
        assert_eq!(g.text_width, 1440.0 - 10.0 * cq - 2.0 * cq - 2.0 * cq);
    }
}
