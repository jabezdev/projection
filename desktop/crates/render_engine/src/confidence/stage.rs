use std::sync::Arc;

use crate::paint::{DrawList, Rect};
use crate::scene::StageClock;
use crate::text::{layout, Fonts, TextAlign, TextStyle};

use super::*;

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

pub(super) fn timer_text(clock: &StageClock) -> String {
    format_ms(clock.remaining(now_ms()))
}

pub(super) fn format_ms(ms: i64) -> String {
    let over = ms < 0;
    let seconds = if over { (-ms + 999) / 1000 } else { (ms + 999) / 1000 };
    let (h, m, s) = (seconds / 3600, (seconds / 60) % 60, seconds % 60);
    let sign = if over { "+" } else { "" };
    if h > 0 { format!("{sign}{h}:{m:02}:{s:02}") } else { format!("{sign}{m}:{s:02}") }
}

impl ConfidenceRenderer {
    pub(super) fn draw_stage(&mut self, list: &mut DrawList, fonts: &Arc<Fonts>, m: &Metrics, size: (u32, u32)) {
        let stage = self.scene.stage.clone();
        if stage.is_empty() {
            return;
        }
        let top = m.footer_top();
        let band = Rect::new(0.0, top, m.w, m.footer);
        list.fill(band, rgb("#10171d"));
        list.fill(Rect::new(0.0, top, m.w, 1.0), rgb("#3d4d5b"));
        let timer_w = if stage.timer.is_some() { m.timer_width() } else { 0.0 };
        if let Some(clock) = stage.timer {
            let remaining = clock.remaining(now_ms());
            let text = format_ms(remaining);
            if self.timer.as_ref().is_none_or(|(t, w, _)| *t != text || *w != size.0) {
                let style = TextStyle { weight: 700, line_height: 0.0, align: TextAlign::Center, ..TextStyle::new(MONO_FONT, 7.0 * m.cq) };
                self.timer = Some((text.clone(), size.0, Arc::new(layout(fonts, &text, &style, Some(timer_w)))));
            }
            let (background, color) = if remaining <= 0 {
                (rgb("#7a1c1c"), rgb("#ffffff"))
            } else if remaining <= 120_000 {
                (rgb("#5a4210"), rgb("#ffd27a"))
            } else {
                (rgb("#172029"), if clock.ends_at_ms.is_some() { rgb("#ffffff") } else { rgb("#8e9ba7") })
            };
            let rect = Rect::new(m.w - timer_w, top + 1.0, timer_w, m.footer - 1.0);
            list.fill(rect, background);
            let (_, _, laid) = self.timer.as_ref().expect("timer laid out");
            list.text(laid, rect.x, rect.y + (rect.h - laid.height) / 2.0, color);
        } else {
            self.timer = None;
        }
        if stage.message.is_empty() {
            self.message = None;
            return;
        }
        let width = (m.w - timer_w - 4.0 * m.cq).max(1.0);
        let key = format!("{}\u{0}{}", stage.message, width as u32);
        if self.message.as_ref().is_none_or(|(t, w, _)| *t != key || *w != size.0) {
            let style = TextStyle { weight: 700, line_height: 4.6 * m.cq, ..TextStyle::new(UI_FONT, 3.8 * m.cq) };
            self.message = Some((key, size.0, Arc::new(super::layout::ellipsize(fonts, &stage.message, &style, width))));
        }
        let rect = Rect::new(0.0, top + 1.0, m.w - timer_w, m.footer - 1.0);
        list.fill(rect, rgb("#ffd23f"));
        let (_, _, laid) = self.message.as_ref().expect("message laid out");
        list.text(laid, 2.0 * m.cq, rect.y + (rect.h - laid.height) / 2.0, rgb("#121314"));
    }
}

#[cfg(test)]
mod tests {
    use super::format_ms;

    #[test]
    fn timers_count_down_then_show_overtime() {
        assert_eq!(format_ms(30 * 60_000), "30:00");
        assert_eq!(format_ms(61_500), "1:02");
        assert_eq!(format_ms(0), "0:00");
        assert_eq!(format_ms(-1_000), "+0:01");
        assert_eq!(format_ms(-65_000), "+1:05");
        assert_eq!(format_ms(3_600_000 + 5_000), "1:00:05");
    }
}
