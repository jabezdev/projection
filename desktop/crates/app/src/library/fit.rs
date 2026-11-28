use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use protocol::model::split_slides;
use protocol::{LiveService, SessionItemKind};
use render_engine::{split_to_fit, Fonts, LyricStyle, SlideText};

const MEASURE_SIZE: (u32, u32) = (1920, 1080);
const CACHE_LIMIT: usize = 4000;

#[derive(Default)]
pub struct Fitter {
    fonts: OnceLock<Arc<Fonts>>,
    state: Mutex<(Option<LyricStyle>, HashMap<SlideText, Vec<SlideText>>)>,
}

impl Fitter {
    pub fn set_fonts(&self, fonts: Arc<Fonts>) {
        let _ = self.fonts.set(fonts);
    }

    pub fn set_style(&self, style: LyricStyle) -> bool {
        let mut state = self.state.lock().expect("fitter poisoned");
        if state.0.as_ref().is_some_and(|s| fit_fields(s) == fit_fields(&style)) {
            return false;
        }
        *state = (Some(style), HashMap::new());
        true
    }

    fn split(&self, slide: SlideText) -> Vec<SlideText> {
        let Some(fonts) = self.fonts.get() else { return vec![slide] };
        let mut state = self.state.lock().expect("fitter poisoned");
        let Some(style) = state.0.clone() else { return vec![slide] };
        if let Some(found) = state.1.get(&slide) {
            return found.clone();
        }
        let parts = split_to_fit(fonts, &style, MEASURE_SIZE, &slide);
        if state.1.len() > CACHE_LIMIT {
            state.1.clear();
        }
        state.1.insert(slide, parts.clone());
        parts
    }

    pub fn apply(&self, live: &mut LiveService) {
        if self.fonts.get().is_none() {
            return;
        }
        for item in &mut live.items {
            let SessionItemKind::Text { steps, parallel, captions } = &mut item.kind else { continue };
            if let Some(song) = item.song.as_mut() {
                for section in &mut song.sections {
                    let slides: Vec<String> = split_slides(&section.lyrics)
                        .into_iter()
                        .flat_map(|text| self.split(SlideText { text, ..Default::default() }))
                        .map(|s| s.text)
                        .collect();
                    section.lyrics = slides.join("\n\n");
                }
                *steps = song.sections.iter().flat_map(|s| split_slides(&s.lyrics)).collect();
                continue;
            }
            let at = |list: &[String], i: usize| list.get(i).cloned().unwrap_or_default();
            let mut fitted = (Vec::new(), Vec::new(), Vec::new());
            for (i, text) in steps.iter().enumerate() {
                for part in self.split(SlideText { text: text.clone(), parallel: at(parallel, i), caption: at(captions, i) }) {
                    fitted.0.push(part.text);
                    fitted.1.push(part.parallel);
                    fitted.2.push(part.caption);
                }
            }
            let had_parallel = !parallel.is_empty();
            let had_captions = !captions.is_empty();
            *steps = fitted.0;
            *parallel = if had_parallel { fitted.1 } else { Vec::new() };
            *captions = if had_captions { fitted.2 } else { Vec::new() };
        }
    }
}

fn fit_fields(style: &LyricStyle) -> (String, u32, u16, bool, bool, u32, u32, u32, u32, u32) {
    (
        style.font.clone(),
        style.size.to_bits(),
        style.weight,
        style.italic,
        style.caps,
        style.margin_x.to_bits(),
        style.margin_y.to_bits(),
        style.line_height.to_bits(),
        style.letter_spacing.to_bits(),
        style.border.to_bits(),
    )
}
