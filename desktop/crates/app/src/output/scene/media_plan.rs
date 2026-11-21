
use protocol::{
    CaptureSource, OutputConfig, PlaybackState, ProgramState, ProjectModel,
    SessionItem, SessionItemKind,
};
use render_engine::{
    Display, MainScene, MediaLayer, MediaSource, VideoKey,
};

use super::*;

/// Ambient videos the scene needs decoded: a song's background video, and
/// the logo video while the logo is up.
pub fn ambient_videos(scene: &MainScene) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(MediaLayer { source: MediaSource::Video(VideoKey::Ambient(path)), .. }) = &scene.background {
        paths.push(path.clone());
    }
    if let (Display::Logo, Some(MediaSource::Video(VideoKey::Ambient(path)))) = (scene.display, &scene.logo) {
        if !paths.contains(path) {
            paths.push(path.clone());
        }
    }
    paths
}

/// What the outputs need decoded or loaded besides the scenes themselves:
/// ambient loops that play now, and what to prepare for the cues on either
/// side of the current one so they appear without a decode delay.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaPlan {
    pub ambient: Vec<String>,
    /// Ambient loops opened paused on their first frame: neighboring song
    /// background videos and the logo video.
    pub prepared_ambient: Vec<String>,
    /// The next (or else previous) video or audio item, opened paused.
    pub foreground: Option<String>,
    /// Stills of neighboring items, slides either side, the logo image.
    pub images: Vec<String>,
    /// Screens or windows being captured for the current item.
    pub captures: Vec<CaptureSource>,
}

/// Plans `main`'s media and what the neighboring cues need.
pub fn media_plan(config: &OutputConfig, main: &MainScene, program: &ProgramState, project: Option<&ProjectModel>, playback: Option<&PlaybackState>) -> MediaPlan {
    let mut plan = MediaPlan { ambient: ambient_videos(main), ..Default::default() };
    let add = |list: &mut Vec<String>, path: &str| {
        if !path.is_empty() && !list.iter().any(|p| p == path) {
            list.push(path.to_string());
        }
    };
    let logo = config.logo_path.trim();
    if is_video(logo) {
        add(&mut plan.prepared_ambient, logo);
    } else {
        add(&mut plan.images, logo);
    }
    let items = project.map(|p| p.session.items.as_slice()).unwrap_or_default();
    let current = program.active_item_id.and_then(|id| items.iter().position(|item| item.id == id));
    if let Some(SessionItemKind::Capture { source, options }) = current.map(|i| &items[i].kind) {
        if options.projection || options.confidence {
            plan.captures.push(source.clone());
        }
    }
    // A presentation's slides either side of the current one.
    if let Some(SessionItemKind::Presentation { slides, .. }) = current.map(|i| &items[i].kind) {
        let step = program.active_step_index.unwrap_or(0) as usize;
        for index in [step.wrapping_sub(1), step + 1] {
            if let Some(slide) = slides.get(index) {
                add(&mut plan.images, slide);
            }
        }
    }
    // Neighbours skip section headings, which never go on air.
    let cueable = |item: &&SessionItem| item.kind.is_cueable();
    let neighbors = match current {
        Some(i) => vec![items.iter().skip(i + 1).find(cueable), items.iter().take(i).rev().find(cueable)],
        None => vec![items.iter().find(cueable)],
    };
    for item in neighbors.into_iter().flatten() {
        match &item.kind {
            SessionItemKind::Image { path } => add(&mut plan.images, path),
            SessionItemKind::Presentation { path, slides } => add(&mut plan.images, slides.first().unwrap_or(path)),
            SessionItemKind::Video { path, .. } | SessionItemKind::Audio { path, .. } => {
                // The next item wins; the current session needs no preparing.
                if plan.foreground.is_none() && playback.is_none_or(|p| p.path != *path) {
                    plan.foreground = Some(path.clone());
                }
            }
            SessionItemKind::Text { .. } | SessionItemKind::Capture { .. } | SessionItemKind::Section {} => {}
        }
        if let Some(background) = item.background.as_deref().filter(|b| !b.starts_with(protocol::SHADER_PREFIX)) {
            if is_video(background) {
                add(&mut plan.prepared_ambient, background);
            } else {
                add(&mut plan.images, background);
            }
        }
    }
    plan.prepared_ambient.retain(|path| !plan.ambient.contains(path));
    plan
}
