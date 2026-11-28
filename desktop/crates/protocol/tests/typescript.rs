use std::path::PathBuf;

use protocol::model::{Author, AuthorRole, Change, Credits, Entry, ItemContent, MediaFile, MediaKind, Section, SectionKind, SectionPatch, Service, ServiceItem, ServiceTemplate, Song, SongItem, SongVersion};
use protocol::sync::{Conflict, Push, RecordKind, SyncState, SyncStatus};
use protocol::{
    CaptureFit, LiveService, LiveSong, CaptureKind, CaptureOptions, CaptureSource, EngineCommand, EngineEvent, InputBinding, InputBindings, InputTrigger, MediaAction, MediaDirective, MidiMessage, OutputConfig, PlaybackKind,
    PlaybackPhase, PlaybackReport, PlaybackState, ProgramState, SessionItem, SessionItemKind, StageState, StageTimer, TimerAction, VideoOptions,
};
use serde::Serialize;

fn playback(kind: PlaybackKind) -> PlaybackState {
    PlaybackState {
        session_id: 7,
        generation: 2,
        item_id: "30".into(),
        path: "/media/clip.mp4".into(),
        kind,
        paused: false,
        suspended: false,
        phase: PlaybackPhase::Playing,
        position_ms: 1_500,
        duration_ms: Some(60_000),
        position_at_ms: 1_700_000_000_000,
        volume: 80,
        muted: false,
        rate_percent: 100,
        looping: true,
        start_ms: 0,
        end_ms: Some(30_000),
        confidence: true,
        error: None,
    }
}

fn item(id: i64, kind: SessionItemKind) -> SessionItem {
    SessionItem { id: id.to_string(), title: Some("Item".into()), kind, song: None, background: None }
}

fn section(id: &str, kind: SectionKind, lyrics: &str) -> Section {
    Section { id: id.into(), kind, number: Some(1), name: None, color: None, lyrics: lyrics.into() }
}

fn song() -> Song {
    let current = SongVersion { id: "ver1".into(), number: 1, sections: vec![section("v1", SectionKind::Verse, "Line one\n\nLine two"), Section { name: Some("Vamp".into()), color: Some("#123456".into()), number: None, ..section("x", SectionKind::Other, "Oh") }], arrangement: vec!["v1".into(), "x".into()], background: Some("med1".into()) };
    let credits = Credits { authors: vec![Author { name: "John Newton".into(), role: Some(AuthorRole::Words) }, Author { name: "Anon".into(), role: None }], copyright: Some("Public Domain".into()), ccli: Some("22025".into()), publisher: None, year: Some(1779) };
    Song { id: "song1".into(), title: "Amazing Grace".into(), archived: false, credits, key: Some("G".into()), tags: vec!["hymn".into()], current }
}

fn service(options: VideoOptions) -> Service {
    let entry = |id: &str, section_id: Option<&str>, variation: SectionPatch| Entry { id: id.into(), section_id: section_id.map(Into::into), variation };
    let content = [
        ItemContent::Song(SongItem { song_id: "song1".into(), version_id: Some("ver1".into()), key: Some("A".into()), arrangement: Some(vec![
            entry("e1", Some("v1"), SectionPatch::default()),
            entry("e2", Some("v1"), SectionPatch { lyrics: Some("Softly".into()), ..Default::default() }),
            entry("e3", None, SectionPatch { kind: Some(SectionKind::Tag), number: Some(2), name: Some("End".into()), color: Some("#ffffff".into()), lyrics: Some("Amen".into()) }),
        ]) }),
        ItemContent::Text { steps: vec!["Welcome".into()] },
        ItemContent::Media { media_id: "med1".into(), options },
        ItemContent::Presentation { media_id: Some("med2".into()), slides: vec!["/media/p1.png".into()] },
        ItemContent::Capture { options: CaptureOptions::default(), source: Some(CaptureSource { kind: CaptureKind::Screen, id: "screen:1".into(), name: "Main".into() }) },
        ItemContent::Scripture { reference: "John 3:16-17".into(), translation: "kjv".into(), parallel: Some("adb1905".into()) },
        ItemContent::Header,
    ];
    let items = content.into_iter().enumerate().map(|(i, content)| ServiceItem { id: format!("item{i}"), title: Some("Item".into()), notes: Some("Note".into()), background: Some("shader:aurora".into()), content }).collect();
    Service { id: "svc1".into(), scheduled_at_ms: 1_795_000_000_000, title: "Sunday".into(), leader: Some("Ana".into()), notes: None, ended_at_ms: Some(1_795_000_100_000), items }
}

fn sample(name: &str, ts_type: &str, value: impl Serialize) -> String {
    let json = serde_json::to_string_pretty(&value).expect("serializable sample");
    format!("export const {name} = {json} satisfies {ts_type};\n")
}

fn generated() -> String {
    let options = VideoOptions { confidence: true, looping: true, start_ms: 1_000, end_ms: Some(9_000), volume: 70, rate_percent: 125 };
    let samples = [
        sample("videoPlayback", "PlaybackState", playback(PlaybackKind::Video)),
        sample("audioPlayback", "PlaybackState", playback(PlaybackKind::Audio)),
        sample(
            "directives",
            "MediaDirective[]",
            [
                MediaDirective::Open { playback: playback(PlaybackKind::Video) },
                MediaDirective::Play { session_id: 7 },
                MediaDirective::Pause { session_id: 7 },
                MediaDirective::Seek { session_id: 7, generation: 3, position_ms: 4_000 },
                MediaDirective::Configure { playback: playback(PlaybackKind::Audio) },
                MediaDirective::Close { session_id: 7 },
            ],
        ),
        sample(
            "actions",
            "MediaAction[]",
            [
                MediaAction::Play,
                MediaAction::Pause,
                MediaAction::Stop,
                MediaAction::Seek { position_ms: 2_000 },
                MediaAction::SetMuted { muted: true },
                MediaAction::SetVolume { volume: 50 },
            ],
        ),
        sample(
            "mediaCommand",
            "{ type: 'media'; payload: { session_id: number; action: MediaAction } }",
            EngineCommand::Media { session_id: Some(7), action: MediaAction::Seek { position_ms: 2_000 } },
        ),
        sample(
            "report",
            "PlaybackReport",
            PlaybackReport { session_id: 7, generation: 3, phase: PlaybackPhase::Ended, position_ms: 9_000, duration_ms: None, error: Some("Decoder stopped".into()) },
        ),
        sample(
            "playbackEvent",
            "{ type: 'playback_changed'; payload: { playback: PlaybackState | null; revision: number } }",
            EngineEvent::PlaybackChanged { playback: Some(playback(PlaybackKind::Video)), revision: 12 },
        ),
        sample(
            "items",
            "SessionItem[]",
            [
                SessionItem { song: Some(LiveSong { song_id: "song1".into(), version_id: "ver1".into(), sections: song().current.sections, title: song().title, credits: song().credits }), ..item(1, SessionItemKind::text(vec!["Line one".into()])) },
                item(2, SessionItemKind::Image { path: "/media/a.png".into() }),
                item(3, SessionItemKind::Video { path: "/media/b.mp4".into(), options: options.clone() }),
                item(4, SessionItemKind::Audio { path: "/media/c.mp3".into(), options }),
                item(5, SessionItemKind::Presentation { path: "/media/d.png".into(), slides: vec!["/media/d.png".into()] }),
                item(
                    6,
                    SessionItemKind::Capture {
                        source: CaptureSource { kind: CaptureKind::Window, id: "window:PowerPoint|Sermon".into(), name: "Sermon - PowerPoint".into() },
                        options: CaptureOptions { projection: true, confidence: true, projection_fit: CaptureFit::Fill, confidence_fit: CaptureFit::Fit },
                    },
                ),
                item(7, SessionItemKind::Section {}),
                item(8, SessionItemKind::Text { steps: vec!["For God so loved".into()], parallel: vec!["Sapagka't".into()], captions: vec!["John 3:16 · KJV · ADB1905".into()] }),
            ],
        ),
        sample(
            "stageCommands",
            "({ type: 'stage_timer'; payload: { action: TimerAction } } | { type: 'set_stage_message'; payload: { text: string } })[]",
            [
                EngineCommand::StageTimer { action: TimerAction::Start { duration_ms: 1_800_000 } },
                EngineCommand::StageTimer { action: TimerAction::Pause },
                EngineCommand::StageTimer { action: TimerAction::Resume },
                EngineCommand::StageTimer { action: TimerAction::Adjust { delta_ms: -60_000 } },
                EngineCommand::StageTimer { action: TimerAction::Clear },
                EngineCommand::SetStageMessage { text: "Wrap up".into() },
            ],
        ),
        sample(
            "program",
            "ProgramState",
            ProgramState {
                active_item_id: Some("8".into()),
                active_step_index: Some(0),
                active_text: "For God so loved".into(),
                active_parallel: "Sapagka't".into(),
                active_caption: "John 3:16 · KJV · ADB1905".into(),
                stage: StageState { message: "Wrap up".into(), timer: Some(StageTimer { duration_ms: 1_800_000, remaining_ms: 900_000, running_since_ms: Some(1_795_000_000_000) }) },
                ..ProgramState::default()
            },
        ),
        sample("song", "Song", song()),
        sample("media", "MediaFile[]", [
            MediaFile { id: "med1".into(), title: "Loop".into(), kind: MediaKind::Video, hash: None, path: Some("/media/loop.mp4".into()) },
            MediaFile { id: "med2".into(), title: "Slides".into(), kind: MediaKind::Presentation, hash: Some("abc".into()), path: None },
        ]),
        sample("service", "Service", service(VideoOptions::default())),
        sample("serviceEvent", "{ type: 'service_changed'; payload: { service: LiveService | null; version: number } }", EngineEvent::ServiceChanged { service: Some(LiveService { service: service(VideoOptions::default()), items: vec![item(9, SessionItemKind::Section {})] }), version: 3 }),
        sample("outputConfig", "OutputConfig", OutputConfig::default()),
        sample("inputBindings", "InputBindings", {
            let mut map = InputBindings::default();
            map.bindings.push(InputBinding { trigger: InputTrigger::Midi { message: MidiMessage::Control, channel: 2, number: 64 }, action: protocol::InputAction::Stop });
            map.bindings.push(InputBinding { trigger: InputTrigger::StreamDeck { button: 3 }, action: protocol::InputAction::Logo });
            map.bindings.push(InputBinding { trigger: InputTrigger::Key { key: "k".into(), ctrl: true, alt: false, shift: true }, action: protocol::InputAction::Clear });
            map
        }),
        sample("templates", "ServiceTemplate[]", {
            let mut sunday = ServiceTemplate::sunday();
            sunday.parts[1].items.push(service(VideoOptions::default()).items[2].clone());
            vec![sunday]
        }),
        sample("syncStatus", "SyncStatus", SyncStatus { paired: true, deployment: Some("https://happy-cat-123.convex.cloud".into()), device_name: Some("Church PC".into()), offline: false, state: SyncState::Idle, last_synced_ms: Some(1_795_000_000_000), error: None, pending: 2, conflicts: 1, uploads: 3, downloads: 0, live_service: Some("svc1".into()) }),
        sample("conflicts", "Conflict[]", [Conflict {
            id: "c1".into(),
            kind: RecordKind::Service,
            record_id: "svc1".into(),
            title: "Sunday".into(),
            label: "Rename service".into(),
            mine: serde_json::json!("Sunday AM"),
            theirs: serde_json::json!("Sunday Service"),
            keep_mine: Push::Change { change: Change::Service { id: "svc1".into(), ops: vec![] }, versions: vec![], current: None },
            at_ms: 1_795_000_000_000,
        }]),
    ];
    let mut text = String::from(
        "// Generated by `cargo test -p protocol --test typescript`; do not edit.\n\
         import type { MediaAction, MediaDirective, PlaybackReport, PlaybackState } from './playback';\n\
         import type { Conflict, LiveService, MediaFile, Service, ServiceTemplate, SessionItem, Song, SyncStatus } from './models';\n\
         import type { OutputConfig } from '../settings/outputState';\n\
         import type { InputBindings } from './input';\n\
         import type { ProgramState, TimerAction } from './useEngine';\n\n",
    );
    for sample in samples {
        text.push_str(&sample);
    }
    text
}

#[test]
fn typescript_fixtures_match_the_wire_format() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../app/frontend/src/api/protocol.fixtures.ts");
    let expected = generated();
    if std::env::var_os("UPDATE_PROTOCOL_FIXTURES").is_some() {
        std::fs::write(&path, &expected).expect("write fixtures");
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == expected,
        "{} is out of date with the protocol; regenerate it with UPDATE_PROTOCOL_FIXTURES=1 and fix any TypeScript type errors",
        path.display()
    );
}
