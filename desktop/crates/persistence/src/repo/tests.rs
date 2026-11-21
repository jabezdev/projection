use rusqlite::Connection;

use super::*;

use protocol::MediaKind;

#[test]
fn backups_hold_every_service_and_song() {
    let dir = std::env::temp_dir().join(format!("magi-backup-{}-{}", std::process::id(), line!()));
    std::fs::create_dir_all(&dir).unwrap();
    let repo = Repository::new(dir.join("app.db")).unwrap();
    repo.create_project("Sunday", 1).unwrap();
    repo.create_song("Song", "Lyrics").unwrap();
    let copy = dir.join("copy.db");
    repo.backup_to(&copy).unwrap();
    let restored = Repository::new(&copy).unwrap();
    assert_eq!(restored.list_projects().unwrap()[0].name, "Sunday");
    assert_eq!(restored.list_songs().unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn services_songs_and_media_can_be_deleted_and_media_renamed() {
    let repo = Repository::new(":memory:").unwrap();
    let project = repo.create_project("Sunday", 1).unwrap();
    let song = repo.create_song("Song", "Lyrics").unwrap();
    let media = repo.create_media("Clip".into(), MediaKind::Video, "/media/clip.mp4".into()).unwrap();
    repo.rename_media(media.id, "Welcome clip").unwrap();
    assert_eq!(repo.list_media().unwrap()[0].title, "Welcome clip");
    assert!(repo.delete_project(project.id).unwrap());
    assert!(repo.delete_song(song.id).unwrap());
    assert!(repo.delete_media(media.id).unwrap());
    assert!(!repo.delete_media(media.id).unwrap(), "already gone");
    assert!(repo.list_projects().unwrap().is_empty() && repo.list_songs().unwrap().is_empty() && repo.list_media().unwrap().is_empty());
    assert!(repo.rename_media(media.id, "x").is_err());
}

#[test]
fn media_catalog_retains_each_kind_and_original_path() {
    let repo = Repository::new(":memory:").unwrap();
    for (kind, path) in [
        (MediaKind::Video, "/media/welcome.mp4"),
        (MediaKind::Image, "/media/still.png"),
        (MediaKind::Audio, "/media/song.wav"),
    ] {
        let created = repo
            .create_media("A title".into(), kind.clone(), path.into())
            .unwrap();
        assert_eq!(created.kind, kind);
        assert!(repo
            .list_media()
            .unwrap()
            .iter()
            .any(|item| item == &created));
    }
    assert_eq!(repo.list_media().unwrap().len(), 3);
}

#[test]
fn upgrading_version_one_adds_media_without_losing_services() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE schema_migrations(version INTEGER NOT NULL); INSERT INTO schema_migrations VALUES (1);
        CREATE TABLE projects(id INTEGER PRIMARY KEY, name TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL, model_json TEXT);
        INSERT INTO projects VALUES (1, 'Existing service', 0, 0, NULL);").unwrap();
    let repo = Repository { conn };
    repo.init_schema().unwrap();
    assert_eq!(repo.get_project(1).unwrap().name, "Existing service");
    assert!(repo.list_media().unwrap().is_empty());
    repo.create_song("Song", "First\nsecond\n\nThird").unwrap();
    repo.create_media("Video".into(), MediaKind::Video, "/media/a.mp4".into())
        .unwrap();
    repo.init_schema().unwrap();
    assert_eq!(repo.list_media().unwrap().len(), 1);
    assert_eq!(repo.list_songs().unwrap().len(), 1);
}

#[test]
fn service_schedule_preserves_media_options_and_song_document_metadata() {
    let repo = Repository::new(":memory:").unwrap();
    let mut service = repo.create_project("Service", 1).unwrap();
    service.session.items.push(protocol::SessionItem {
        id: 10,
        title: Some("Audio".into()),
        kind: protocol::SessionItemKind::Audio {
            path: "/audio/test.wav".into(),
            options: protocol::VideoOptions { looping: true, volume: 40, ..Default::default() },
        },
        song: None,
        background: None,
    });
    service.session.items.push(protocol::SessionItem {
        id: 11,
        title: Some("Song".into()),
        kind: protocol::SessionItemKind::Text {
            steps: vec!["A\nB".into(), "C".into()],
        },
        song: Some("{\"slideSeparator\":\"blank-line\"}".into()),
        background: Some("/media/bg.png".into()),
    });
    service.session.items.push(protocol::SessionItem {
        id: 12,
        title: Some("Video".into()),
        kind: protocol::SessionItemKind::Video {
            path: "/video/clip.mp4".into(),
            options: protocol::VideoOptions {
                confidence: true, looping: true, start_ms: 1500,
                end_ms: Some(8000), volume: 75, rate_percent: 125,
            },
        },
        song: None, background: None,
    });
    repo.save_project(&mut service, 2).unwrap();
    assert_eq!(repo.get_project(service.id).unwrap(), service);
}
