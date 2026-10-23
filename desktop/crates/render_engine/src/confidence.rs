//! The confidence output: clock, arrangement tape and the scrolling
//! teleprompter, or the foreground video when its confidence option is on.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam::channel::{bounded, Receiver};

use crate::media::MediaTextures;
use crate::main_scene::MediaStage;
use crate::paint::{css_color, with_alpha, DrawList, Item, Rect, Rgba, TextEffects};
use crate::scene::{Activity, ConfidenceGroup, ConfidenceScene, ConfidenceStyle, MediaLayer, MediaSource};
use crate::text::{layout, Fonts, TextAlign, TextLayout, TextStyle};

const UI_FONT: &str = "Segoe UI";
const MONO_FONT: &str = "monospace";

fn rgb(hex: &str) -> Rgba {
    css_color(hex, [1.0, 0.0, 1.0, 1.0])
}

/// CSS `saturate()`.
fn saturate(c: Rgba, s: f32) -> Rgba {
    let [r, g, b, a] = c;
    [
        (0.213 + 0.787 * s) * r + (0.715 - 0.715 * s) * g + (0.072 - 0.072 * s) * b,
        (0.213 - 0.213 * s) * r + (0.715 + 0.285 * s) * g + (0.072 - 0.072 * s) * b,
        (0.213 - 0.213 * s) * r + (0.715 - 0.715 * s) * g + (0.072 + 0.928 * s) * b,
        a,
    ]
}

/// CSS `color-mix(in srgb, a p, b)`.
fn mix(a: Rgba, p: f32, b: Rgba) -> Rgba {
    [a[0] * p + b[0] * (1.0 - p), a[1] * p + b[1] * (1.0 - p), a[2] * p + b[2] * (1.0 - p), 1.0]
}

/// Browser-like smooth scrolling: eases from wherever it is to the target.
#[derive(Debug, Clone, Copy)]
struct Smooth {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

impl Smooth {
    fn at(value: f32, now: Instant) -> Self {
        Self { from: value, to: value, start: now, duration: Duration::ZERO }
    }

    fn value(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.start);
        if elapsed >= self.duration {
            return self.to;
        }
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        let eased = if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 };
        self.from + (self.to - self.from) * eased
    }

    fn scroll_to(&mut self, to: f32, now: Instant, animate: bool) {
        if (to - self.to).abs() < 0.5 {
            return;
        }
        let from = self.value(now);
        let distance = (to - from).abs();
        let duration = if animate { Duration::from_millis((250.0 + distance * 0.3).min(650.0) as u64) } else { Duration::ZERO };
        *self = Self { from, to, start: now, duration };
    }

    fn running(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) < self.duration
    }
}

struct Row {
    group: usize,
    line: usize,
    /// Offset from the top of the scroll content.
    top: f32,
    height: f32,
    code: Arc<TextLayout>,
    text: Arc<TextLayout>,
}

struct Section {
    top: f32,
    height: f32,
    color: Rgba,
    aside: Rgba,
    label: Arc<TextLayout>,
}

struct Card {
    code: Arc<TextLayout>,
    title: Arc<TextLayout>,
    color: Rgba,
}

/// What a layout is built from: groups, lyric size, style and output size.
type LayoutKey = (Vec<ConfidenceGroup>, f32, ConfidenceStyle, (u32, u32));

/// Everything shaped for one scene at one output size.
struct Layout {
    size: (u32, u32),
    /// What it was built from.
    groups: Vec<ConfidenceGroup>,
    scale: f32,
    style: ConfidenceStyle,
    rows: Vec<Row>,
    sections: Vec<Section>,
    content_height: f32,
    cards: Vec<Card>,
    waiting: Option<Arc<TextLayout>>,
}

/// Pixel metrics for one output width (`cq` = 1% of the width).
struct Metrics {
    cq: f32,
    w: f32,
    h: f32,
    clock: bool,
    tape: bool,
}

impl Metrics {
    fn new(size: (u32, u32), style: &ConfidenceStyle) -> Self {
        Self { cq: size.0 as f32 / 100.0, w: size.0 as f32, h: size.1 as f32, clock: style.clock, tape: style.tape }
    }
    fn bar(&self) -> bool {
        self.clock || self.tape
    }
    fn topbar(&self) -> f32 {
        if self.bar() { (6.0 * self.cq).max(20.0) } else { 0.0 }
    }
    /// Top of the scroll viewport, below the topbar's 1px border.
    fn scroll_top(&self) -> f32 {
        if self.bar() { self.topbar() + 1.0 } else { 0.0 }
    }
    fn scroll_height(&self) -> f32 {
        (self.h - self.scroll_top()).max(0.0)
    }
    fn time_width(&self) -> f32 {
        if self.clock { 13.0 * self.cq } else { 0.0 }
    }
    fn tape_left(&self) -> f32 {
        if self.clock { self.time_width() + 1.0 } else { 0.0 }
    }
    /// `flex: 0 0 18cqw` under the app's border-box sizing: the padding and
    /// right border are inside.
    fn card_width(&self) -> f32 {
        18.0 * self.cq
    }
    /// The title's room: the card less its padding, right border, code and gap.
    fn card_title_room(&self, code_width: f32) -> f32 {
        (self.card_width() - 2.0 * self.cq - 1.0 - code_width - self.cq).max(0.0)
    }
    fn padding(&self) -> f32 {
        25.0 * self.cq
    }
    fn aside(&self) -> f32 {
        6.0 * self.cq
    }
    fn code_x(&self) -> f32 {
        self.aside() + 2.0 * self.cq
    }
    fn text_x(&self) -> f32 {
        self.code_x() + 6.0 * self.cq + self.cq
    }
    fn text_width(&self) -> f32 {
        (self.w - self.text_x() - 2.0 * self.cq).max(1.0)
    }
}

/// Shaped text by content, kept across rebuilds so an edit reshapes only
/// what changed. Valid for one output width, lyric size and style.
#[derive(Default)]
struct TextCache {
    metrics: Option<(u32, u32, ConfidenceStyle)>,
    entries: HashMap<(u8, String), Arc<TextLayout>>,
}

pub struct ConfidenceRenderer {
    scene: ConfidenceScene,
    revision: u64,
    /// The layout on screen; replaced when a rebuild finishes.
    layout: Option<Arc<Layout>>,
    /// The active row within `layout`, which may lag the scene while a
    /// rebuild for new groups is in flight.
    active: Option<(usize, usize)>,
    building: Option<(LayoutKey, Receiver<Layout>)>,
    cache: Arc<Mutex<TextCache>>,
    video: MediaStage,
    scroll: Smooth,
    tape: Smooth,
    /// Scroll targets were computed for this revision and size.
    focused: Option<(u64, (u32, u32))>,
    clock: Option<(String, u32, Arc<TextLayout>)>,
}

impl Default for ConfidenceRenderer {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            scene: ConfidenceScene { size: 4.0, transition_ms: 250.0, ..Default::default() },
            revision: 0,
            layout: None,
            active: None,
            building: None,
            cache: Arc::default(),
            video: MediaStage::default(),
            scroll: Smooth::at(0.0, now),
            tape: Smooth::at(0.0, now),
            focused: None,
            clock: None,
        }
    }
}

fn clock_text() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

impl ConfidenceRenderer {
    pub fn set_scene(&mut self, scene: ConfidenceScene, media: &MediaTextures, now: Instant) {
        let video = scene.video.clone().map(|key| MediaLayer { source: MediaSource::Video(key), cover: false });
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
        })
    }

    /// Keeps the layout in step with the scene and size. The first layout is
    /// built in place; later ones on a worker while the old one stays up, so
    /// loading or editing a long service never stalls presentation.
    fn update_layout(&mut self, fonts: &Arc<Fonts>, size: (u32, u32)) {
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
        // Metrics follow the layout on screen, which may lag a style change.
        let m = Metrics::new(size, &laid.style);
        self.focus(&m, size, now);
        let scroll = self.scroll.value(now);
        let tape = self.tape.value(now);
        let laid = laid.as_ref();
        let active = self.active;
        let mut list = DrawList::new();
        let full = Rect::new(0.0, 0.0, m.w, m.h);
        list.fill(full, rgb("#06090b"));

        // Teleprompter.
        let viewport = Rect::new(0.0, m.scroll_top(), m.w, m.scroll_height());
        list.set_clip(Some(viewport));
        let origin = m.scroll_top() + m.padding() - scroll;
        if let Some(waiting) = &laid.waiting {
            list.text(waiting, 0.0, origin, rgb("#a8b7c4"));
        }
        // `top: 45%` is measured in the scrollport less its 20cq scroll-padding.
        let stick = m.scroll_top() + 20.0 * m.cq + 0.45 * (m.scroll_height() - 40.0 * m.cq);
        for section in &laid.sections {
            let rect = Rect::new(0.0, origin + section.top, m.aside(), section.height);
            if !rect.intersects(&viewport) {
                continue;
            }
            list.fill(rect, section.aside);
            // Rotated label, sticky at 45% of the viewport within its section.
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

        // Topbar: clock and tape.
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
            // Content is centered in the card's padding box above the colored edge.
            let center = (0.5 * m.cq + rect.h - 0.5 * m.cq - 0.3 * m.cq) / 2.0;
            list.text(&card.code, x + m.cq, center - card.code.height / 2.0, with_alpha(saturate(card.color, tone), alpha));
            let title_x = x + m.cq + card.code.width + m.cq;
            list.text(&card.title, title_x, center - card.title.height / 2.0, with_alpha([1.0, 1.0, 1.0, 1.0], alpha));
        }
        list.set_clip(None);

        // The foreground video covers everything once it can be drawn.
        let video_opacity = self.video.opacity(now);
        if video_opacity > 0.0 {
            list.fill(full, [0.0, 0.0, 0.0, video_opacity]);
            self.video.draw(&mut list, full, now);
        }
        list
    }

    /// Centers the active row and keeps its tape card in view: animated when
    /// the cue changes, immediately after a resize.
    fn focus(&mut self, m: &Metrics, size: (u32, u32), now: Instant) {
        // Wait for a layout of the current groups before moving.
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
        // Spacer after the cards lets the last ones scroll left.
        let strip = m.w - m.tape_left();
        let max_tape = (laid.cards.len() as f32 * m.card_width() + 70.0 * m.cq - strip).max(0.0);
        let tape = (group as f32 * m.card_width() - strip / 5.0).clamp(0.0, max_tape);
        self.tape.scroll_to(tape, now, true);
    }
}

fn build_layout(fonts: &Fonts, cache: &Mutex<TextCache>, groups: &[ConfidenceGroup], scale: f32, style: &ConfidenceStyle, size: (u32, u32)) -> Layout {
    let m = Metrics::new(size, style);
    let cq = m.cq;
    let mut cache = cache.lock().expect("text cache poisoned");
    let metrics = Some((size.0, scale.to_bits(), style.clone()));
    if cache.metrics != metrics {
        *cache = TextCache { metrics, entries: HashMap::new() };
    }
    let mut previous = std::mem::take(&mut cache.entries);
    let entries = &mut cache.entries;
    let mut shape = |kind: u8, text: &str, make: &dyn Fn() -> TextLayout| -> Arc<TextLayout> {
        let key = (kind, text.to_string());
        let laid = previous.remove(&key).or_else(|| entries.get(&key).cloned()).unwrap_or_else(|| Arc::new(make()));
        entries.insert(key, laid.clone());
        laid
    };
    let lyric = TextStyle { line_height: style.line_height * scale * cq, ..TextStyle::new(&style.font, scale * cq) };
    let code_style = TextStyle { line_height: 0.0, ..TextStyle::new(MONO_FONT, 1.3 * cq) };
    let label_style = TextStyle { weight: 750, line_height: 4.2 * cq, ..TextStyle::new(UI_FONT, 2.8 * cq) };
    let card_code = TextStyle { weight: 700, line_height: 3.3 * cq, ..TextStyle::new(UI_FONT, 2.2 * cq) };
    let card_title = TextStyle { line_height: 1.8 * cq, ..TextStyle::new(UI_FONT, 1.2 * cq) };
    let mut rows = Vec::new();
    let mut sections = Vec::new();
    let mut cards = Vec::new();
    let mut top = 0.0;
    for (index, group) in groups.iter().enumerate() {
        let section_top = top;
        for (line, text) in group.lines.iter().enumerate() {
            let code_text = format!("{}.{}", group.code, line + 1);
            let code = shape(0, &code_text, &|| layout(fonts, &code_text, &code_style, None));
            let shown = if style.caps { text.to_uppercase() } else { text.clone() };
            let laid = shape(1, &shown, &|| layout(fonts, &shown, &lyric, Some(m.text_width())));
            let height = 3.6 * cq + laid.height + 1.0;
            rows.push(Row { group: index, line, top, height, code, text: laid });
            top += height;
        }
        sections.push(Section {
            top: section_top,
            height: top - section_top,
            color: group.color,
            aside: mix(group.color, 0.26, rgb("#0a131c")),
            label: shape(2, &group.code, &|| layout(fonts, &group.code, &label_style, None)),
        });
        let code = shape(3, &group.code, &|| layout(fonts, &group.code, &card_code, None));
        let room = m.card_title_room(code.width);
        let title = shape(4, &format!("{room}\u{0}{}", group.title), &|| ellipsize(fonts, &group.title, &card_title, room));
        cards.push(Card { title, code, color: group.color });
    }
    let waiting = groups.is_empty().then(|| {
        let style = TextStyle { line_height: 6.0 * cq, align: TextAlign::Center, ..TextStyle::new(UI_FONT, 4.0 * cq) };
        Arc::new(layout(fonts, "Waiting for the next cue", &style, Some(m.w)))
    });
    Layout { size, groups: groups.to_vec(), scale, style: style.clone(), rows, sections, content_height: top, cards, waiting }
}

/// One line of `text`, cut with an ellipsis to fit `width`.
fn ellipsize(fonts: &Fonts, text: &str, style: &TextStyle, width: f32) -> TextLayout {
    let full = layout(fonts, text, style, None);
    if full.width <= width {
        return full;
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut low, mut high) = (0, chars.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + "…";
        if layout(fonts, &candidate, style, None).width <= width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    layout(fonts, &(chars[..low].iter().collect::<String>() + "…"), style, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_match_the_parity_reference_at_1080p() {
        // Values from the parity references in tools/parity.
        let m = Metrics::new((1920, 1080), &ConfidenceStyle::default());
        assert!((m.scroll_top() - 116.2).abs() < 0.05);
        assert!((m.tape_left() - 250.6).abs() < 0.05);
        assert!((m.card_width() - 345.6).abs() < 0.05);
        assert!((m.text_x() - 288.0).abs() < 0.1);
        assert!((m.text_width() - 1593.6).abs() < 0.1);
    }

    #[test]
    fn saturate_and_mix_follow_css() {
        let gray = saturate([1.0, 0.0, 0.0, 1.0], 0.0);
        assert!((gray[0] - 0.213).abs() < 1e-6 && (gray[1] - 0.213).abs() < 1e-6);
        assert_eq!(mix([1.0, 1.0, 1.0, 1.0], 0.25, [0.0, 0.0, 0.0, 1.0])[0], 0.25);
    }
}
