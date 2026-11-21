use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Context;


use super::*;

impl RenderEngine {
    /// Starts rendering still previews of every output's scene, delivered to
    /// `sink` once each new scene settles.
    pub fn start_previews(&self, sink: SnapshotSink) -> anyhow::Result<()> {
        let aspects = self.aspects.lock().expect("aspects poisoned").clone();
        let sizes = self.sizes.lock().expect("sizes poisoned").iter()
            .map(|(&role, &size)| {
                let [_, _, width, height] = fit_aspect(size, aspects.get(&role).copied());
                (role, (width, height))
            })
            .collect();
        let previews = Previews::start(self.resources.clone(), sizes, sink)?;
        for (role, scene) in self.scenes.lock().expect("scenes poisoned").iter() {
            previews.set_scene(preview_target(*role), scene.clone());
        }
        *self.previews.lock().expect("previews poisoned") = Some(previews);
        Ok(())
    }

    /// Reports when a shown output waits for new media and when it shows it.
    /// Set once.
    pub fn on_status(&self, sink: StatusSink) {
        let _ = self.resources.status.set(sink);
    }

    /// Starts a live monitor of `spec.role` presenting into `spec.target`,
    /// parked until [`RenderEngine::set_monitor`] gives it a size. Call on the
    /// thread that owns the window, like [`RenderEngine::start`].
    pub fn add_monitor(&self, spec: MonitorSpec) -> anyhow::Result<()> {
        let role = spec.role;
        let surface = self
            .gpu
            .instance
            .create_surface(wgpu::SurfaceTarget::DisplayAndWindow(spec.target))
            .with_context(|| format!("Creating the {} monitor surface", role.label()))?;
        anyhow::ensure!(self.gpu.adapter.is_surface_supported(&surface), "The operator display is not supported by the output GPU");
        let scene = self.scenes.lock().expect("scenes poisoned").get(&role).cloned();
        let worker = MonitorWorker::spawn(self.resources.clone(), role, surface, self.content_size(role), scene)?;
        if let Some(old) = self.monitors.lock().expect("monitors poisoned").insert(role, worker) {
            old.signal_stop();
            old.join();
        }
        Ok(())
    }

    /// Shows `role`'s monitor at `size` physical pixels, or parks it.
    pub fn set_monitor(&self, role: OutputRole, size: Option<(u32, u32)>) {
        if let Some(monitor) = self.monitors.lock().expect("monitors poisoned").get(&role) {
            monitor.set_surface(size);
        }
    }

    pub fn has_monitor(&self, role: OutputRole) -> bool {
        self.monitors.lock().expect("monitors poisoned").contains_key(&role)
    }

    /// `role`'s letterboxed content size.
    pub(super) fn content_size(&self, role: OutputRole) -> (u32, u32) {
        let size = self.sizes.lock().expect("sizes poisoned").get(&role).copied().unwrap_or((1920, 1080));
        let aspect = self.aspects.lock().expect("aspects poisoned").get(&role).copied();
        let [_, _, width, height] = fit_aspect(size, aspect);
        (width, height)
    }

    /// Previews a scene that no output shows: a style sample of the
    /// projection (main scene) or the confidence screen.
    pub fn preview_style(&self, scene: Scene) {
        let target = match scene {
            Scene::Main(_) => PreviewTarget::Style,
            Scene::Confidence(_) => PreviewTarget::ConfidenceStyle,
        };
        if let Some(previews) = self.previews.lock().expect("previews poisoned").as_ref() {
            previews.set_scene(target, scene);
        }
    }

    pub fn gpu(&self) -> &Arc<Gpu> {
        &self.gpu
    }

    /// Replaces what `role` shows. Textures no current scene uses are
    /// released (outputs keep their own until fades end).
    pub fn set_scene(&self, role: OutputRole, scene: Scene) {
        let mut scenes = self.scenes.lock().expect("scenes poisoned");
        scenes.insert(role, scene.clone());
        self.retain_media(&scenes);
        drop(scenes);
        if let Some(previews) = self.previews.lock().expect("previews poisoned").as_ref() {
            previews.set_scene(preview_target(role), scene.clone());
        }
        if let Some(monitor) = self.monitors.lock().expect("monitors poisoned").get(&role) {
            monitor.set_scene(scene.clone());
        }
        if let Some(worker) = self.outputs.get(&role) {
            worker.set_scene(scene);
        }
    }

    /// Loads `images` ahead of their cues (the next and previous slides), so
    /// they appear without a decode delay. Replaces the previous set.
    pub fn preload(&self, images: &[String]) {
        let wanted: HashSet<String> = images.iter().cloned().collect();
        let mut preloads = self.preloads.lock().expect("preloads poisoned");
        if *preloads == wanted {
            return;
        }
        for path in &wanted {
            self.media.image(path);
        }
        *preloads = wanted;
        drop(preloads);
        let scenes = self.scenes.lock().expect("scenes poisoned");
        self.retain_media(&scenes);
    }

    /// Keeps the textures the scenes use plus the preloaded stills.
    pub(super) fn retain_media(&self, scenes: &HashMap<OutputRole, Scene>) {
        let (mut videos, mut images) = (HashSet::new(), self.preloads.lock().expect("preloads poisoned").clone());
        for scene in scenes.values() {
            for source in scene_sources(scene) {
                match source {
                    MediaSource::Video(key) => videos.insert(key),
                    MediaSource::Image(path) => images.insert(path),
                };
            }
        }
        self.media.retain(&videos, &images);
    }
}

/// Media a scene draws.
pub(super) fn scene_sources(scene: &Scene) -> Vec<MediaSource> {
    match scene {
        Scene::Main(main) => main
            .background
            .iter()
            .map(|layer| layer.source.clone())
            .chain(main.logo.clone().filter(|_| main.display == Display::Logo))
            .collect(),
        Scene::Confidence(confidence) => confidence.video.iter().map(|key| MediaSource::Video(key.clone())).collect(),
    }
}
