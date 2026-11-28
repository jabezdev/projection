use crate::scene::ConfidenceStyle;

pub(super) struct Metrics {
    pub(super) cq: f32,
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) clock: bool,
    pub(super) tape: bool,
    pub(super) footer: f32,
}

impl Metrics {
    pub(super) fn new(size: (u32, u32), style: &ConfidenceStyle) -> Self {
        Self { cq: size.0 as f32 / 100.0, w: size.0 as f32, h: size.1 as f32, clock: style.clock, tape: style.tape, footer: 0.0 }
    }
    pub(super) fn with_stage(mut self, shown: bool) -> Self {
        self.footer = if shown { (10.0 * self.cq).max(36.0) } else { 0.0 };
        self
    }
    pub(super) fn footer_top(&self) -> f32 {
        self.h - self.footer
    }
    pub(super) fn timer_width(&self) -> f32 {
        28.0 * self.cq
    }
    pub(super) fn bar(&self) -> bool {
        self.clock || self.tape
    }
    pub(super) fn topbar(&self) -> f32 {
        if self.bar() { (6.0 * self.cq).max(20.0) } else { 0.0 }
    }
    pub(super) fn scroll_top(&self) -> f32 {
        if self.bar() { self.topbar() + 1.0 } else { 0.0 }
    }
    pub(super) fn scroll_height(&self) -> f32 {
        (self.h - self.scroll_top() - self.footer).max(0.0)
    }
    pub(super) fn time_width(&self) -> f32 {
        if self.clock { 13.0 * self.cq } else { 0.0 }
    }
    pub(super) fn tape_left(&self) -> f32 {
        if self.clock { self.time_width() + 1.0 } else { 0.0 }
    }
    pub(super) fn card_width(&self) -> f32 {
        18.0 * self.cq
    }
    pub(super) fn card_title_room(&self, code_width: f32) -> f32 {
        (self.card_width() - 2.0 * self.cq - 1.0 - code_width - self.cq).max(0.0)
    }
    pub(super) fn padding(&self) -> f32 {
        25.0 * self.cq
    }
    pub(super) fn aside(&self) -> f32 {
        6.0 * self.cq
    }
    pub(super) fn code_x(&self) -> f32 {
        self.aside() + 2.0 * self.cq
    }
    pub(super) fn text_x(&self) -> f32 {
        self.code_x() + 6.0 * self.cq + self.cq
    }
    pub(super) fn text_width(&self) -> f32 {
        (self.w - self.text_x() - 2.0 * self.cq).max(1.0)
    }
}
