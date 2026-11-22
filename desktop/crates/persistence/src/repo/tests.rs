use protocol::model::{new_id, Entry, ItemContent, MediaFile, MediaKind, Section, SectionKind, SectionPatch, Service, ServiceItem, Song, SongItem, SongVersion};
use protocol::{CaptureKind, CaptureOptions, CaptureSource, SessionItemKind};

use super::Repository;

pub(super) const DAY: i64 = 86_400_000;
pub(super) const NOV_1: i64 = 1_793_491_200_000;

pub(super) fn section(id: &str, kind: SectionKind, lyrics: &str) -> Section {
    Section { id: id.into(), kind, number: None, name: None, color: None, lyrics: lyrics.into() }
}

pub(super) fn grace() -> Song {
    let current = SongVersion {
        id: new_id(),
        number: 1,
        sections: vec![section("v", SectionKind::Verse, "line a\n\nline b"), section("c", SectionKind::Chorus, "how sweet")],
        arrangement: vec!["v".into(), "c".into(), "c".into()],
        background: None,
    };
    Song { id: new_id(), title: "Amazing Grace".into(), archived: false, credits: Default::default(), key: None, tags: vec![], current }
}

pub(super) fn item(content: ItemContent) -> ServiceItem {
    ServiceItem { id: new_id(), title: None, notes: None, background: None, content }
}

pub(super) fn song_item(song: &Song) -> ServiceItem {
    item(ItemContent::Song(SongItem { song_id: song.id.clone(), version_id: None, key: None, arrangement: None }))
}

pub(super) fn service(title: &str, scheduled_at_ms: i64, items: Vec<ServiceItem>) -> Service {
    Service { id: new_id(), scheduled_at_ms, title: title.into(), leader: None, notes: None, ended_at_ms: None, items }
}

pub(super) fn song_of(item: &ServiceItem) -> &SongItem {
    let ItemContent::Song(song) = &item.content else { panic!("not a song: {item:?}") };
    song
}

pub(super) fn chorus_in(repo: &Repository, service: &Service) -> String {
    let live = repo.resolve_service(&repo.service(&service.id).unwrap().unwrap()).unwrap();
    live.items[0].song.as_ref().unwrap().sections[1].lyrics.clone()
}

#[test]
fn backups_hold_every_service_and_song() {
    let dir = std::env::temp_dir().join(format!("palto-backup-{}", new_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let repo = Repository::new(dir.join("app.db")).unwrap();
    let song = grace();
    repo.insert_song(&song, 1).unwrap();
    repo.save_service(&service("Sunday", NOV_1, vec![song_item(&song)]), 1).unwrap();
    repo.backup_to(&dir.join("copy.db")).unwrap();
    let restored = Repository::new(dir.join("copy.db")).unwrap();
    assert_eq!(restored.list_services().unwrap()[0].title, "Sunday");
    assert_eq!(restored.list_library(false).unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn library_fixes_reach_upcoming_services_but_not_past_ones() {
    let repo = Repository::new(":memory:").unwrap();
    let song = grace();
    repo.insert_song(&song, 1).unwrap();
    let past = service("Last week", NOV_1, vec![song_item(&song)]);
    let upcoming = service("Next week", NOV_1 + 14 * DAY, vec![song_item(&song)]);
    repo.save_service(&past, 1).unwrap();
    repo.save_service(&upcoming, 1).unwrap();
    assert_eq!(repo.freeze_past_services(NOV_1 + 7 * DAY).unwrap(), 1);

    let mut sections = song.current.sections.clone();
    sections[1].lyrics = "fixed typo".into();
    let fixed = repo.edit_song_content(&song.id, sections.clone(), song.current.arrangement.clone(), None, 2).unwrap();
    assert_eq!(fixed.number, 2, "version 1 is pinned by last week");
    assert_eq!(chorus_in(&repo, &past), "how sweet");
    assert_eq!(chorus_in(&repo, &upcoming), "fixed typo");

    sections[1].lyrics = "fixed twice".into();
    let again = repo.edit_song_content(&song.id, sections, song.current.arrangement.clone(), None, 3).unwrap();
    assert_eq!((again.id.as_str(), again.number), (fixed.id.as_str(), 2), "unpinned versions change in place");
    assert_eq!(repo.freeze_past_services(NOV_1 + 7 * DAY).unwrap(), 0, "frozen once");
}

#[test]
fn ending_a_service_pins_its_songs_and_archived_songs_still_resolve() {
    let repo = Repository::new(":memory:").unwrap();
    let mut song = grace();
    repo.insert_song(&song, 1).unwrap();
    let sunday = service("Sunday", NOV_1, vec![song_item(&song)]);
    repo.save_service(&sunday, 1).unwrap();
    repo.end_service(&sunday.id, NOV_1 + 3_600_000).unwrap();
    let ended = repo.service(&sunday.id).unwrap().unwrap();
    assert_eq!(ended.ended_at_ms, Some(NOV_1 + 3_600_000));
    assert_eq!(song_of(&ended.items[0]).version_id.as_deref(), Some(song.current.id.as_str()));
    repo.save_service(&sunday, 2).unwrap();
    assert_eq!(song_of(&repo.service(&sunday.id).unwrap().unwrap().items[0]).version_id.as_deref(), Some(song.current.id.as_str()), "a stale copy cannot unpin a past service");

    song.archived = true;
    repo.update_song_details(&song, 2).unwrap();
    assert!(repo.list_library(false).unwrap().is_empty());
    assert_eq!(repo.list_library(true).unwrap().len(), 1);
    assert_eq!(chorus_in(&repo, &sunday), "how sweet");
    assert!(repo.delete_service(&sunday.id).unwrap());
    assert!(repo.service(&sunday.id).unwrap().is_none());
}

#[test]
fn services_resolve_variations_media_and_device_bindings() {
    let repo = Repository::new(":memory:").unwrap();
    let song = grace();
    repo.insert_song(&song, 1).unwrap();
    let clip = MediaFile { id: new_id(), title: "Welcome".into(), kind: MediaKind::Video, hash: None, path: Some("/media/welcome.mp4".into()) };
    repo.save_media_file(&clip).unwrap();
    let varied = item(ItemContent::Song(SongItem {
        song_id: song.id.clone(),
        version_id: None,
        key: Some("G".into()),
        arrangement: Some(vec![
            Entry { id: "e1".into(), section_id: Some("c".into()), variation: SectionPatch { lyrics: Some("softly\n\nagain".into()), ..Default::default() } },
            Entry { id: "e2".into(), section_id: None, variation: SectionPatch { kind: Some(SectionKind::Tag), lyrics: Some("amen".into()), ..Default::default() } },
        ]),
    }));
    let capture = CaptureSource { kind: CaptureKind::Screen, id: "1".into(), name: "Main".into() };
    let mut video = item(ItemContent::Media { media_id: clip.id.clone(), options: Default::default() });
    video.background = Some("shader:aurora".into());
    let sunday = service("Sunday", NOV_1, vec![varied, video, item(ItemContent::Capture { options: CaptureOptions::default(), source: Some(capture) }), item(ItemContent::Header)]);
    repo.save_service(&sunday, 1).unwrap();

    let loaded = repo.service(&sunday.id).unwrap().unwrap();
    assert_eq!(loaded, sunday, "device-only fields come back on this machine");
    let bodies: Vec<String> = repo.conn.prepare("SELECT body FROM service_item").unwrap().query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect();
    assert!(bodies.iter().all(|b| !b.contains("Main")), "capture sources stay out of the shared item");
    let live = repo.resolve_service(&loaded).unwrap();
    assert_eq!(live.items[0].title.as_deref(), Some("Amazing Grace"));
    assert_eq!(live.items[0].kind, SessionItemKind::Text { steps: vec!["softly".into(), "again".into(), "amen".into()] });
    assert_eq!(live.items[1].kind, SessionItemKind::Video { path: "/media/welcome.mp4".into(), options: Default::default() });
    assert_eq!(live.items[1].background.as_deref(), Some("shader:aurora"));
    assert!(matches!(&live.items[2].kind, SessionItemKind::Capture { source, .. } if source.name == "Main"));
    assert_eq!(live.items[3].kind, SessionItemKind::Section {});

    assert_eq!(repo.relink_media_path("/media/welcome.mp4", "/new/welcome.mp4").unwrap(), 1);
    assert_eq!(repo.media_file_at("/new/welcome.mp4").unwrap().unwrap().id, clip.id);
    assert!(repo.delete_media_file(&clip.id).unwrap());
    assert_eq!(repo.resolve_service(&loaded).unwrap().items[1].kind, SessionItemKind::Image { path: String::new() }, "a missing file shows nothing");
}

#[test]
fn an_old_database_starts_over_with_the_new_tables() {
    let dir = std::env::temp_dir().join(format!("palto-old-{}", new_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE schema_migrations (version INTEGER NOT NULL); INSERT INTO schema_migrations VALUES (2);
            CREATE TABLE songs (id INTEGER PRIMARY KEY, title TEXT NOT NULL, lyrics TEXT NOT NULL); INSERT INTO songs(title, lyrics) VALUES ('Old', 'x');").unwrap();
    }
    let repo = Repository::new(&path).unwrap();
    assert!(repo.list_library(true).unwrap().is_empty());
    repo.insert_song(&grace(), 1).unwrap();
    drop(repo);
    assert_eq!(Repository::new(&path).unwrap().list_library(true).unwrap().len(), 1, "reopening keeps the data");
    std::fs::remove_dir_all(dir).unwrap();
}
