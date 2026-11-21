use std::collections::HashMap;
use std::sync::{Arc, Mutex};


use crate::paint::Rgba;
use crate::scene::{ConfidenceGroup, ConfidenceStyle};
use crate::text::{layout, Fonts, TextAlign, TextLayout, TextStyle};

use super::*;

pub(super) struct Row {
    pub(super) group: usize,
    pub(super) line: usize,
    /// Offset from the top of the scroll content.
    pub(super) top: f32,
    pub(super) height: f32,
    pub(super) code: Arc<TextLayout>,
    pub(super) text: Arc<TextLayout>,
}

pub(super) struct Section {
    pub(super) top: f32,
    pub(super) height: f32,
    pub(super) color: Rgba,
    pub(super) aside: Rgba,
    pub(super) label: Arc<TextLayout>,
}

pub(super) struct Card {
    pub(super) code: Arc<TextLayout>,
    pub(super) title: Arc<TextLayout>,
    pub(super) color: Rgba,
}

/// What a layout is built from: groups, lyric size, style and output size.
pub(super) type LayoutKey = (Vec<ConfidenceGroup>, f32, ConfidenceStyle, (u32, u32));

/// Everything shaped for one scene at one output size.
pub(super) struct Layout {
    pub(super) size: (u32, u32),
    /// What it was built from.
    pub(super) groups: Vec<ConfidenceGroup>,
    pub(super) scale: f32,
    pub(super) style: ConfidenceStyle,
    pub(super) rows: Vec<Row>,
    pub(super) sections: Vec<Section>,
    pub(super) content_height: f32,
    pub(super) cards: Vec<Card>,
    pub(super) waiting: Option<Arc<TextLayout>>,
}

/// Shaped text by content, kept across rebuilds so an edit reshapes only
/// what changed. Valid for one output width, lyric size and style.
#[derive(Default)]
pub(super) struct TextCache {
    pub(super) metrics: Option<(u32, u32, ConfidenceStyle)>,
    pub(super) entries: HashMap<(u8, String), Arc<TextLayout>>,
}

pub(super) fn build_layout(fonts: &Fonts, cache: &Mutex<TextCache>, groups: &[ConfidenceGroup], scale: f32, style: &ConfidenceStyle, size: (u32, u32)) -> Layout {
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
pub(super) fn ellipsize(fonts: &Fonts, text: &str, style: &TextStyle, width: f32) -> TextLayout {
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
