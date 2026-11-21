use protocol::{
    BackgroundFit, BackgroundKind, DisplayMode, OutputConfig, PlaybackState, ProgramState, ProjectModel,
    SessionItem, SessionItemKind,
};
use render_engine::{
    MediaLayer, MediaSource, ShaderSpec, TransitionKind, VideoKey,
};

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

    let program = ProgramState { active_item_id: Some(3), ..Default::default() };
    let main = main_scene(&config, &program, Some(&project), None);
    let plan = media_plan(&config, &main, &program, Some(&project), None);
    assert_eq!(plan.images, ["/i/before.png"]);
    assert_eq!(plan.prepared_ambient, ["/logo/spin.mov", "/bg/next-loop.mp4"]);
    assert_eq!(plan.foreground, None);

    let program = ProgramState { active_item_id: Some(1), ..Default::default() };
    let plan = media_plan(&config, &main_scene(&config, &program, Some(&project), None), &program, Some(&project), None);
    assert_eq!(plan.foreground.as_deref(), Some("/v/clip.mp4"));

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
