use protocol::model::{ItemContent, Service};

use super::tests::{grace, item, service, song_item, DAY, NOV_1};
use super::{History, Repository};

fn titles(repo: &Repository, service: &Service) -> Vec<String> {
    repo.service(&service.id).unwrap().unwrap().items.iter().map(|i| i.title.clone().unwrap_or_default()).collect()
}

fn titled(title: &str) -> protocol::model::ServiceItem {
    let mut item = item(ItemContent::Header);
    item.title = Some(title.into());
    item
}

fn labels(undo: Option<&str>, redo: Option<&str>) -> History {
    History { undo: undo.map(Into::into), redo: redo.map(Into::into) }
}

#[test]
fn service_edits_undo_and_redo_in_order() {
    let repo = Repository::new(":memory:").unwrap();
    let mut sunday = service("Sunday", NOV_1, vec![titled("Welcome")]);
    repo.save_service(&sunday, 1_000).unwrap();
    assert_eq!(repo.history(&sunday.id).unwrap(), History::default(), "creating is not an edit");

    sunday.items.push(titled("Offering"));
    repo.save_service(&sunday, 10_000).unwrap();
    sunday.items.remove(0);
    repo.save_service(&sunday, 20_000).unwrap();
    assert_eq!(repo.history(&sunday.id).unwrap(), labels(Some("Remove Welcome"), None));

    assert_eq!(repo.undo_service(&sunday.id, 30_000).unwrap().unwrap().items.len(), 2);
    assert_eq!(titles(&repo, &sunday), ["Welcome", "Offering"]);
    assert_eq!(repo.history(&sunday.id).unwrap(), labels(Some("Add Offering"), Some("Remove Welcome")));
    repo.undo_service(&sunday.id, 31_000).unwrap();
    assert_eq!(titles(&repo, &sunday), ["Welcome"]);
    assert!(repo.undo_service(&sunday.id, 32_000).unwrap().is_none());

    repo.redo_service(&sunday.id, 33_000).unwrap();
    assert_eq!(titles(&repo, &sunday), ["Welcome", "Offering"]);
    assert_eq!(repo.history(&sunday.id).unwrap(), labels(Some("Add Offering"), Some("Remove Welcome")));

    let mut current = repo.service(&sunday.id).unwrap().unwrap();
    current.title = "Sunday AM".into();
    repo.save_service(&current, 40_000).unwrap();
    assert_eq!(repo.history(&sunday.id).unwrap(), labels(Some("Rename service"), None), "a new edit drops the redo steps");

    let log = repo.edit_log(&sunday.id).unwrap();
    assert_eq!(log.len(), 6, "undos and redos are recorded as edits of their own");
    assert_eq!(log.iter().filter(|s| s.reverts.is_some()).count(), 2);
}

#[test]
fn quick_edits_to_one_field_merge_into_one_step() {
    let repo = Repository::new(":memory:").unwrap();
    let mut sunday = service("S", NOV_1, vec![]);
    repo.save_service(&sunday, 1_000).unwrap();
    for (title, at) in [("Su", 10_000), ("Sun", 10_400), ("Sunday", 10_900)] {
        sunday.title = title.into();
        repo.save_service(&sunday, at).unwrap();
    }
    sunday.title = "Sunday AM".into();
    repo.save_service(&sunday, 13_000).unwrap();
    assert_eq!(repo.edit_log(&sunday.id).unwrap().len(), 2);
    repo.undo_service(&sunday.id, 14_000).unwrap();
    assert_eq!(repo.service(&sunday.id).unwrap().unwrap().title, "Sunday");
    repo.undo_service(&sunday.id, 15_000).unwrap();
    assert_eq!(repo.service(&sunday.id).unwrap().unwrap().title, "S");
}

#[test]
fn past_services_have_no_history() {
    let repo = Repository::new(":memory:").unwrap();
    let mut past = service("Last week", NOV_1, vec![]);
    repo.save_service(&past, 1).unwrap();
    past.title = "Renamed".into();
    repo.save_service(&past, 10_000).unwrap();
    assert!(repo.history(&past.id).unwrap().undo.is_some());
    repo.freeze_past_services(NOV_1 + 7 * DAY).unwrap();
    assert_eq!(repo.history(&past.id).unwrap(), History::default());
    assert!(repo.edit_log(&past.id).unwrap().is_empty());
    past.title = "Again".into();
    repo.save_service(&past, 20_000).unwrap();
    assert!(repo.undo_service(&past.id, 30_000).unwrap().is_none());
}

#[test]
fn song_edits_undo_even_across_a_new_version() {
    let repo = Repository::new(":memory:").unwrap();
    let song = grace();
    repo.insert_song(&song, 1).unwrap();
    repo.save_service(&service("Last week", NOV_1, vec![song_item(&song)]), 1).unwrap();
    repo.freeze_past_services(NOV_1 + 7 * DAY).unwrap();

    let mut edited = song.clone();
    edited.current.sections[1].lyrics = "fixed".into();
    edited.title = "Grace".into();
    let saved = repo.save_song(&edited, 10_000).unwrap();
    assert_eq!(saved.current.number, 2);
    assert_eq!(repo.history(&song.id).unwrap().undo.as_deref(), Some("Edit song"));

    let restored = repo.undo_song(&song.id, 20_000).unwrap().unwrap();
    assert_eq!(restored.title, "Amazing Grace");
    assert!(restored.current.same_content(&song.current));
    assert_eq!(repo.history(&song.id).unwrap().redo.as_deref(), Some("Edit song"));
    assert_eq!(repo.redo_song(&song.id, 30_000).unwrap().unwrap().current.sections[1].lyrics, "fixed");
}

#[test]
fn archiving_is_undoable_and_saving_details_keeps_the_version() {
    let repo = Repository::new(":memory:").unwrap();
    let song = grace();
    repo.insert_song(&song, 1).unwrap();
    repo.save_service(&service("Last week", NOV_1, vec![song_item(&song)]), 1).unwrap();
    repo.freeze_past_services(NOV_1 + 7 * DAY).unwrap();
    let mut archived = song.clone();
    archived.archived = true;
    assert_eq!(repo.save_song(&archived, 10_000).unwrap().current.id, song.current.id, "details alone never start a version");
    assert_eq!(repo.history(&song.id).unwrap().undo.as_deref(), Some("Archive song"));
    assert!(!repo.undo_song(&song.id, 20_000).unwrap().unwrap().archived);
}

#[test]
fn old_steps_fold_into_the_saved_record() {
    let repo = Repository::new(":memory:").unwrap();
    let mut sunday = service("0", NOV_1, vec![]);
    repo.save_service(&sunday, 0).unwrap();
    for i in 1..=105 {
        sunday.title = i.to_string();
        repo.save_service(&sunday, i * 10_000).unwrap();
    }
    assert_eq!(repo.edit_log(&sunday.id).unwrap().len(), 100);
    assert!(repo.compacted_through(&sunday.id).unwrap().is_some());
    for i in 0..100 {
        assert!(repo.undo_service(&sunday.id, 2_000_000 + i).unwrap().is_some());
    }
    assert!(repo.undo_service(&sunday.id, 3_000_000).unwrap().is_none());
    assert_eq!(repo.service(&sunday.id).unwrap().unwrap().title, "5");
}

#[test]
fn saving_rewrites_only_changed_items_and_keeps_device_bindings() {
    let repo = Repository::new(":memory:").unwrap();
    let capture = item(ItemContent::Capture { options: Default::default(), source: Some(protocol::CaptureSource { kind: protocol::CaptureKind::Screen, id: "1".into(), name: "Main".into() }) });
    let mut sunday = service("Sunday", NOV_1, vec![capture, titled("A"), titled("B")]);
    repo.save_service(&sunday, 1).unwrap();
    sunday.items.swap(1, 2);
    sunday.items.remove(0);
    repo.save_service(&sunday, 10_000).unwrap();
    assert_eq!(repo.service(&sunday.id).unwrap().unwrap(), sunday);
    let bindings: i64 = repo.conn.query_row("SELECT COUNT(*) FROM device_binding", [], |row| row.get(0)).unwrap();
    assert_eq!(bindings, 0);
    let undone = repo.undo_service(&sunday.id, 20_000).unwrap().unwrap();
    assert!(matches!(&undone.items[0].content, ItemContent::Capture { source: Some(s), .. } if s.name == "Main"));
}
