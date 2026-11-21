use super::*;

fn section(id: &str, kind: SectionKind, lyrics: &str) -> Section {
    Section { id: id.into(), kind, number: None, name: None, color: None, lyrics: lyrics.into() }
}

fn version() -> SongVersion {
    SongVersion {
        id: "v1".into(),
        number: 1,
        sections: vec![section("a", SectionKind::Verse, "one"), section("b", SectionKind::Chorus, "two")],
        arrangement: vec!["a".into(), "b".into(), "b".into()],
        background: None,
    }
}

fn item(arrangement: Option<Vec<Entry>>) -> SongItem {
    SongItem { song_id: "s".into(), version_id: None, key: None, arrangement }
}

#[test]
fn ids_are_unique_and_sort_by_time() {
    let a = new_id();
    let b = new_id();
    assert_ne!(a, b);
    assert_eq!(a.len(), 23);
    assert!(a[..10] <= b[..10]);
}

#[test]
fn a_song_item_without_its_own_arrangement_follows_the_library() {
    let lyrics: Vec<_> = item(None).resolve(&version()).into_iter().map(|s| s.lyrics).collect();
    assert_eq!(lyrics, ["one", "two", "two"]);
}

#[test]
fn variations_change_one_occurrence_and_service_only_parts_stand_alone() {
    let entries = vec![
        Entry { id: "e1".into(), section_id: Some("b".into()), variation: SectionPatch { lyrics: Some("two, softly".into()), ..Default::default() } },
        Entry { id: "e2".into(), section_id: Some("b".into()), variation: SectionPatch::default() },
        Entry { id: "e3".into(), section_id: None, variation: SectionPatch { kind: Some(SectionKind::Tag), lyrics: Some("amen".into()), ..Default::default() } },
        Entry { id: "e4".into(), section_id: Some("gone".into()), variation: SectionPatch::default() },
    ];
    let resolved = item(Some(entries)).resolve(&version());
    let lyrics: Vec<_> = resolved.iter().map(|s| s.lyrics.as_str()).collect();
    assert_eq!(lyrics, ["two, softly", "two", "amen"]);
    assert_eq!(resolved[0].kind, SectionKind::Chorus);
    assert_eq!(resolved[2].kind, SectionKind::Tag);
}

#[test]
fn a_service_is_past_from_the_day_after_or_once_ended() {
    let mut service = Service { id: "x".into(), scheduled_at_ms: 1_000, title: "Sunday".into(), leader: None, notes: None, ended_at_ms: None, items: vec![] };
    assert!(!service.is_past(500));
    assert!(service.is_past(1_001));
    service.ended_at_ms = Some(1);
    assert!(service.is_past(500));
}

#[test]
fn device_fields_can_be_split_off() {
    let content = ItemContent::Capture {
        options: crate::CaptureOptions::default(),
        source: Some(crate::CaptureSource { kind: crate::CaptureKind::Screen, id: "1".into(), name: "Main".into() }),
    };
    assert!(serde_json::to_string(&content).unwrap().contains("Main"));
    assert!(!serde_json::to_string(&content.without_device_fields()).unwrap().contains("Main"));
}

#[test]
fn shared_section_fixtures_match() {
    let fixtures: serde_json::Value = serde_json::from_str(include_str!("../../../../fixtures/sections.json")).unwrap();
    for case in fixtures["slides"].as_array().unwrap() {
        let expected: Vec<String> = serde_json::from_value(case["slides"].clone()).unwrap();
        assert_eq!(split_slides(case["lyrics"].as_str().unwrap()), expected, "{}", case["name"]);
    }
    for case in fixtures["codes"].as_array().unwrap() {
        let mut value = case["section"].clone();
        value["id"] = "x".into();
        value["lyrics"] = "".into();
        let section: Section = serde_json::from_value(value).unwrap();
        assert_eq!((section.code(), section.color()), (case["code"].as_str().unwrap().to_string(), case["color"].as_str().unwrap()), "{}", case["section"]);
    }
}
