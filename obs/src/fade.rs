#[derive(Debug, Clone, PartialEq)]
pub struct Fader {
    pub shown: String,
    pub opacity: f32,
    pending: Option<String>,
}

impl Default for Fader {
    fn default() -> Self {
        Self { shown: String::new(), opacity: 1.0, pending: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changed {
    pub text: bool,
    pub opacity: bool,
}

impl Fader {
    pub fn tick(&mut self, target: &str, seconds: f32, duration: f32) -> Changed {
        let before = self.opacity;
        let mut changed = Changed::default();
        if target == self.shown {
            self.pending = None;
        } else if self.pending.as_deref() != Some(target) {
            self.pending = Some(target.to_string());
        }
        if duration <= 0.0 {
            if let Some(next) = self.pending.take() {
                self.shown = next;
                changed.text = true;
            }
            self.opacity = 1.0;
        } else {
            let step = seconds / duration;
            if self.pending.is_some() && !self.shown.is_empty() {
                self.opacity = (self.opacity - step).max(0.0);
            }
            if self.pending.is_some() && (self.shown.is_empty() || self.opacity <= 0.001) {
                self.shown = self.pending.take().unwrap_or_default();
                self.opacity = 0.0;
                changed.text = true;
            } else if self.pending.is_none() {
                self.opacity = (self.opacity + step).min(1.0);
            }
        }
        changed.opacity = self.opacity != before;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(fader: &mut Fader, target: &str, frames: usize) {
        for _ in 0..frames {
            fader.tick(target, 0.1, 0.3);
        }
    }

    #[test]
    fn new_text_fades_out_then_in() {
        let mut fader = Fader::default();
        assert!(fader.tick("One", 0.1, 0.3).text, "empty text swaps at once");
        run(&mut fader, "One", 5);
        assert_eq!((fader.shown.as_str(), fader.opacity), ("One", 1.0));
        fader.tick("Two", 0.1, 0.3);
        assert_eq!(fader.shown, "One");
        assert!(fader.opacity < 1.0);
        run(&mut fader, "Two", 3);
        assert_eq!(fader.shown, "Two");
        run(&mut fader, "Two", 4);
        assert_eq!(fader.opacity, 1.0);
    }

    #[test]
    fn going_back_cancels_the_swap() {
        let mut fader = Fader { shown: "One".into(), ..Fader::default() };
        fader.tick("Two", 0.1, 0.3);
        run(&mut fader, "One", 5);
        assert_eq!((fader.shown.as_str(), fader.opacity), ("One", 1.0));
    }

    #[test]
    fn no_fade_swaps_at_once() {
        let mut fader = Fader { shown: "One".into(), ..Fader::default() };
        let changed = fader.tick("", 0.016, 0.0);
        assert_eq!((changed.text, fader.shown.as_str(), fader.opacity), (true, "", 1.0));
    }
}
