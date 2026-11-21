


use super::*;

impl RenderEngine {
    pub fn resize(&self, role: OutputRole, width: u32, height: u32) {
        self.sizes.lock().expect("sizes poisoned").insert(role, (width, height));
        self.resize_preview(role);
        if let Some(worker) = self.outputs.get(&role) {
            worker.resize(width, height);
        }
    }

    /// Letterboxes `role`'s content to `aspect` (width, height); `None` fills
    /// the window.
    pub fn set_aspect(&self, role: OutputRole, aspect: Option<(u32, u32)>) {
        let mut aspects = self.aspects.lock().expect("aspects poisoned");
        match aspect {
            Some(aspect) => aspects.insert(role, aspect),
            None => aspects.remove(&role),
        };
        drop(aspects);
        self.resize_preview(role);
        if let Some(worker) = self.outputs.get(&role) {
            worker.set_aspect(aspect);
        }
    }

    /// Shows or parks `role`'s presentation, following its window.
    pub fn set_shown(&self, role: OutputRole, shown: bool) {
        if let Some(worker) = self.outputs.get(&role) {
            worker.set_shown(shown);
        }
    }

    /// Previews and monitors render the letterboxed content only.
    pub(super) fn resize_preview(&self, role: OutputRole) {
        if !self.sizes.lock().expect("sizes poisoned").contains_key(&role) {
            return;
        }
        let (width, height) = self.content_size(role);
        if let Some(monitor) = self.monitors.lock().expect("monitors poisoned").get(&role) {
            monitor.set_content_size((width, height));
        }
        if let Some(previews) = self.previews.lock().expect("previews poisoned").as_ref() {
            previews.resize(role, width, height);
        }
    }

    /// Presents `role` again, for windows whose contents the system discarded.
    pub fn redraw(&self, role: OutputRole) {
        if let Some(worker) = self.outputs.get(&role) {
            worker.redraw();
        }
    }

    pub fn stats(&self, role: OutputRole) -> Option<OutputStats> {
        self.outputs.get(&role).map(OutputWorker::stats)
    }
}

/// The centered (x, y, width, height) area of `size` with the `aspect`
/// ratio; the whole of `size` without one.
pub fn fit_aspect(size: (u32, u32), aspect: Option<(u32, u32)>) -> [u32; 4] {
    let (width, height) = size;
    let Some((aw, ah)) = aspect.filter(|&(w, h)| w > 0 && h > 0 && width > 0 && height > 0) else {
        return [0, 0, width, height];
    };
    let (fit_w, fit_h) = if width as u64 * ah as u64 > height as u64 * aw as u64 {
        ((height as u64 * aw as u64 / ah as u64) as u32, height)
    } else {
        (width, (width as u64 * ah as u64 / aw as u64) as u32)
    };
    let (fit_w, fit_h) = (fit_w.max(1), fit_h.max(1));
    [(width - fit_w) / 2, (height - fit_h) / 2, fit_w, fit_h]
}

pub(super) fn preview_target(role: OutputRole) -> PreviewTarget {
    match role {
        OutputRole::Projection => PreviewTarget::Projection,
        OutputRole::Confidence => PreviewTarget::Confidence,
    }
}

#[cfg(test)]
mod fit_tests {
    use super::fit_aspect;

    #[test]
    fn letterboxes_and_pillarboxes_to_the_aspect() {
        assert_eq!(fit_aspect((1920, 1080), None), [0, 0, 1920, 1080]);
        assert_eq!(fit_aspect((1920, 1080), Some((4, 3))), [240, 0, 1440, 1080]);
        assert_eq!(fit_aspect((1024, 768), Some((16, 9))), [0, 96, 1024, 576]);
        assert_eq!(fit_aspect((0, 0), Some((16, 9))), [0, 0, 0, 0]);
    }
}
