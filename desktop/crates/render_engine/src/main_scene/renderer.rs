use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::media::MediaTextures;
use crate::paint::{with_alpha, DrawList, Rect};
use crate::scene::{Activity, Display, MainScene, MediaLayer, TransitionKind};
use crate::text::{layout, Fonts, TextAlign, TextLayout, TextStyle};

use super::*;

pub struct MainRenderer {
    pub(super) scene: MainScene,
    pub(super) background: MediaStage,
    pub(super) logo: MediaStage,
    pub(super) lyrics: Lyrics,
    pub(super) layouts: HashMap<String, Arc<TextLayout>>,
    pub(super) layout_key: Option<(LyricGeometry, bool)>,
    pub(super) logo_text: Option<(u32, Arc<TextLayout>)>,
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
        self.background.set_transition(scene.transition);
        self.background.set(scene.background.as_ref(), media, now, fade);
        if scene.display == Display::Logo {
            let logo = scene.logo.clone().map(|source| MediaLayer { source, cover: false });
            self.logo.set(logo.as_ref(), media, now, fade);
        } else {
            self.logo.clear();
        }
        let text = if scene.display == Display::Live { scene.text.as_str() } else { "" };
        self.lyrics.set(text, now, fade, scene.transition);
        self.scene = scene;
    }

    pub fn activity(&self, now: Instant) -> Activity {
        let lyrics = self.lyrics.running(now);
        self.background
            .activity(now)
            .and(self.logo.activity(now))
            .and(Activity { redraw: lyrics, settled: !lyrics, generation: 0, loading: false })
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
        let visible: Vec<(String, f32, bool)> = self.lyrics.visible().map(|(l, incoming)| (l.text.clone(), l.opacity.value(now), incoming)).collect();
        if !visible.is_empty() {
            let layouts: Vec<(Arc<TextLayout>, f32, bool)> =
                visible.iter().map(|(text, progress, incoming)| (self.layout(fonts, text, &geometry, style.caps), *progress, *incoming)).collect();
            let text_height = layouts.iter().map(|(l, ..)| l.height).fold(0.0, f32::max);
            let rect = geometry.text_box(style.vertical, text_height);
            list.fill(rect, style.box_color);
            list.border(rect, geometry.border, style.border_color);
            let (x, y) = (rect.x + geometry.border + geometry.padding_x, rect.y + geometry.border + geometry.padding_y);
            let transition = self.scene.transition;
            if transition.kind == TransitionKind::Slide {
                let inner = Rect::new(rect.x + geometry.border, rect.y + geometry.border, rect.w - 2.0 * geometry.border, rect.h - 2.0 * geometry.border);
                list.set_clip(Some(inner));
                for (laid, progress, incoming) in &layouts {
                    let (dx, dy) = slide_offset(&transition, *incoming, *progress, inner);
                    list.text_with_effects(laid, x + dx, y + dy, style.color, geometry.effects);
                }
                list.set_clip(None);
            } else {
                for (laid, opacity, _) in &layouts {
                    list.text_with_effects(laid, x, y, with_alpha(style.color, *opacity), geometry.effects.with_alpha(*opacity));
                }
            }
        }
        self.layouts.retain(|text, _| visible.iter().any(|(t, ..)| t == text));

        if self.scene.display == Display::Logo {
            if self.scene.logo.is_some() {
                self.logo.draw(&mut list, geometry.area, now);
            } else {
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

    pub(super) fn layout(&mut self, fonts: &Fonts, text: &str, geometry: &LyricGeometry, caps: bool) -> Arc<TextLayout> {
        if let Some(laid) = self.layouts.get(text) {
            return laid.clone();
        }
        let shown = if caps { text.to_uppercase() } else { text.to_string() };
        let laid = Arc::new(layout(fonts, &shown, &geometry.text_style, Some(geometry.text_width)));
        self.layouts.insert(text.to_string(), laid.clone());
        laid
    }
}
