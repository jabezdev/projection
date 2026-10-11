//! Builds what each native output shows from the engine's state and the
//! saved output style, and keeps the renderer and ambient videos in step.

use std::sync::{Arc, RwLock};

use core_engine::song::Song;
use crossbeam::channel::{unbounded, Receiver, Sender};
use protocol::{
    BackgroundFit, BackgroundKind, DisplayMode, EngineSnapshot, OutputConfig, PlaybackState, ProgramState, ProjectModel,
    SessionItem, SessionItemKind, TextAlign as ConfigAlign, VerticalAlign,
};
use render_engine::{
    css_color, ConfidenceGroup, ConfidenceScene, Display, LyricStyle, MainScene, MediaLayer, MediaSource, OutputRole,
    Scene, TextAlign, Vertical, VideoKey,
};

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "mkv", "webm", "avi"];
const NON_SONG_COLOR: &str = "#7d9baf";

fn is_video(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.iter().any(|v| v.eq_ignore_ascii_case(e)))
}

fn active_item<'a>(program: &ProgramState, project: Option<&'a ProjectModel>) -> Option<&'a SessionItem> {
    let id = program.active_item_id?;
    project?.session.items.iter().find(|item| item.id == id)
}

/// The foreground session showing `item`, if it is a video being played.
fn foreground_video(item: Option<&SessionItem>, playback: Option<&PlaybackState>) -> Option<VideoKey> {
    let item = item?;
    let playback = playback?;
    match &item.kind {
        SessionItemKind::Video { path, .. } if playback.item_id == item.id && playback.path == *path => {
            Some(VideoKey::Session(playback.session_id))
        }
        _ => None,
    }
}

pub fn lyric_style(config: &OutputConfig) -> LyricStyle {
    let mut box_color = css_color(&config.box_color, [0.0, 0.0, 0.0, 1.0]);
    box_color[3] *= (config.box_opacity as f32 / 100.0).clamp(0.0, 1.0);
    LyricStyle {
        font: config.font.clone(),
        size: config.size as f32,
        weight: config.weight.clamp(1.0, 1000.0) as u16,
        italic: config.italic,
        caps: config.caps,
        color: css_color(&config.color, [1.0, 1.0, 1.0, 1.0]),
        align: match config.align {
            ConfigAlign::Left => TextAlign::Left,
            ConfigAlign::Center => TextAlign::Center,
            ConfigAlign::Right => TextAlign::Right,
        },
        vertical: match config.vertical {
            VerticalAlign::Top => Vertical::Top,
            VerticalAlign::Center => Vertical::Center,
            VerticalAlign::Bottom => Vertical::Bottom,
        },
        margin_x: config.margin_x as f32,
        margin_y: config.margin_y as f32,
        line_height: config.line_height as f32,
        letter_spacing: config.letter_spacing as f32,
        border: config.border as f32,
        border_color: css_color(&config.border_color, [1.0, 1.0, 1.0, 1.0]),
        box_color,
    }
}

pub fn main_scene(config: &OutputConfig, program: &ProgramState, project: Option<&ProjectModel>, playback: Option<&PlaybackState>) -> MainScene {
    let item = active_item(program, project);
    let item_is_video = matches!(item.map(|i| &i.kind), Some(SessionItemKind::Video { .. }));
    let fit_cover = config.background_fit == BackgroundFit::Cover;
    let background = program.background.as_ref().and_then(|bg| match bg.kind {
        BackgroundKind::Image => Some(MediaLayer { source: MediaSource::Image(bg.path.clone()), cover: fit_cover }),
        // The video item itself plays in the foreground session.
        BackgroundKind::Video if item_is_video => {
            foreground_video(item, playback).map(|key| MediaLayer { source: MediaSource::Video(key), cover: fit_cover })
        }
        // A song's background video loops muted and always fills the screen.
        BackgroundKind::Video => Some(MediaLayer { source: MediaSource::Video(VideoKey::Ambient(bg.path.clone())), cover: true }),
    });
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
    }
}

/// The lyric style alone, for the output settings dialog.
pub fn style_sample_scene(config: &OutputConfig) -> MainScene {
    let program = ProgramState { active_text: "Your love never fails\nYour mercy remains".into(), ..ProgramState::default() };
    MainScene { logo: None, ..main_scene(config, &program, None, None) }
}

/// Teleprompter groups: one per song part occurrence, one per other item.
pub fn confidence_groups(project: Option<&ProjectModel>) -> Vec<(i64, usize, ConfidenceGroup)> {
    let mut groups = Vec::new();
    for item in project.map(|p| p.session.items.as_slice()).unwrap_or_default() {
        let title = item.title.clone().filter(|t| !t.is_empty());
        let (kind, steps) = match &item.kind {
            SessionItemKind::Text { steps } => ("text", steps),
            SessionItemKind::Image { .. } => ("image", &Vec::new()),
            SessionItemKind::Video { .. } => ("video", &Vec::new()),
            SessionItemKind::Audio { .. } => ("audio", &Vec::new()),
            SessionItemKind::Presentation { .. } => ("presentation", &Vec::new()),
        };
        if kind != "text" {
            let label = title.clone().unwrap_or_else(|| kind.to_string());
            groups.push((
                item.id,
                0,
                ConfidenceGroup {
                    key: item.id.to_string(),
                    code: if kind == "presentation" { "Slides".into() } else { kind.into() },
                    title: label.clone(),
                    color: css_color(NON_SONG_COLOR, [1.0; 4]),
                    lines: vec![label],
                },
            ));
            continue;
        }
        let song = Song::decode(item.song.as_deref().unwrap_or(&steps.join("\n\n")));
        for occurrence in song.occurrences() {
            groups.push((
                item.id,
                occurrence.first_step,
                ConfidenceGroup {
                    key: format!("{}-{}", item.id, occurrence.occurrence),
                    code: occurrence.part.code(),
                    title: title.clone().unwrap_or_default(),
                    color: css_color(occurrence.part.color(), [1.0; 4]),
                    lines: occurrence.slides.clone(),
                },
            ));
        }
    }
    groups
}

pub fn confidence_scene(config: &OutputConfig, program: &ProgramState, project: Option<&ProjectModel>, playback: Option<&PlaybackState>) -> ConfidenceScene {
    let groups = confidence_groups(project);
    let step = program.active_step_index.unwrap_or(0) as usize;
    let active = program.active_item_id.and_then(|id| {
        groups
            .iter()
            .position(|(item, start, g)| *item == id && step >= *start && step < start + g.lines.len())
            .map(|index| (index, step - groups[index].1))
    });
    let item = active_item(program, project);
    let shows_video = program.display == DisplayMode::Live
        && matches!(item.map(|i| &i.kind), Some(SessionItemKind::Video { options, .. }) if options.confidence);
    ConfidenceScene {
        groups: groups.into_iter().map(|(_, _, g)| g).collect(),
        active,
        size: config.confidence_size as f32,
        video: if shows_video { foreground_video(item, playback) } else { None },
        transition_ms: config.transition as f32,
    }
}

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

/// Rebuilds both scenes whenever the engine or the output style changes.
#[derive(Clone)]
pub struct SceneFeed {
    refresh: Sender<()>,
}

impl SceneFeed {
    /// `config` reads the current output style; `apply` receives the scenes
    /// and the ambient videos they need.
    pub fn start(
        snapshot: Arc<RwLock<EngineSnapshot>>,
        config: impl Fn() -> OutputConfig + Send + 'static,
        apply: impl Fn(Vec<(OutputRole, Scene)>, Vec<String>) + Send + 'static,
    ) -> anyhow::Result<Self> {
        let (refresh, requests) = unbounded();
        std::thread::Builder::new().name("scene-feed".into()).spawn(move || run(&requests, &snapshot, config, apply))?;
        let feed = Self { refresh };
        feed.refresh();
        Ok(feed)
    }

    pub fn refresh(&self) {
        let _ = self.refresh.send(());
    }
}

fn run(
    requests: &Receiver<()>,
    snapshot: &RwLock<EngineSnapshot>,
    config: impl Fn() -> OutputConfig,
    apply: impl Fn(Vec<(OutputRole, Scene)>, Vec<String>),
) {
    let mut last: Option<(MainScene, ConfidenceScene)> = None;
    while requests.recv().is_ok() {
        // Bursts of changes produce one rebuild.
        while requests.try_recv().is_ok() {}
        let config = config();
        let (main, confidence) = {
            let state = snapshot.read().expect("snapshot poisoned");
            let (program, project, playback) = (&state.program_state, state.project.as_ref(), state.playback.as_ref());
            (main_scene(&config, program, project, playback), confidence_scene(&config, program, project, playback))
        };
        if last.as_ref().is_some_and(|(m, c)| *m == main && *c == confidence) {
            continue;
        }
        let ambient = ambient_videos(&main);
        let mut scenes = Vec::new();
        if last.as_ref().is_none_or(|(m, _)| *m != main) {
            scenes.push((OutputRole::Projection, Scene::Main(main.clone())));
        }
        if last.as_ref().is_none_or(|(_, c)| *c != confidence) {
            scenes.push((OutputRole::Confidence, Scene::Confidence(confidence.clone())));
        }
        apply(scenes, ambient);
        last = Some((main, confidence));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{Background, PlaybackKind, PlaybackPhase, SessionModel, VideoOptions};

    fn project(items: Vec<SessionItem>) -> ProjectModel {
        ProjectModel { id: 1, name: "Service".into(), session: SessionModel { items } }
    }

    fn song_item(id: i64) -> SessionItem {
        let song = serde_json::json!({
            "format": "magi.song.v1", "slideSeparator": "blank-line", "background": "",
            "parts": [
                {"id": "v", "type": "Verse", "number": 1, "lyrics": "one\n\ntwo"},
                {"id": "c", "type": "Chorus", "number": 1, "lyrics": "chorus"}
            ],
            "arrangement": ["v", "c", "v"]
        });
        SessionItem {
            id,
            title: Some("Song".into()),
            kind: SessionItemKind::Text { steps: vec!["one".into(), "two".into(), "chorus".into(), "one".into(), "two".into()] },
            song: Some(song.to_string()),
            background: None,
        }
    }

    fn video_item(id: i64, confidence: bool) -> SessionItem {
        SessionItem {
            id,
            title: None,
            kind: SessionItemKind::Video { path: "/v/clip.mp4".into(), options: VideoOptions { confidence, ..Default::default() } },
            song: None,
            background: None,
        }
    }

    fn playback(session_id: u64, item_id: i64) -> PlaybackState {
        PlaybackState {
            session_id,
            generation: 0,
            item_id,
            path: "/v/clip.mp4".into(),
            kind: PlaybackKind::Video,
            paused: false,
            suspended: false,
            phase: PlaybackPhase::Playing,
            position_ms: 0,
            duration_ms: None,
            position_at_ms: 0,
            volume: 100,
            muted: false,
            rate_percent: 100,
            looping: false,
            start_ms: 0,
            end_ms: None,
            confidence: false,
            error: None,
        }
    }

    #[test]
    fn repeated_parts_are_separate_groups_and_the_active_line_follows_the_step() {
        let project = project(vec![song_item(1), video_item(2, false)]);
        let program = ProgramState { active_item_id: Some(1), active_step_index: Some(3), ..Default::default() };
        let scene = confidence_scene(&OutputConfig::default(), &program, Some(&project), None);
        let codes: Vec<_> = scene.groups.iter().map(|g| g.code.as_str()).collect();
        assert_eq!(codes, ["V1", "C1", "V1", "video"]);
        assert_eq!(scene.active, Some((2, 0)), "step 3 is the second verse occurrence");
        assert_eq!(scene.groups[3].lines, ["video"]);
    }

    #[test]
    fn legacy_plain_lyric_items_are_one_verse_of_their_steps() {
        let legacy = SessionItem {
            id: 5,
            title: Some("Old".into()),
            kind: SessionItemKind::Text { steps: vec!["first".into(), "second".into()] },
            song: None,
            background: None,
        };
        let project = project(vec![legacy]);
        let program = ProgramState { active_item_id: Some(5), active_step_index: Some(1), ..Default::default() };
        let scene = confidence_scene(&OutputConfig::default(), &program, Some(&project), None);
        assert_eq!(scene.groups.len(), 1);
        assert_eq!(scene.groups[0].code, "V1");
        assert_eq!(scene.groups[0].lines, ["first", "second"]);
        assert_eq!(scene.active, Some((0, 1)));
    }

    #[test]
    fn song_video_backgrounds_loop_ambient_and_cover() {
        let project = project(vec![song_item(1)]);
        let program = ProgramState {
            active_item_id: Some(1),
            background: Some(Background { kind: BackgroundKind::Video, path: "/bg/loop.mp4".into() }),
            ..Default::default()
        };
        let config = OutputConfig { background_fit: BackgroundFit::Contain, ..Default::default() };
        let scene = main_scene(&config, &program, Some(&project), None);
        let background = scene.background.clone().unwrap();
        assert_eq!(background.source, MediaSource::Video(VideoKey::Ambient("/bg/loop.mp4".into())));
        assert!(background.cover);
        assert_eq!(ambient_videos(&scene), ["/bg/loop.mp4"]);
    }

    #[test]
    fn a_video_item_uses_its_foreground_session_on_both_outputs_when_enabled() {
        let project = project(vec![video_item(2, true)]);
        let mut program = ProgramState {
            active_item_id: Some(2),
            background: Some(Background { kind: BackgroundKind::Video, path: "/v/clip.mp4".into() }),
            ..Default::default()
        };
        let playback = playback(7, 2);
        let main = main_scene(&OutputConfig::default(), &program, Some(&project), Some(&playback));
        assert_eq!(main.background.unwrap().source, MediaSource::Video(VideoKey::Session(7)));
        let confidence = confidence_scene(&OutputConfig::default(), &program, Some(&project), Some(&playback));
        assert_eq!(confidence.video, Some(VideoKey::Session(7)));
        // Only while Live.
        program.display = DisplayMode::Black;
        assert_eq!(confidence_scene(&OutputConfig::default(), &program, Some(&project), Some(&playback)).video, None);
    }

    #[test]
    fn logo_videos_play_only_while_the_logo_is_up() {
        let config = OutputConfig { logo_path: "/logo/spin.mov".into(), ..Default::default() };
        let mut program = ProgramState::default();
        assert!(ambient_videos(&main_scene(&config, &program, None, None)).is_empty());
        program.display = DisplayMode::Logo;
        assert_eq!(ambient_videos(&main_scene(&config, &program, None, None)), ["/logo/spin.mov"]);
    }

    #[test]
    fn box_opacity_applies_to_the_box_color() {
        let config = OutputConfig { box_color: "#102030".into(), box_opacity: 50.0, ..Default::default() };
        let style = lyric_style(&config);
        assert!((style.box_color[3] - 0.5).abs() < 1e-6);
        assert!((style.box_color[0] - 16.0 / 255.0).abs() < 1e-6);
    }
}
