use crate::*;

#[test]
fn existing_video_services_load_with_playback_defaults() {
    let video: SessionItemKind = serde_json::from_str(r#"{"kind":"video","data":{"path":"clip.mp4"}}"#).unwrap();
    assert_eq!(video, SessionItemKind::Video { path: "clip.mp4".into(), options: VideoOptions::default() });
    let audio: SessionItemKind = serde_json::from_str(r#"{"kind":"audio","data":{"path":"song.mp3"}}"#).unwrap();
    assert_eq!(audio, SessionItemKind::Audio { path: "song.mp3".into(), options: VideoOptions::default() });
}

#[test]
fn remote_clients_keep_shorthand_transport_commands() {
    let play: EngineCommand = serde_json::from_str(r#"{"type":"play"}"#).unwrap();
    assert!(matches!(play.normalize(), EngineCommand::Media { session_id: None, action: MediaAction::Play }));
    let seek: EngineCommand = serde_json::from_str(r#"{"type":"seek","payload":{"time_ms":-5}}"#).unwrap();
    assert!(matches!(seek.normalize(), EngineCommand::Media { action: MediaAction::Seek { position_ms: 0 }, .. }));
}

#[test]
fn addressed_media_commands_have_a_stable_wire_format() {
    let json = r#"{"type":"media","payload":{"session_id":42,"action":{"type":"seek","payload":{"position_ms":1500}}}}"#;
    let command: EngineCommand = serde_json::from_str(json).unwrap();
    assert!(matches!(command, EngineCommand::Media { session_id: Some(42), action: MediaAction::Seek { position_ms: 1500 } }));
    let directive = MediaDirective::Seek { session_id: 42, generation: 3, position_ms: 1500 };
    assert_eq!(
        serde_json::to_value(&directive).unwrap(),
        serde_json::json!({"type": "seek", "session_id": 42, "generation": 3, "position_ms": 1500})
    );
}

#[test]
fn snapshots_from_before_playback_existed_still_load() {
    let old = r#"{"program_state":{"mode":"manual","transport":"stopped","time_ms":0,"active_item_id":null,"next_item_id":null,"active_step_index":null,"active_text":"","background":null},"version":3,"service":null}"#;
    let snapshot: EngineSnapshot = serde_json::from_str(old).unwrap();
    assert_eq!((snapshot.version, snapshot.playback, snapshot.playback_revision), (3, None, 0));
}
