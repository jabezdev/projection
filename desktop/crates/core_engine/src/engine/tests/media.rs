use super::*;

fn video_item(id: i64, path: &str, options: VideoOptions) -> SessionItem {
    SessionItem { song: None, background: None, id: id.to_string(), title: None, kind: SessionItemKind::Video { path: path.into(), options } }
}

fn video_setup() -> Harness {
    let mut h = setup();
    {
        let mut items = h.items();
        items[0].background = Some("song.jpg".into());
        items[1] = video_item(20, "clip.mp4", VideoOptions { start_ms: 2_000, end_ms: Some(9_000), ..VideoOptions::default() });
    }
    h.load();
    h
}

fn report(h: &Harness, phase: PlaybackPhase, position_ms: u64) -> PlaybackReport {
    let p = h.playback();
    PlaybackReport { session_id: p.session_id, generation: p.generation, phase, position_ms, duration_ms: Some(10_000), error: None }
}

#[test]
fn without_autoplay_a_cued_video_waits_paused_on_its_first_frame() {
    let mut h = video_setup();
    h.send(EngineCommand::SetAutoplay { enabled: false });
    h.take_directives();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    assert!(h.playback().paused);
    assert!(matches!(h.take_directives().as_slice(), [MediaDirective::Open { playback }] if playback.paused));
}

#[test]
fn cueing_a_video_opens_one_session_and_leaving_closes_it_once() {
    let mut h = video_setup();
    h.take_directives();
    let events = h.send(EngineCommand::GoToItem { item_id: "20".into() });
    let playback = h.playback().clone();
    assert_eq!((playback.item_id.as_str(), playback.position_ms, playback.end_ms), ("20", 2_000, Some(9_000)));
    assert!(!playback.paused);
    assert!(matches!(h.take_directives().as_slice(), [MediaDirective::Open { playback: p }] if p.session_id == playback.session_id));
    assert!(events.iter().any(|e| matches!(e, EngineEvent::PlaybackChanged { playback: Some(_), .. })));

    h.send(EngineCommand::GoToItem { item_id: "30".into() });
    assert_eq!(h.take_directives(), vec![MediaDirective::Close { session_id: playback.session_id }]);
    assert!(h.engine.snapshot().playback.is_none());
    h.send(EngineCommand::GoToStep { index: 0 });
    assert!(h.take_directives().is_empty(), "the session closes exactly once");
}

#[test]
fn a_foreground_video_never_becomes_the_next_song_background() {
    let mut h = video_setup();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    assert_eq!(h.engine.snapshot().program_state.background.as_ref().map(|b| b.path.as_str()), Some("clip.mp4"));
    h.send(EngineCommand::GoToItem { item_id: "30".into() });
    assert_eq!(h.engine.snapshot().program_state.background.as_ref().map(|b| b.path.as_str()), Some("song.jpg"));
}

#[test]
fn without_a_song_background_leaving_a_video_clears_the_background() {
    let mut h = video_setup();
    h.items()[0].background = None;
    h.load();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    h.send(EngineCommand::GoToItem { item_id: "30".into() });
    assert_eq!(h.engine.snapshot().program_state.background, None);
}

#[test]
fn recueing_the_same_file_rejects_commands_for_the_earlier_session() {
    let mut h = video_setup();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    let first = h.playback().session_id;
    h.send(EngineCommand::GoToItem { item_id: "30".into() });
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    let second = h.playback().session_id;
    assert_ne!(first, second);
    h.take_directives();

    let events = h.send(EngineCommand::Media { session_id: Some(first), action: MediaAction::Pause });
    assert!(matches!(events.as_slice(), [EngineEvent::Error { source, .. }] if source == "media"));
    assert!(!h.playback().paused);
    assert!(h.take_directives().is_empty());

    h.send(EngineCommand::Media { session_id: Some(second), action: MediaAction::Pause });
    assert!(h.playback().paused);
    assert_eq!(h.take_directives(), vec![MediaDirective::Pause { session_id: second }]);
}

#[test]
fn reports_from_earlier_sessions_or_seek_generations_are_ignored() {
    let mut h = video_setup();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    let stale_generation = report(&h, PlaybackPhase::Playing, 8_000);
    h.send(EngineCommand::Media { session_id: None, action: MediaAction::Seek { position_ms: 3_000 } });
    assert_eq!(h.playback().generation, 1);

    assert!(h.report(stale_generation).is_empty(), "a pre-seek position must not snap back");
    let mut other_session = report(&h, PlaybackPhase::Playing, 1_000);
    other_session.session_id += 99;
    assert!(h.report(other_session).is_empty());
    assert_eq!(h.playback().position_ms, 3_000);

    h.now_ms = 5_000;
    let events = h.report(report(&h, PlaybackPhase::Playing, 3_250));
    let p = h.playback();
    assert_eq!((p.phase, p.position_ms, p.position_at_ms, p.duration_ms), (PlaybackPhase::Playing, 3_250, 5_000, Some(10_000)));
    assert!(matches!(events.as_slice(), [EngineEvent::PlaybackChanged { .. }]));
}

#[test]
fn seeks_clamp_to_the_clip_and_start_a_new_generation() {
    let mut h = video_setup();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    let session_id = h.playback().session_id;
    h.take_directives();
    h.send(EngineCommand::Media { session_id: None, action: MediaAction::Seek { position_ms: 500 } });
    h.send(EngineCommand::Media { session_id: None, action: MediaAction::Seek { position_ms: 60_000 } });
    assert_eq!(
        h.take_directives(),
        vec![
            MediaDirective::Seek { session_id, generation: 1, position_ms: 2_000 },
            MediaDirective::Seek { session_id, generation: 2, position_ms: 8_999 },
        ]
    );
}

#[test]
fn stop_pauses_and_returns_to_the_clip_start() {
    let mut h = video_setup();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    let session_id = h.playback().session_id;
    h.take_directives();
    h.send(EngineCommand::Stop);
    assert!(h.playback().paused);
    assert_eq!(
        h.take_directives(),
        vec![MediaDirective::Pause { session_id }, MediaDirective::Seek { session_id, generation: 1, position_ms: 2_000 }]
    );
}

#[test]
fn play_after_the_clip_ended_restarts_from_the_clip_start() {
    let mut h = video_setup();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    h.report(report(&h, PlaybackPhase::Ended, 9_000));
    assert!(h.playback().paused, "a non-looping clip pauses at its end");
    let session_id = h.playback().session_id;
    h.take_directives();
    h.send(EngineCommand::Play);
    assert_eq!(
        h.take_directives(),
        vec![MediaDirective::Seek { session_id, generation: 1, position_ms: 2_000 }, MediaDirective::Play { session_id }]
    );
}

#[test]
fn logo_suspends_foreground_media_and_live_resumes_it() {
    let mut h = video_setup();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    let session_id = h.playback().session_id;
    h.take_directives();

    h.send(EngineCommand::SetDisplay { display: DisplayMode::Black });
    h.send(EngineCommand::SetDisplay { display: DisplayMode::Clear });
    assert!(h.take_directives().is_empty());

    h.send(EngineCommand::SetDisplay { display: DisplayMode::Logo });
    assert!(h.playback().suspended && !h.playback().paused);
    assert_eq!(h.take_directives(), vec![MediaDirective::Pause { session_id }]);

    h.send(EngineCommand::SetDisplay { display: DisplayMode::Live });
    assert!(h.playback().running());
    assert_eq!(h.take_directives(), vec![MediaDirective::Play { session_id }]);
}

#[test]
fn a_paused_video_stays_paused_after_logo() {
    let mut h = video_setup();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    h.send(EngineCommand::Pause);
    h.take_directives();
    h.send(EngineCommand::SetDisplay { display: DisplayMode::Logo });
    h.send(EngineCommand::SetDisplay { display: DisplayMode::Live });
    assert!(h.take_directives().is_empty());
    assert!(h.playback().paused);
}

#[test]
fn video_cued_under_logo_waits_for_live() {
    let mut h = video_setup();
    h.send(EngineCommand::SetDisplay { display: DisplayMode::Logo });
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    assert!(h.playback().suspended);
    let session_id = h.playback().session_id;
    h.take_directives();
    h.send(EngineCommand::SetDisplay { display: DisplayMode::Live });
    assert_eq!(h.take_directives(), vec![MediaDirective::Play { session_id }]);
}

fn audio_setup() -> Harness {
    let mut h = setup();
    {
        let mut items = h.items();
        items[0].background = Some("song.jpg".into());
        items[1] = SessionItem {
            song: None,
            background: None,
            id: "20".into(),
            title: None,
            kind: SessionItemKind::Audio { path: "song.mp3".into(), options: VideoOptions { looping: true, volume: 40, ..VideoOptions::default() } },
        };
    }
    h.load();
    h
}

#[test]
fn cueing_standalone_audio_opens_an_audio_session_over_the_song_background() {
    let mut h = audio_setup();
    h.send(EngineCommand::GoToItem { item_id: "0".into() });
    h.take_directives();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    let playback = h.playback().clone();
    assert_eq!((playback.kind, playback.volume, playback.looping, playback.paused), (PlaybackKind::Audio, 40, true, false));
    assert!(matches!(h.take_directives().as_slice(), [MediaDirective::Open { playback: p }] if p.kind == PlaybackKind::Audio));
    assert_eq!(h.engine.snapshot().program_state.background.as_ref().map(|b| b.path.as_str()), Some("song.jpg"));

    h.send(EngineCommand::GoToItem { item_id: "30".into() });
    assert_eq!(h.take_directives(), vec![MediaDirective::Close { session_id: playback.session_id }]);
}

#[test]
fn standalone_audio_keeps_playing_under_the_logo() {
    let mut h = audio_setup();
    h.send(EngineCommand::SetDisplay { display: DisplayMode::Logo });
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    assert!(h.playback().running(), "audio cued under the Logo starts");
    h.take_directives();
    h.send(EngineCommand::SetDisplay { display: DisplayMode::Live });
    h.send(EngineCommand::SetDisplay { display: DisplayMode::Logo });
    assert!(h.playback().running());
    assert!(h.take_directives().is_empty());
}

#[test]
fn editing_the_active_video_options_reconfigures_without_restarting() {
    let mut h = video_setup();
    h.send(EngineCommand::GoToItem { item_id: "20".into() });
    let session_id = h.playback().session_id;
    h.take_directives();
    h.save(|items| items[1] = video_item(20, "clip.mp4", VideoOptions { volume: 40, looping: true, ..VideoOptions::default() }));
    let p = h.playback();
    assert_eq!((p.session_id, p.volume, p.looping, p.start_ms, p.end_ms), (session_id, 40, true, 0, None));
    assert!(matches!(h.take_directives().as_slice(), [MediaDirective::Configure { .. }]));
}

#[test]
fn media_commands_without_a_cued_session_are_errors() {
    let mut h = setup();
    h.load();
    let events = h.send(EngineCommand::Play);
    assert!(matches!(events.as_slice(), [EngineEvent::Error { source, .. }] if source == "media"));
    assert!(h.media.directives.is_empty());
}

#[test]
fn shorthand_and_addressed_transport_commands_are_equivalent() {
    let (mut a, mut b) = (video_setup(), video_setup());
    a.send(EngineCommand::GoToItem { item_id: "20".into() });
    b.send(EngineCommand::GoToItem { item_id: "20".into() });
    let id = b.playback().session_id;
    a.take_directives();
    b.take_directives();
    a.send(EngineCommand::Seek { time_ms: 4_000 });
    b.send(EngineCommand::Media { session_id: Some(id), action: MediaAction::Seek { position_ms: 4_000 } });
    assert_eq!(a.take_directives(), b.take_directives());
    assert_eq!(a.playback(), b.playback());
}
