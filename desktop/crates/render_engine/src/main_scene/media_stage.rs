use std::sync::Arc;
use std::time::Instant;

use crate::frame::SharedFrame;
use crate::media::{ImageSlot, MediaTextures};
use crate::paint::{fit, DrawList, Rect};
use crate::scene::{millis, Activity, Fade, MediaLayer, MediaSource, Transition, TransitionKind};


pub(super) enum Handle {
    Video(Arc<SharedFrame>),
    Image(Arc<ImageSlot>),
}

impl Handle {
    pub(super) fn open(source: &MediaSource, media: &MediaTextures) -> Self {
        match source {
            MediaSource::Video(key) => Handle::Video(media.video(key)),
            MediaSource::Image(path) => Handle::Image(media.image(path)),
        }
    }

    pub(super) fn ready(&self) -> bool {
        match self {
            Handle::Video(frame) => frame.size().is_some(),
            Handle::Image(slot) => slot.ready().is_some(),
        }
    }

    pub(super) fn generation(&self) -> u64 {
        match self {
            Handle::Video(frame) => frame.generation(),
            Handle::Image(_) => 0,
        }
    }

    pub(super) fn draw(&self, list: &mut DrawList, area: Rect, cover: bool, opacity: f32) {
        if area.w <= 0.0 || area.h <= 0.0 {
            return;
        }
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

pub(super) struct Layer {
    pub(super) source: MediaSource,
    pub(super) cover: bool,
    pub(super) handle: Handle,
    pub(super) opacity: Fade,
}

/// A new source comes in (fading, cut or sliding) only once it can be
/// drawn, while the previous one stays until then and goes out.
#[derive(Default)]
pub(crate) struct MediaStage {
    pub(super) target: Option<MediaSource>,
    pub(super) shown: Option<MediaSource>,
    pub(super) layers: Vec<Layer>,
    pub(super) remove_at: Option<Instant>,
    pub(super) transition: Transition,
}

/// How long a transition of `kind` lasts.
pub(super) fn duration(kind: TransitionKind, fade_ms: f32) -> f32 {
    if kind == TransitionKind::Cut { 0.0 } else { fade_ms }
}

/// Where a sliding layer is at `progress` (0 out of view, 1 in place): the
/// incoming one arrives from behind the movement, the outgoing one leaves ahead.
pub(super) fn slide_offset(transition: &Transition, incoming: bool, progress: f32, area: Rect) -> (f32, f32) {
    let (vx, vy) = transition.direction.vector();
    let away = 1.0 - progress;
    let sign = if incoming { -1.0 } else { 1.0 };
    (sign * vx * area.w * away, sign * vy * area.h * away)
}

impl MediaStage {
    pub fn set_transition(&mut self, transition: Transition) {
        self.transition = transition;
    }

    pub fn set(&mut self, layer: Option<&MediaLayer>, media: &MediaTextures, now: Instant, fade_ms: f32) {
        let fade_ms = duration(self.transition.kind, fade_ms);
        let easing = self.transition.easing;
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
                l.opacity.retarget_eased(0.0, now, millis(fade_ms), easing);
            }
            self.remove_at = Some(now + millis(fade_ms));
        }
        self.target = key;
    }

    /// Advances readiness and removes faded layers.
    pub fn update(&mut self, now: Instant, fade_ms: f32) {
        let fade_ms = duration(self.transition.kind, fade_ms);
        if let Some(target) = self.target.clone() {
            let ready = self.layers.iter().any(|l| l.source == target && l.handle.ready());
            if self.shown.as_ref() != Some(&target) && ready {
                for l in &mut self.layers {
                    l.opacity.retarget_eased(if l.source == target { 1.0 } else { 0.0 }, now, millis(fade_ms), self.transition.easing);
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
        let sliding = self.transition.kind == TransitionKind::Slide;
        if sliding {
            list.set_clip(Some(area));
        }
        for layer in &self.layers {
            let progress = layer.opacity.value(now);
            if progress <= 0.0 {
                continue;
            }
            if sliding {
                let incoming = layer.opacity.target() > 0.0;
                let (dx, dy) = slide_offset(&self.transition, incoming, progress, area);
                layer.handle.draw(list, Rect::new(area.x + dx, area.y + dy, area.w, area.h), layer.cover, 1.0);
            } else {
                layer.handle.draw(list, area, layer.cover, progress);
            }
        }
        if sliding {
            list.set_clip(None);
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
        Activity { redraw: fading || pending || video, settled: !(fading || pending), generation: generations, loading: pending }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Highest layer opacity, for a backing drawn behind letterboxed media.
    pub fn opacity(&self, now: Instant) -> f32 {
        self.layers.iter().filter(|l| l.handle.ready()).map(|l| l.opacity.value(now)).fold(0.0, f32::max)
    }
}
