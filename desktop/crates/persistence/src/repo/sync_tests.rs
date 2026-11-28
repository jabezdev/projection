use protocol::model::{new_id, ItemContent, ServiceItem, ServiceTemplate, DEFAULT_TEMPLATE_ID};
use protocol::sync::{Deleted, PullPage, RecordKind, RemoteService, RemoteSong};
use protocol::{CaptureKind, CaptureOptions, CaptureSource};

use super::tests::{grace, item, service, song_item, NOV_1};
use super::Repository;

fn repo() -> Repository {
    Repository::new(":memory:").unwrap()
}

fn page() -> PullPage {
    PullPage { cursor: 1.0, more: false, songs: vec![], services: vec![], templates: vec![], media: vec![], deleted: vec![], pinned: vec![] }
}

#[test]
fn the_sunday_template_is_seeded_once_and_can_be_deleted_for_good() {
    let repo = repo();
    repo.seed_default_template(0).unwrap();
    assert_eq!(repo.list_templates().unwrap()[0].id, DEFAULT_TEMPLATE_ID);
    assert!(repo.delete_template(DEFAULT_TEMPLATE_ID).unwrap());
    repo.seed_default_template(0).unwrap();
    assert!(repo.list_templates().unwrap().is_empty());
}

#[test]
fn local_edits_queue_until_pushed_and_new_edits_after_a_push_queue_again() {
    let repo = repo();
    let svc = service("Sunday", NOV_1, vec![]);
    repo.save_service(&svc, 0).unwrap();
    let dirty = repo.dirty_records().unwrap();
    assert_eq!(dirty.len(), 1);
    assert_eq!(dirty[0].base, None);
    repo.save_service(&protocol::model::Service { title: "Sunday AM".into(), ..svc.clone() }, 1).unwrap();
    let base = serde_json::to_value(&svc).unwrap();
    repo.pushed(RecordKind::Service, &svc.id, dirty[0].version, &base).unwrap();
    assert_eq!(repo.dirty_records().unwrap().len(), 1, "the rename came after the push began");
    let again = repo.dirty_records().unwrap();
    repo.pushed(RecordKind::Service, &svc.id, again[0].version, &base).unwrap();
    assert!(repo.dirty_records().unwrap().is_empty());
}

#[test]
fn deleting_a_synced_record_queues_a_delete_but_a_never_synced_one_does_not() {
    let repo = repo();
    let synced = service("A", NOV_1, vec![]);
    let local = service("B", NOV_1, vec![]);
    repo.save_service(&synced, 0).unwrap();
    repo.save_service(&local, 0).unwrap();
    repo.pushed(RecordKind::Service, &synced.id, 1, &serde_json::to_value(&synced).unwrap()).unwrap();
    repo.delete_service(&synced.id).unwrap();
    repo.delete_service(&local.id).unwrap();
    assert_eq!(repo.deleted_records().unwrap(), vec![(RecordKind::Service, synced.id.clone())]);
}

#[test]
fn a_pull_updates_clean_records_keeps_device_fields_and_skips_records_with_unpushed_edits() {
    let repo = repo();
    let song = grace();
    repo.insert_song(&song, 0).unwrap();
    let mut capture = item(ItemContent::Capture { options: CaptureOptions::default(), source: Some(CaptureSource { kind: CaptureKind::Screen, id: "screen:1".into(), name: "Main".into() }) });
    capture.title = Some("Laptop".into());
    let svc = service("Sunday", NOV_1, vec![song_item(&song), capture.clone()]);
    repo.save_service(&svc, 0).unwrap();
    for record in repo.dirty_records().unwrap() {
        repo.pushed(record.kind, &record.id, record.version, &serde_json::Value::Null).unwrap();
    }
    let mut remote = repo.wire_service(&svc.id).unwrap().unwrap();
    remote.title = "Sunday Service".into();
    remote.items[1].title = Some("Pastor's laptop".into());
    let mut remote_song = song.clone();
    remote_song.title = "Amazing Grace (My Chains)".into();
    let mut pull = page();
    pull.services.push(RemoteService { service: remote, frozen: false });
    pull.songs.push(RemoteSong { song: remote_song, versions: vec![] });
    let applied = repo.apply_pull(&pull, 5).unwrap();
    assert_eq!(applied.changed.len(), 2);
    let local = repo.service(&svc.id).unwrap().unwrap();
    assert_eq!(local.title, "Sunday Service");
    let ItemContent::Capture { source, .. } = &local.items[1].content else { panic!() };
    assert_eq!(source.as_ref().map(|s| s.id.as_str()), Some("screen:1"));
    assert_eq!(repo.song(&song.id).unwrap().unwrap().title, "Amazing Grace (My Chains)");
    assert!(repo.dirty_records().unwrap().is_empty(), "applying a pull is not a local edit");

    repo.save_service(&protocol::model::Service { title: "Mine".into(), ..local }, 6).unwrap();
    let mut again = page();
    let mut theirs = repo.wire_service(&svc.id).unwrap().unwrap();
    theirs.title = "Theirs".into();
    again.services.push(RemoteService { service: theirs, frozen: false });
    assert_eq!(repo.apply_pull(&again, 7).unwrap().skipped, 1);
    assert_eq!(repo.service(&svc.id).unwrap().unwrap().title, "Mine");
}

#[test]
fn a_remote_delete_removes_clean_records_and_spares_edited_ones() {
    let repo = repo();
    let template = ServiceTemplate { id: new_id(), name: "Youth".into(), title: None, weekday: None, start_minutes: None, parts: vec![] };
    repo.save_template(&template, 0).unwrap();
    let svc = service("Sunday", NOV_1, vec![]);
    repo.save_service(&svc, 0).unwrap();
    let clean = repo.dirty_records().unwrap().into_iter().find(|r| r.kind == RecordKind::Template).unwrap();
    repo.pushed(clean.kind, &clean.id, clean.version, &serde_json::to_value(&template).unwrap()).unwrap();
    let mut pull = page();
    pull.deleted = vec![Deleted { kind: RecordKind::Template, id: template.id.clone() }, Deleted { kind: RecordKind::Service, id: svc.id.clone() }];
    repo.apply_pull(&pull, 1).unwrap();
    assert!(repo.template(&template.id).unwrap().is_none());
    assert!(repo.service(&svc.id).unwrap().is_some());
    let dirty = repo.dirty_records().unwrap();
    assert_eq!(dirty.len(), 1);
    assert_eq!(dirty[0].base, None, "it goes back up as a new record");
}

#[test]
fn template_items_keep_this_computers_slides_while_the_shared_copy_leaves_them_out() {
    let repo = repo();
    let slides = ServiceItem { id: new_id(), title: Some("Offering".into()), notes: None, background: None, content: ItemContent::Presentation { media_id: None, slides: vec!["/media/offering.png".into()] } };
    let mut template = ServiceTemplate::sunday();
    template.parts[3].items.push(slides.clone());
    repo.save_template(&template, 0).unwrap();
    let ItemContent::Presentation { slides: wire, .. } = &repo.wire_template("sunday").unwrap().unwrap().parts[3].items[0].content else { panic!() };
    assert!(wire.is_empty());
    let mut remote = repo.wire_template("sunday").unwrap().unwrap();
    remote.name = "Sunday AM".into();
    repo.pushed(RecordKind::Template, "sunday", 1, &serde_json::Value::Null).unwrap();
    let mut pull = page();
    pull.templates.push(remote);
    repo.apply_pull(&pull, 1).unwrap();
    let local = repo.template("sunday").unwrap().unwrap();
    assert_eq!(local.name, "Sunday AM");
    let ItemContent::Presentation { slides: kept, .. } = &local.parts[3].items[0].content else { panic!() };
    assert_eq!(kept, &vec!["/media/offering.png".to_string()]);
    let service = local.instantiate(&local.default_parts(), None, 0);
    repo.save_service(&service, 2).unwrap();
    let saved = repo.service(&service.id).unwrap().unwrap();
    assert!(saved.items.iter().any(|i| matches!(&i.content, ItemContent::Presentation { slides, .. } if !slides.is_empty())));
}
