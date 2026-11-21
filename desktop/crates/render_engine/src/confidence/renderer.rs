use std::sync::Arc;
use std::time::Instant;

use crossbeam::channel::bounded;

use crate::media::MediaTextures;
use crate::paint::{with_alpha, DrawList, Item, Rect, TextEffects};
use crate::scene::{Activity, ConfidenceScene, MediaLayer, MediaSource};
use crate::text::{layout, Fonts, TextAlign, TextStyle};

use super::*;

pub(super) fn clock_text() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

impl ConfidenceRenderer {
    pub fn set_scene(&mut self, scene: ConfidenceScene, media: &MediaTextures, now: Instant) {
        let video = scene.video.clone().map(|key| MediaLayer { source: MediaSource::Video(key), cover: scene.video_cover });
        self.video.set(video.as_ref(), media, now, scene.transition_ms);
        if scene.groups != self.scene.groups || scene.size != self.scene.size || scene.active != self.scene.active || scene.style != self.scene.style {
            self.revision += 1;
        }
        self.scene = scene;
    }

    pub fn activity(&self, now: Instant) -> Activity {
        let scrolling = self.scroll.running(now) || self.tape.running(now);
        let clock_changed = self.scene.style.clock && self.clock.as_ref().is_none_or(|(text, ..)| *text != clock_text());
        let built = self.building.as_ref().is_some_and(|(_, done)| !done.is_empty());
        let building = self.building.is_some();
        self.video.activity(now).and(Activity {
            redraw: scrolling || clock_changed || built,
            settled: !scrolling && !building,
            generation: 0,
            loading: false,
        })
    }

    pub(super) fn update_layout(&mut self, fonts: &Arc<Fonts>, size: (u32, u32)) {
        let wanted: LayoutKey = (self.scene.groups.clone(), self.scene.size, self.scene.style.clone(), size);
        if let Some((key, done)) = &self.building {
            if *key != wanted {
                self.building = None;
            } else if let Ok(layout) = done.try_recv() {
                self.layout = Some(Arc::new(layout));
                self.building = None;
            }
        }
        let current = self.layout.as_ref().is_some_and(|l| l.groups == wanted.0 && l.scale == wanted.1 && l.style == wanted.2 && l.size == wanted.3);
        if current {
            self.active = self.scene.active;
            return;
        }
        if self.layout.is_none() {
            self.layout = Some(Arc::new(build_layout(fonts, &self.cache, &wanted.0, wanted.1, &wanted.2, size)));
            self.active = self.scene.active;
            return;
        }
        if self.building.is_none() {
            let (done_tx, done) = bounded(1);
            let (fonts, cache, (groups, scale, style, size)) = (fonts.clone(), self.cache.clone(), wanted.clone());
            let spawned = std::thread::Builder::new().name("confidence-layout".into()).spawn(move || {
                let _ = done_tx.send(build_layout(&fonts, &cache, &groups, scale, &style, size));
            });
            match spawned {
                Ok(_) => self.building = Some((wanted, done)),
                Err(e) => log::error!("Starting confidence layout: {e}"),
            }
        }
    }

    pub fn frame(&mut self, fonts: &Arc<Fonts>, size: (u32, u32), now: Instant) -> DrawList {
        self.video.update(now, self.scene.transition_ms);
        self.update_layout(fonts, size);
        let laid = self.layout.clone().expect("layout built");
        let m = Metrics::new(size, &laid.style);
        self.focus(&m, size, now);
        let scroll = self.scroll.value(now);
        let tape = self.tape.value(now);
        let laid = laid.as_ref();
        let active = self.active;
        let mut list = DrawList::new();
        let full = Rect::new(0.0, 0.0, m.w, m.h);
        list.fill(full, rgb("#06090b"));

        let viewport = Rect::new(0.0, m.scroll_top(), m.w, m.scroll_height());
        list.set_clip(Some(viewport));
        let origin = m.scroll_top() + m.padding() - scroll;
        if let Some(waiting) = &laid.waiting {
            list.text(waiting, 0.0, origin, rgb("#a8b7c4"));
        }
        let stick = m.scroll_top() + 20.0 * m.cq + 0.45 * (m.scroll_height() - 40.0 * m.cq);
        for section in &laid.sections {
            let rect = Rect::new(0.0, origin + section.top, m.aside(), section.height);
            if !rect.intersects(&viewport) {
                continue;
            }
            list.fill(rect, section.aside);
            let (label_w, label_h) = (section.label.height, section.label.width + 3.0 * m.cq);
            let y = stick.min(rect.bottom() - label_h).max(rect.y);
            let x = (m.aside() - label_w) / 2.0;
            list.push(Item::Text {
                layout: section.label.clone(),
                x: x.round(),
                y: (y + label_h - 1.5 * m.cq).round(),
                color: section.color,
                angle: -std::f32::consts::FRAC_PI_2,
                effects: TextEffects::default(),
            });
        }
        for row in &laid.rows {
            let rect = Rect::new(m.aside(), origin + row.top, m.w - m.aside(), row.height);
            if !rect.intersects(&viewport) {
                continue;
            }
            let is_active = active == Some((row.group, row.line));
            let is_past = active.is_some_and(|(g, l)| row.group < g || (row.group == g && row.line < l));
            if is_active {
                list.fill(Rect::new(rect.x, rect.y, rect.w, rect.h - 1.0), rgb("#17302f"));
                list.fill(Rect::new(rect.x, rect.y, 0.3 * m.cq, rect.h - 1.0), rgb("#47c6bd"));
            }
            let color = laid.sections[row.group].color;
            list.text(&row.code, m.code_x(), rect.y + 1.8 * m.cq + 0.7 * m.cq, color);
            let text_color = if is_active { rgb("#ffffff") } else if is_past { rgb("#45525d") } else { rgb("#8e9ba7") };
            list.text(&row.text, m.text_x(), rect.y + 1.8 * m.cq, text_color);
            list.fill(Rect::new(rect.x, rect.bottom() - 1.0, rect.w, 1.0), rgb("#1c2730"));
        }
        list.set_clip(None);

        if m.bar() {
            list.fill(Rect::new(0.0, 0.0, m.w, m.topbar()), rgb("#172029"));
            list.fill(Rect::new(0.0, m.topbar(), m.w, 1.0), rgb("#3d4d5b"));
        }
        if m.clock {
            list.fill(Rect::new(m.time_width(), 0.0, 1.0, m.topbar()), rgb("#42515e"));
            let now_text = clock_text();
            if self.clock.as_ref().is_none_or(|(text, width, _)| *text != now_text || *width != size.0) {
                let style = TextStyle { weight: 600, line_height: 0.0, align: TextAlign::Center, ..TextStyle::new(MONO_FONT, 3.4 * m.cq) };
                let clock = Arc::new(layout(fonts, &now_text, &style, Some(m.time_width())));
                self.clock = Some((now_text, size.0, clock));
            }
            let (_, _, clock) = self.clock.as_ref().expect("clock laid out");
            list.text(clock, 0.0, (m.topbar() - clock.height) / 2.0, rgb("#ffffff"));
        }
        let strip = if m.tape { Rect::new(m.tape_left(), 0.0, m.w - m.tape_left(), m.topbar()) } else { Rect::default() };
        list.set_clip(Some(strip));
        let active_group = active.map(|(g, _)| g);
        for (index, card) in laid.cards.iter().enumerate() {
            let x = m.tape_left() + index as f32 * m.card_width() - tape;
            let rect = Rect::new(x, 0.0, m.card_width(), m.topbar());
            if !rect.intersects(&strip) {
                continue;
            }
            let current = active_group == Some(index);
            let past = active_group.is_some_and(|g| index < g);
            let (alpha, tone) = if past { (0.45, 0.1) } else { (1.0, 1.0) };
            if current {
                list.fill(Rect::new(rect.x, rect.y, rect.w - 1.0, rect.h), rgb("#28403f"));
                list.border(Rect::new(rect.x, rect.y, rect.w - 1.0, rect.h), 0.15 * m.cq, rgb("#47c6bd"));
            }
            list.fill(Rect::new(rect.right() - 1.0, 0.0, 1.0, rect.h), with_alpha(saturate(rgb("#3c4a57"), tone), alpha));
            list.fill(Rect::new(rect.x, rect.h - 0.3 * m.cq, rect.w - 1.0, 0.3 * m.cq), with_alpha(saturate(card.color, tone), alpha));
            let center = (0.5 * m.cq + rect.h - 0.5 * m.cq - 0.3 * m.cq) / 2.0;
            list.text(&card.code, x + m.cq, center - card.code.height / 2.0, with_alpha(saturate(card.color, tone), alpha));
            let title_x = x + m.cq + card.code.width + m.cq;
            list.text(&card.title, title_x, center - card.title.height / 2.0, with_alpha([1.0, 1.0, 1.0, 1.0], alpha));
        }
        list.set_clip(None);

        let video_opacity = self.video.opacity(now);
        if video_opacity > 0.0 {
            list.fill(full, [0.0, 0.0, 0.0, video_opacity]);
            self.video.draw(&mut list, full, now);
        }
        list
    }

    pub(super) fn focus(&mut self, m: &Metrics, size: (u32, u32), now: Instant) {
        let laid = self.layout.clone().expect("layout built");
        if laid.groups != self.scene.groups || laid.style != self.scene.style || laid.size != size || self.focused == Some((self.revision, size)) {
            return;
        }
        let animate = self.focused.is_some_and(|(_, s)| s == size);
        self.focused = Some((self.revision, size));
        let Some((group, line)) = self.scene.active else { return };
        if let Some(row) = laid.rows.iter().find(|r| r.group == group && r.line == line) {
            let max = (2.0 * m.padding() + laid.content_height - m.scroll_height()).max(0.0);
            let target = (m.padding() + row.top - (m.scroll_height() - row.height) / 2.0).clamp(0.0, max);
            self.scroll.scroll_to(target, now, animate);
        }
        let strip = m.w - m.tape_left();
        let max_tape = (laid.cards.len() as f32 * m.card_width() + 70.0 * m.cq - strip).max(0.0);
        let tape = (group as f32 * m.card_width() - strip / 5.0).clamp(0.0, max_tape);
        self.tape.scroll_to(tape, now, true);
    }
}
