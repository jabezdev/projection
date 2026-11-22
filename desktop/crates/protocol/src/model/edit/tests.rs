use serde_json::{json, Value};

use super::*;
use crate::model::{Entry, ItemContent, SectionKind, SectionPatch, SongItem, SongVersion};

fn text(id: &str) -> ServiceItem {
    ServiceItem { id: id.into(), title: Some(id.to_uppercase()), notes: None, background: None, content: ItemContent::Text { steps: vec![id.into()] } }
}

fn song_item(id: &str) -> ServiceItem {
    ServiceItem { id: id.into(), title: None, notes: None, background: None, content: ItemContent::Song(SongItem { song_id: "grace".into(), version_id: None, key: None, arrangement: None }) }
}

fn service(items: Vec<ServiceItem>) -> Service {
    Service { id: "s".into(), scheduled_at_ms: 1, title: "Sunday".into(), leader: None, notes: None, ended_at_ms: None, items }
}

fn order(ids: &str) -> Service {
    service(ids.chars().map(|c| text(&c.to_string())).collect())
}

fn section(id: &str, kind: SectionKind, lyrics: &str) -> Section {
    Section { id: id.into(), kind, number: Some(1), name: None, color: None, lyrics: lyrics.into() }
}

fn song() -> Song {
    let current = SongVersion {
        id: "v1".into(),
        number: 1,
        sections: vec![section("a", SectionKind::Verse, "one"), section("b", SectionKind::Chorus, "two")],
        arrangement: vec!["a".into(), "b".into()],
        background: None,
    };
    Song { id: "grace".into(), title: "Amazing Grace".into(), archived: false, credits: Default::default(), key: None, tags: vec![], current }
}

fn round_trip_service(before: &Service, after: &Service) -> Vec<ServiceOp> {
    let change = Change::Service { id: before.id.clone(), ops: diff_service(before, after) };
    let mut forward = before.clone();
    assert_eq!(change.apply_to_service(&mut forward), 0);
    assert_eq!(&forward, after);
    let mut back = after.clone();
    assert_eq!(change.inverted().apply_to_service(&mut back), 0);
    assert_eq!(&back, before);
    let Change::Service { ops, .. } = change else { unreachable!() };
    ops
}

fn round_trip_song(before: &Song, after: &Song) -> Vec<SongOp> {
    let change = Change::Song { id: before.id.clone(), ops: diff_song(before, after) };
    let mut forward = before.clone();
    assert_eq!(change.apply_to_song(&mut forward), 0);
    assert_eq!(forward.title, after.title);
    assert!(forward.current.same_content(&after.current));
    let mut back = after.clone();
    assert_eq!(change.inverted().apply_to_song(&mut back), 0);
    assert!(back.current.same_content(&before.current));
    let Change::Song { ops, .. } = change else { unreachable!() };
    ops
}

#[test]
fn every_reordering_round_trips() {
    let cases = [("abcd", "dabc"), ("abcd", "bcda"), ("abcd", "dcba"), ("abcd", "axcd"), ("abcd", ""), ("", "xyz"), ("abcdef", "fxbdaey"), ("ab", "ab")];
    for (before, after) in cases {
        round_trip_service(&order(before), &order(after));
    }
}

#[test]
fn moving_one_item_is_one_operation() {
    assert_eq!(round_trip_service(&order("abcd"), &order("bcda")), vec![ServiceOp::MoveItem { id: "a".into(), from: None, to: Some("d".into()) }]);
    assert_eq!(round_trip_service(&order("abcd"), &order("dabc")).len(), 1);
}

#[test]
fn item_fields_are_set_one_by_one() {
    let before = service(vec![song_item("x"), text("t")]);
    let mut after = before.clone();
    after.title = "Sunday AM".into();
    after.items[1].notes = Some("slow".into());
    if let ItemContent::Song(song) = &mut after.items[0].content {
        song.key = Some("G".into());
        song.arrangement = Some(vec![Entry { id: "e".into(), section_id: Some("a".into()), variation: SectionPatch { lyrics: Some("new".into()), ..Default::default() } }]);
    }
    let ops = round_trip_service(&before, &after);
    let fields: Vec<String> = ops
        .iter()
        .map(|op| match op {
            ServiceOp::Set { field, .. } => format!("{field:?}"),
            ServiceOp::SetItem { id, field, .. } => format!("{id}.{field:?}"),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(fields, ["Title", "x.Key", "x.Arrangement", "t.Notes"]);
}

#[test]
fn songs_round_trip_and_ignore_the_version_id() {
    let before = song();
    let mut after = before.clone();
    after.current.id = "v2".into();
    after.current.number = 2;
    after.title = "Grace".into();
    after.current.sections[1].lyrics = "three".into();
    after.current.sections.insert(0, section("c", SectionKind::Bridge, "four"));
    after.current.arrangement = vec!["c".into(), "a".into(), "b".into(), "b".into()];
    let ops = round_trip_song(&before, &after);
    assert_eq!(ops.len(), 4);
    assert!(matches!(ops.last(), Some(SongOp::Set { field: SongField::Arrangement, .. })));
}

#[test]
fn changes_out_of_step_are_counted_not_refused() {
    let before = order("ab");
    let after = order("b");
    let change = Change::Service { id: "s".into(), ops: diff_service(&before, &after) };
    let mut elsewhere = order("ab");
    elsewhere.items[0].notes = Some("changed".into());
    assert_eq!(change.apply_to_service(&mut elsewhere), 1);
    assert_eq!(elsewhere.items.len(), 1);
}

#[test]
fn typing_in_one_field_merges() {
    let a = service(vec![]);
    let mut b = a.clone();
    b.title = "Sun".into();
    let mut c = b.clone();
    c.title = "Sunday night".into();
    let first = Change::Service { id: "s".into(), ops: diff_service(&a, &b) };
    let second = Change::Service { id: "s".into(), ops: diff_service(&b, &c) };
    let merged = first.merge(&second).unwrap();
    assert_eq!(merged, Change::Service { id: "s".into(), ops: vec![ServiceOp::Set { field: ServiceField::Title, before: json!("Sunday"), after: json!("Sunday night") }] });
    let mut d = c.clone();
    d.leader = Some("Ann".into());
    assert!(second.merge(&Change::Service { id: "s".into(), ops: diff_service(&c, &d) }).is_none());
}

#[test]
fn labels_name_what_changed() {
    let name = |id: &str| format!("“{id}”");
    let removed = Change::Service { id: "s".into(), ops: diff_service(&order("ab"), &order("b")) };
    assert_eq!(removed.describe(&name), "Remove “a”");
    let mut edited = song();
    edited.current.sections[0].lyrics = "changed".into();
    assert_eq!(Change::Song { id: "grace".into(), ops: diff_song(&song(), &edited) }.describe(&name), "Edit Verse 1");
    let mut archived = song();
    archived.archived = true;
    assert_eq!(Change::Song { id: "grace".into(), ops: diff_song(&song(), &archived) }.describe(&name), "Archive song");
}

#[test]
fn match_the_shared_fixtures() {
    let fixtures: Value = serde_json::from_str(include_str!("../../../../../fixtures/edits.json")).unwrap();
    for case in fixtures["services"].as_array().unwrap() {
        let before: Service = serde_json::from_value(case["before"].clone()).unwrap();
        let after: Service = serde_json::from_value(case["after"].clone()).unwrap();
        let ops = round_trip_service(&before, &after);
        assert_eq!(serde_json::to_value(&ops).unwrap(), case["ops"], "{}", case["name"]);
        assert_eq!(Change::Service { id: before.id.clone(), ops }.describe(&|id| id.to_string()), case["label"], "{}", case["name"]);
    }
    for case in fixtures["songs"].as_array().unwrap() {
        let before: Song = serde_json::from_value(case["before"].clone()).unwrap();
        let after: Song = serde_json::from_value(case["after"].clone()).unwrap();
        let ops = round_trip_song(&before, &after);
        assert_eq!(serde_json::to_value(&ops).unwrap(), case["ops"], "{}", case["name"]);
        assert_eq!(Change::Song { id: before.id.clone(), ops }.describe(&|id| id.to_string()), case["label"], "{}", case["name"]);
    }
}
