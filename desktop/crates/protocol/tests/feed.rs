use std::path::PathBuf;

use protocol::model::{Author, AuthorRole, Credits, Section, SectionKind, Service};
use protocol::{DisplayMode, EngineSnapshot, FeedDisplay, FeedItemKind, FeedState, LiveService, LiveSong, ProgramState, SessionItem, SessionItemKind};

fn section(id: &str, kind: SectionKind, number: Option<u32>, lyrics: &str) -> Section {
    Section { id: id.into(), kind, number, name: None, color: None, lyrics: lyrics.into() }
}

fn snapshot(display: DisplayMode, item: Option<&str>, step: Option<u32>) -> EngineSnapshot {
    let sections = vec![
        section("v1", SectionKind::Verse, Some(1), "Amazing grace, how sweet the sound\nThat saved a wretch like me\n\nI once was lost, but now am found"),
        Section { name: Some("Ending".into()), ..section("t", SectionKind::Tag, None, "Amen") },
    ];
    let steps = sections.iter().flat_map(Section::slides).collect();
    let credits = Credits { authors: vec![Author { name: "John Newton".into(), role: Some(AuthorRole::Words) }], copyright: Some("Public Domain".into()), ccli: Some("22025".into()), publisher: None, year: Some(1779) };
    let song = SessionItem {
        id: "i1".into(),
        title: None,
        kind: SessionItemKind::Text { steps },
        song: Some(LiveSong { song_id: "s1".into(), version_id: "v1".into(), sections, title: "Amazing Grace".into(), credits }),
        background: None,
    };
    let welcome = SessionItem { id: "i2".into(), title: Some("Welcome".into()), kind: SessionItemKind::Text { steps: vec!["Welcome home".into()] }, song: None, background: None };
    let service = Service { id: "svc".into(), scheduled_at_ms: 0, title: "Sunday".into(), leader: None, notes: None, ended_at_ms: None, items: Vec::new() };
    let live = LiveService { service, items: vec![song, welcome] };
    let text = item.zip(step).and_then(|(id, step)| match &live.item(id)?.kind {
        SessionItemKind::Text { steps } => steps.get(step as usize).cloned(),
        _ => None,
    });
    let program_state = ProgramState { display, active_item_id: item.map(Into::into), active_step_index: step, active_text: text.unwrap_or_default(), ..ProgramState::default() };
    EngineSnapshot { program_state, version: 1, service: Some(live), ..EngineSnapshot::default() }
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/feed")
}

#[test]
fn feed_messages_match_the_fixtures() {
    let cases = [
        ("song", snapshot(DisplayMode::Live, Some("i1"), Some(0))),
        ("song_tag", snapshot(DisplayMode::Live, Some("i1"), Some(2))),
        ("text", snapshot(DisplayMode::Live, Some("i2"), Some(0))),
        ("black", snapshot(DisplayMode::Black, Some("i1"), Some(1))),
        ("nothing", snapshot(DisplayMode::Clear, None, None)),
    ];
    let update = std::env::var_os("UPDATE_PROTOCOL_FIXTURES").is_some();
    for (name, snapshot) in cases {
        let state = FeedState { seq: 7, ..FeedState::from_snapshot(&snapshot) };
        let path = fixtures().join(format!("{name}.json"));
        let json = serde_json::to_string_pretty(&state).unwrap() + "\n";
        if update {
            std::fs::create_dir_all(fixtures()).unwrap();
            std::fs::write(&path, json).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(json, expected, "{} is out of date; regenerate it with UPDATE_PROTOCOL_FIXTURES=1", path.display());
    }
}

#[test]
fn the_feed_follows_the_song() {
    let state = FeedState::from_snapshot(&snapshot(DisplayMode::Live, Some("i1"), Some(1)));
    assert_eq!(state.lines, ["I once was lost, but now am found"]);
    assert_eq!(state.section.as_deref(), Some("Verse 1"));
    assert_eq!(state.next_text.as_deref(), Some("Amen"));
    assert_eq!(state.item.map(|i| (i.kind, i.title)), Some((FeedItemKind::Song, Some("Amazing Grace".into()))));
    assert_eq!(FeedState::from_snapshot(&snapshot(DisplayMode::Live, Some("i1"), Some(2))).section.as_deref(), Some("Ending"));
}

#[test]
fn only_content_changes_count() {
    let a = FeedState::from_snapshot(&snapshot(DisplayMode::Live, Some("i1"), Some(0)));
    assert!(a.same_content(&FeedState { seq: 9, ..a.clone() }));
    assert!(!a.same_content(&FeedState::from_snapshot(&snapshot(DisplayMode::Logo, Some("i1"), Some(0)))));
}

#[test]
fn messages_from_a_newer_palto_still_read() {
    let json = std::fs::read_to_string(fixtures().join("newer.json")).unwrap();
    let state: FeedState = serde_json::from_str(&json).unwrap();
    assert_eq!(state.display, FeedDisplay::Other);
    assert_eq!(state.item.unwrap().kind, FeedItemKind::Other);
    assert_eq!(state.lines, ["Still here"]);
}
