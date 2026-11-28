use std::sync::{Arc, Mutex};

use persistence::Repository;
use protocol::model::{new_id, ItemContent, Section, SectionKind, Service, ServiceItem, ServiceTemplate, Song, SongItem, SongVersion};
use serde_json::json;

use super::client::Convex;
use super::cycle;
use crate::Db;

fn computer() -> Db {
    Db(Arc::new(Mutex::new(Repository::new(":memory:").unwrap())))
}

fn song() -> Song {
    let current = SongVersion { id: new_id(), number: 1, sections: vec![Section { id: "v".into(), kind: SectionKind::Verse, number: Some(1), name: None, color: None, lyrics: "Amazing grace".into() }], arrangement: vec!["v".into()], background: None };
    Song { id: new_id(), title: "Amazing Grace".into(), archived: false, credits: Default::default(), key: Some("G".into()), tags: vec![], current }
}

#[tokio::test]
#[ignore = "needs PALTO_SYNC_URL and PALTO_SYNC_CODE from a running Convex backend"]
async fn two_computers_share_songs_services_and_templates_through_the_team_library() {
    let url = std::env::var("PALTO_SYNC_URL").unwrap();
    let code = std::env::var("PALTO_SYNC_CODE").unwrap();
    let convex = Convex::new(&url).unwrap();
    let claimed: serde_json::Value = convex.mutation("devices:claimPairingCode", json!({ "code": code, "deviceName": "Test PC" })).await.unwrap();
    let token = claimed["token"].as_str().unwrap().to_string();
    let (a, b) = (computer(), computer());

    let grace = song();
    let template = ServiceTemplate::sunday();
    let mut service = template.instantiate(&template.default_parts(), Some("Sunday Test"), 1_795_000_000_000);
    service.items.insert(3, ServiceItem { id: new_id(), title: None, notes: None, background: None, content: ItemContent::Song(SongItem { song_id: grace.id.clone(), version_id: None, key: None, arrangement: None }) });
    {
        let a = a.lock().unwrap();
        a.seed_default_template(0).unwrap();
        a.insert_song(&grace, 0).unwrap();
        a.save_service(&service, 0).unwrap();
        a.mark_everything_changed().unwrap();
    }
    let first = cycle::run(&convex, &token, &a, vec![]).await.unwrap();
    assert_eq!(first.conflicts, 0, "the Sunday template is the same on both sides");
    assert_eq!(a.lock().unwrap().pending_count().unwrap(), 0);

    b.lock().unwrap().seed_default_template(0).unwrap();
    cycle::run(&convex, &token, &b, vec![]).await.unwrap();
    let on_b = b.lock().unwrap().service(&service.id).unwrap().expect("the service reached the other computer");
    assert_eq!(on_b.items.len(), service.items.len());
    assert_eq!(b.lock().unwrap().song(&grace.id).unwrap().unwrap().current.sections[0].lyrics, "Amazing grace");

    let on_a = a.lock().unwrap().service(&service.id).unwrap().unwrap();
    a.lock().unwrap().save_service(&Service { notes: Some("Bring candles".into()), title: "Mine".into(), ..on_a }, 1).unwrap();
    b.lock().unwrap().save_service(&Service { title: "Theirs".into(), ..on_b.clone() }, 1).unwrap();
    cycle::run(&convex, &token, &b, vec![]).await.unwrap();
    let merged = cycle::run(&convex, &token, &a, vec![]).await.unwrap();
    assert_eq!(merged.conflicts, 1, "both renamed the service");
    let on_a = a.lock().unwrap().service(&service.id).unwrap().unwrap();
    assert_eq!((on_a.title.as_str(), on_a.notes.as_deref()), ("Theirs", Some("Bring candles")));

    let conflict = a.lock().unwrap().conflicts().unwrap().remove(0);
    cycle::run(&convex, &token, &a, vec![(conflict.id.clone(), conflict.keep_mine.clone())]).await.unwrap();
    assert_eq!(a.lock().unwrap().service(&service.id).unwrap().unwrap().title, "Mine");
    assert!(a.lock().unwrap().conflicts().unwrap().is_empty());
    cycle::run(&convex, &token, &b, vec![]).await.unwrap();
    assert_eq!(b.lock().unwrap().service(&service.id).unwrap().unwrap().title, "Mine");

    a.lock().unwrap().end_service(&service.id, 2).unwrap();
    let mut edited = a.lock().unwrap().song(&grace.id).unwrap().unwrap();
    edited.current.sections[0].lyrics = "Amazing grace, how sweet".into();
    a.lock().unwrap().save_song(&edited, 3).unwrap();
    cycle::run(&convex, &token, &a, vec![]).await.unwrap();
    cycle::run(&convex, &token, &b, vec![]).await.unwrap();
    let b_song = b.lock().unwrap().song(&grace.id).unwrap().unwrap();
    assert_eq!(b_song.current.sections[0].lyrics, "Amazing grace, how sweet");
    let b_service = b.lock().unwrap().service(&service.id).unwrap().unwrap();
    let pinned = b_service.items.iter().find_map(|i| match &i.content { ItemContent::Song(s) => s.version_id.clone(), _ => None }).expect("the past service pins a version");
    assert_eq!(b.lock().unwrap().song_version(&pinned).unwrap().unwrap().sections[0].lyrics, "Amazing grace");

    b.lock().unwrap().delete_service(&service.id).unwrap();
    cycle::run(&convex, &token, &b, vec![]).await.unwrap();
    cycle::run(&convex, &token, &a, vec![]).await.unwrap();
    assert!(a.lock().unwrap().service(&service.id).unwrap().is_none(), "the delete reached the first computer");
}
