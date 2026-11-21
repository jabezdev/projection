

use crate::scene::ConfidenceStyle;


/// Pixel metrics for one output width (`cq` = 1% of the width).
pub(super) struct Metrics {
    pub(super) cq: f32,
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) clock: bool,
    pub(super) tape: bool,
}

impl Metrics {
    pub(super) fn new(size: (u32, u32), style: &ConfidenceStyle) -> Self {
        Self { cq: size.0 as f32 / 100.0, w: size.0 as f32, h: size.1 as f32, clock: style.clock, tape: style.tape }
    }
    pub(super) fn bar(&self) -> bool {
        self.clock || self.tape
    }
    pub(super) fn topbar(&self) -> f32 {
        if self.bar() { (6.0 * self.cq).max(20.0) } else { 0.0 }
    }
    /// Top of the scroll viewport, below the topbar's 1px border.
    pub(super) fn scroll_top(&self) -> f32 {
        if self.bar() { self.topbar() + 1.0 } else { 0.0 }
    }
    pub(super) fn scroll_height(&self) -> f32 {
        (self.h - self.scroll_top()).max(0.0)
    }
    pub(super) fn time_width(&self) -> f32 {
        if self.clock { 13.0 * self.cq } else { 0.0 }
    }
    pub(super) fn tape_left(&self) -> f32 {
        if self.clock { self.time_width() + 1.0 } else { 0.0 }
    }
    /// `flex: 0 0 18cqw` under the app's border-box sizing: the padding and
    /// right border are inside.
    pub(super) fn card_width(&self) -> f32 {
        18.0 * self.cq
    }
    /// The title's room: the card less its padding, right border, code and gap.
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
