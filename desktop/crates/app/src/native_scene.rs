//! Builds what each native output shows from the engine's state and the
//! saved output style, and keeps the renderer and ambient videos in step.

use std::sync::{Arc, RwLock};

use core_engine::song::Song;
use crossbeam::channel::{unbounded, Receiver, Sender};
use protocol::{
    BackgroundFit, BackgroundKind, CaptureFit, CaptureSource, DisplayMode, EngineSnapshot, OutputConfig, PlaybackState, ProgramState, ProjectModel,
    SessionItem, SessionItemKind, TextAlign as ConfigAlign, VerticalAlign,
};
use render_engine::{
    css_color, ConfidenceGroup, ConfidenceScene, ConfidenceStyle, Direction, Display, Easing, LyricStyle, MainScene, MediaLayer, MediaSource,
    OutputRole, Scene, Shadow, ShaderSpec, Stroke, TextAlign, Transition, TransitionKind, Vertical, VideoKey,
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

/// The capture `item` shows, with whether it fills (crops) the screen, if
/// it is routed to the projection (`confidence` false) or confidence screen.
fn capture_on(item: Option<&SessionItem>, confidence: bool) -> Option<(VideoKey, bool)> {
    let SessionItemKind::Capture { source, options } = &item?.kind else { return None };
    let (shown, fit) = if confidence { (options.confidence, options.confidence_fit) } else { (options.projection, options.projection_fit) };
    shown.then(|| (VideoKey::Capture(source.id.clone()), fit == CaptureFit::Fill))
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
        shadow: config.text_shadow.then(|| Shadow {
            color: opacity(css_color(&config.shadow_color, [0.0, 0.0, 0.0, 1.0]), config.shadow_opacity),
            x: config.shadow_offset_x as f32,
            y: config.shadow_offset_y as f32,
            blur: config.shadow_size as f32,
        }),
        outline: (config.text_outline && config.outline_width > 0.0)
            .then(|| Stroke { color: css_color(&config.outline_color, [0.0, 0.0, 0.0, 1.0]), size: config.outline_width as f32 }),
        glow: (config.text_glow && config.glow_size > 0.0)
            .then(|| Stroke { color: opacity(css_color(&config.glow_color, [1.0; 4]), config.glow_opacity), size: config.glow_size as f32 }),
    }
}

/// `color` with a 0–100 opacity applied.
fn opacity(mut color: render_engine::Rgba, percent: f64) -> render_engine::Rgba {
    color[3] *= (percent as f32 / 100.0).clamp(0.0, 1.0);
    color
}

/// The shader background named by a `shader:<preset>` path, in the
/// configured colors and speed.
pub fn shader_key(config: &OutputConfig, path: &str) -> VideoKey {
    let preset = path.trim_start_matches(protocol::SHADER_PREFIX);
    VideoKey::Shader(ShaderSpec::encode(preset, &config.shader_color_a, &config.shader_color_b, config.shader_speed as f32, config.shader_animate))
}

pub fn transition(config: &OutputConfig) -> Transition {
    use protocol::{Easing as E, TransitionDirection as D, TransitionKind as K};
    Transition {
        kind: match config.transition_kind {
            K::Fade => TransitionKind::Fade,
            K::Cut => TransitionKind::Cut,
            K::Slide => TransitionKind::Slide,
        },
        easing: match config.transition_easing {
            E::Linear => Easing::Linear,
            E::Ease => Easing::Ease,
            E::EaseIn => Easing::EaseIn,
            E::EaseOut => Easing::EaseOut,
            E::EaseInOut => Easing::EaseInOut,
        },
        direction: match config.transition_direction {
            D::Left => Direction::Left,
            D::Right => Direction::Right,
            D::Up => Direction::Up,
            D::Down => Direction::Down,
        },
    }
}

pub fn confidence_style(config: &OutputConfig) -> ConfidenceStyle {
    ConfidenceStyle {
        font: config.confidence_font.clone(),
        line_height: config.confidence_line_height as f32,
        caps: config.confidence_caps,
        clock: config.confidence_clock,
        tape: config.confidence_tape,
    }
}

pub fn main_scene(config: &OutputConfig, program: &ProgramState, project: Option<&ProjectModel>, playback: Option<&PlaybackState>) -> MainScene {
    let item = active_item(program, project);
    let item_is_video = matches!(item.map(|i| &i.kind), Some(SessionItemKind::Video { .. }));
    let fit_cover = config.background_fit == BackgroundFit::Cover;
    // A capture routed here replaces the background; otherwise the song background stays.
    let capture = capture_on(item, false).map(|(key, cover)| MediaLayer { source: MediaSource::Video(key), cover });
    let background = capture.or_else(|| program.background.as_ref().and_then(|bg| match bg.kind {
        BackgroundKind::Image => Some(MediaLayer { source: MediaSource::Image(bg.path.clone()), cover: fit_cover }),
        // The video item itself plays in the foreground session.
        BackgroundKind::Video if item_is_video => {
            foreground_video(item, playback).map(|key| MediaLayer { source: MediaSource::Video(key), cover: fit_cover })
        }
        // A song's background video loops muted and always fills the screen.
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

/// The lyric style alone, for the output settings dialog.
pub fn style_sample_scene(config: &OutputConfig) -> MainScene {
    let program = ProgramState { active_text: "Your love never fails\nYour mercy remains".into(), ..ProgramState::default() };
    MainScene { logo: None, ..main_scene(config, &program, None, None) }
}

/// A short song in the confidence style, for the output settings dialog.
pub fn confidence_sample_scene(config: &OutputConfig) -> ConfidenceScene {
    let group = |key: &str, code: &str, color: &str, lines: &[&str]| ConfidenceGroup {
        key: key.into(),
        code: code.into(),
        title: "Your Love Never Fails".into(),
        color: css_color(color, [1.0; 4]),
        lines: lines.iter().map(|l| l.to_string()).collect(),
    };
    ConfidenceScene {
        groups: vec![
            group("v1", "V1", "#4f91e8", &["Nothing can separate\nEven if I ran away", "Your love never fails"]),
            group("c1", "C1", "#e76774", &["Your love never fails\nIt never gives up", "It never runs out on me"]),
            group("b1", "BR1", "#aa84df", &["You make all things work together for my good"]),
        ],
        active: Some((1, 0)),
        size: config.confidence_size as f32,
        video: None,
        video_cover: false,
        transition_ms: config.confidence_transition as f32,
        style: confidence_style(config),
    }
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
            SessionItemKind::Capture { .. } => ("capture", &Vec::new()),
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
    let live = program.display == DisplayMode::Live;
    let shows_video = live && matches!(item.map(|i| &i.kind), Some(SessionItemKind::Video { options, .. }) if options.confidence);
    let capture = capture_on(item, true).filter(|_| live);
    ConfidenceScene {
        groups: groups.into_iter().map(|(_, _, g)| g).collect(),
        active,
        size: config.confidence_size as f32,
        video_cover: capture.as_ref().is_some_and(|(_, cover)| *cover),
        video: match capture {
            Some((key, _)) => Some(key),
            None if shows_video => foreground_video(item, playback),
            None => None,
        },
        transition_ms: config.confidence_transition as f32,
        style: confidence_style(config),
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
    let neighbors = match current {
        Some(i) => vec![items.get(i + 1), i.checked_sub(1).and_then(|p| items.get(p))],
        None => vec![items.first()],
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
            SessionItemKind::Text { .. } | SessionItemKind::Capture { .. } => {}
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

/// Rebuilds both scenes whenever the engine or the output style changes.
#[derive(Clone)]
pub struct SceneFeed {
    refresh: Sender<()>,
}

impl SceneFeed {
    /// `config` reads the current output style; `apply` receives the scenes
    /// that changed and the media they and the neighboring cues need.
    pub fn start(
        snapshot: Arc<RwLock<EngineSnapshot>>,
        config: impl Fn() -> OutputConfig + Send + 'static,
        apply: impl Fn(Vec<(OutputRole, Scene)>, &MediaPlan) + Send + 'static,
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
    apply: impl Fn(Vec<(OutputRole, Scene)>, &MediaPlan),
) {
    let mut last: Option<(MainScene, ConfidenceScene, MediaPlan)> = None;
    while requests.recv().is_ok() {
        // Bursts of changes produce one rebuild.
        while requests.try_recv().is_ok() {}
        let config = config();
        let (main, confidence, plan) = {
            let state = snapshot.read().expect("snapshot poisoned");
            let (program, project, playback) = (&state.program_state, state.project.as_ref(), state.playback.as_ref());
            let main = main_scene(&config, program, project, playback);
            let plan = media_plan(&config, &main, program, project, playback);
            (main, confidence_scene(&config, program, project, playback), plan)
        };
        if last.as_ref().is_some_and(|(m, c, p)| *m == main && *c == confidence && *p == plan) {
            continue;
        }
        let mut scenes = Vec::new();
        if last.as_ref().is_none_or(|(m, ..)| *m != main) {
            scenes.push((OutputRole::Projection, Scene::Main(main.clone())));
        }
        if last.as_ref().is_none_or(|(_, c, _)| *c != confidence) {
            scenes.push((OutputRole::Confidence, Scene::Confidence(confidence.clone())));
        }
        apply(scenes, &plan);
        last = Some((main, confidence, plan));
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
    fn shader_backgrounds_use_the_configured_colors() {
        let project = project(vec![song_item(1)]);
        let program = ProgramState {
            active_item_id: Some(1),
            background: Some(Background { kind: BackgroundKind::Shader, path: "shader:aurora".into() }),
            ..Default::default()
        };
        let config = OutputConfig { shader_color_b: "#ff0000".into(), ..Default::default() };
        let scene = main_scene(&config, &program, Some(&project), None);
        let MediaSource::Video(VideoKey::Shader(spec)) = scene.background.clone().unwrap().source else { panic!("a shader background") };
        let spec = ShaderSpec::parse(&spec).unwrap();
        assert_eq!((spec.preset.as_str(), spec.color_b), ("aurora", [1.0, 0.0, 0.0, 1.0]));
        assert!(ambient_videos(&scene).is_empty(), "shaders decode nothing");
    }

    #[test]
    fn effects_and_transitions_follow_the_settings() {
        let config = OutputConfig { text_outline: true, outline_width: 4.0, text_shadow: false, transition_kind: protocol::TransitionKind::Slide, ..Default::default() };
        let style = lyric_style(&config);
        assert_eq!(style.outline.map(|o| o.size), Some(4.0));
        assert!(style.shadow.is_none() && style.glow.is_none());
        assert_eq!(transition(&config).kind, TransitionKind::Slide);
        let defaults = lyric_style(&OutputConfig::default());
        assert!(defaults.shadow.is_none() && defaults.outline.is_none() && defaults.glow.is_none(), "effects start off");
    }

    fn image_item(id: i64, path: &str) -> SessionItem {
        SessionItem { id, title: None, kind: SessionItemKind::Image { path: path.into() }, song: None, background: None }
    }

    #[test]
    fn the_cues_either_side_are_prepared() {
        let mut song = song_item(2);
        song.background = Some("/bg/next-loop.mp4".into());
        let slides = SessionItem { id: 4, title: None, kind: SessionItemKind::Presentation { path: "/s/1.png".into(), slides: vec!["/s/1.png".into(), "/s/2.png".into(), "/s/3.png".into()] }, song: None, background: None };
        let project = project(vec![image_item(1, "/i/before.png"), video_item(3, false), song, slides]);
        let config = OutputConfig { logo_path: "/logo/spin.mov".into(), ..Default::default() };

        // On the video: the image before and the song after (its background loop).
        let program = ProgramState { active_item_id: Some(3), ..Default::default() };
        let main = main_scene(&config, &program, Some(&project), None);
        let plan = media_plan(&config, &main, &program, Some(&project), None);
        assert_eq!(plan.images, ["/i/before.png"]);
        assert_eq!(plan.prepared_ambient, ["/logo/spin.mov", "/bg/next-loop.mp4"]);
        assert_eq!(plan.foreground, None);

        // On the first image: the video after it is prepared as the next foreground.
        let program = ProgramState { active_item_id: Some(1), ..Default::default() };
        let plan = media_plan(&config, &main_scene(&config, &program, Some(&project), None), &program, Some(&project), None);
        assert_eq!(plan.foreground.as_deref(), Some("/v/clip.mp4"));

        // In a presentation: the slides either side.
        let program = ProgramState { active_item_id: Some(4), active_step_index: Some(1), ..Default::default() };
        let plan = media_plan(&config, &main_scene(&config, &program, Some(&project), None), &program, Some(&project), None);
        assert!(plan.images.contains(&"/s/1.png".to_string()) && plan.images.contains(&"/s/3.png".to_string()), "{:?}", plan.images);
    }

    #[test]
    fn a_playing_logo_loop_is_not_also_prepared() {
        let config = OutputConfig { logo_path: "/logo/spin.mov".into(), ..Default::default() };
        let program = ProgramState { display: DisplayMode::Logo, ..Default::default() };
        let main = main_scene(&config, &program, None, None);
        let plan = media_plan(&config, &main, &program, None, None);
        assert_eq!(plan.ambient, ["/logo/spin.mov"]);
        assert!(plan.prepared_ambient.is_empty());
    }

    #[test]
    fn box_opacity_applies_to_the_box_color() {
        let config = OutputConfig { box_color: "#102030".into(), box_opacity: 50.0, ..Default::default() };
        let style = lyric_style(&config);
        assert!((style.box_color[3] - 0.5).abs() < 1e-6);
        assert!((style.box_color[0] - 16.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn a_capture_shows_on_the_screens_it_is_routed_to() {
        use protocol::{CaptureFit, CaptureKind, CaptureOptions, CaptureSource};
        let source = CaptureSource { kind: CaptureKind::Window, id: "window:app|Slides".into(), name: "Slides".into() };
        let capture = |options: CaptureOptions| SessionItem { id: 9, title: None, kind: SessionItemKind::Capture { source: source.clone(), options }, song: None, background: None };
        let config = OutputConfig::default();
        let song_bg = Background { kind: BackgroundKind::Image, path: "/bg.png".into() };
        let program = ProgramState { active_item_id: Some(9), background: Some(song_bg), ..ProgramState::default() };
        let key = VideoKey::Capture(source.id.clone());

        let both = project(vec![capture(CaptureOptions { projection: true, confidence: true, projection_fit: CaptureFit::Fit, confidence_fit: CaptureFit::Fill })]);
        let main = main_scene(&config, &program, Some(&both), None);
        assert_eq!(main.background, Some(MediaLayer { source: MediaSource::Video(key.clone()), cover: false }));
        let confidence = confidence_scene(&config, &program, Some(&both), None);
        assert_eq!((confidence.video.clone(), confidence.video_cover), (Some(key.clone()), true));
        let plan = media_plan(&config, &main, &program, Some(&both), None);
        assert_eq!(plan.captures, vec![source.clone()]);

        let confidence_only = project(vec![capture(CaptureOptions { projection: false, confidence: true, ..CaptureOptions::default() })]);
        let main = main_scene(&config, &program, Some(&confidence_only), None);
        assert_eq!(main.background.unwrap().source, MediaSource::Image("/bg.png".into()), "the projection keeps the song background");
        let black = ProgramState { display: DisplayMode::Black, ..program.clone() };
        assert_eq!(confidence_scene(&config, &black, Some(&confidence_only), None).video, None, "the teleprompter returns when not live");
    }
}
