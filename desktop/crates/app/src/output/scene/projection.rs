use protocol::{
    BackgroundFit, BackgroundKind, DisplayMode, OutputConfig, PlaybackState, ProgramState, ProjectModel, SessionItemKind,
};
use render_engine::{
    Display, MainScene, MediaLayer, MediaSource, VideoKey,
};

use super::*;

pub fn main_scene(config: &OutputConfig, program: &ProgramState, project: Option<&ProjectModel>, playback: Option<&PlaybackState>) -> MainScene {
    let item = active_item(program, project);
    let item_is_video = matches!(item.map(|i| &i.kind), Some(SessionItemKind::Video { .. }));
    let fit_cover = config.background_fit == BackgroundFit::Cover;
    let capture = capture_on(item, false).map(|(key, cover)| MediaLayer { source: MediaSource::Video(key), cover });
    let background = capture.or_else(|| program.background.as_ref().and_then(|bg| match bg.kind {
        BackgroundKind::Image => Some(MediaLayer { source: MediaSource::Image(bg.path.clone()), cover: fit_cover }),
        BackgroundKind::Video if item_is_video => {
            foreground_video(item, playback).map(|key| MediaLayer { source: MediaSource::Video(key), cover: fit_cover })
        }
        BackgroundKind::Video => Some(MediaLayer { source: MediaSource::Video(VideoKey::Ambient(bg.path.clone())), cover: true }),
        BackgroundKind::Shader => Some(MediaLayer { source: MediaSource::Video(shader_key(config, &bg.path)), cover: true }),
    }));
    let logo = Some(config.logo_path.trim()).filter(|p| !p.is_empty()).map(|path| {
        if is_video(path) {
            MediaSource::Video(VideoKey::Ambient(path.to_string()))
        } else {
            MediaSource::Image(path.to_string())
        }
    });
    MainScene {
        text: program.active_text.clone(),
        display: match program.display {
            DisplayMode::Live => Display::Live,
            DisplayMode::Clear => Display::Clear,
            DisplayMode::Black => Display::Black,
            DisplayMode::Logo => Display::Logo,
        },
        background,
        logo,
        style: lyric_style(config),
        transition_ms: config.transition as f32,
        transition: transition(config),
    }
}

pub fn style_sample_scene(config: &OutputConfig) -> MainScene {
    let program = ProgramState { active_text: "Your love never fails\nYour mercy remains".into(), ..ProgramState::default() };
    MainScene { logo: None, ..main_scene(config, &program, None, None) }
}
