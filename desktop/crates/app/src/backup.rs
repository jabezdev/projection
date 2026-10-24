//! Backups: one zip with a consistent copy of the library database, the
//! settings files, and `media.json` listing every media file the library
//! uses (with whether it still exists). Media files themselves are not
//! included; they can be large and live outside the data folder until the
//! library copies them in (roadmap phase 3).
//!
//! The operator exports a backup on demand. Once a week the app also writes
//! one into `backups/` and keeps the newest few.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Context;
use serde::Serialize;

use persistence::Repository;
use protocol::SessionItemKind;

/// Settings files copied into every backup when they exist.
const SETTINGS_FILES: [&str; 4] = ["app-config.json", "output-settings.json", "output-displays.json", "input-bindings.json"];
pub const FOLDER: &str = "backups";
const AUTO_PREFIX: &str = "auto-";
const AUTO_EVERY: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const AUTO_KEEP: usize = 5;

#[derive(Debug, Serialize, PartialEq)]
pub struct MediaEntry {
    pub path: String,
    /// Library title, for files imported into the media library.
    pub title: Option<String>,
    pub exists: bool,
    pub bytes: Option<u64>,
    /// Services that use the file (as an item, slide or background).
    pub services: Vec<String>,
}

/// Every media file the library knows about or a service uses, plus the logo.
pub fn media_list(db: &Repository, logo_path: &str) -> anyhow::Result<Vec<MediaEntry>> {
    // (path, library title, service using it)
    let mut uses: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    for media in db.list_media()? {
        uses.push((media.path, Some(media.title), None));
    }
    for header in db.list_projects()? {
        let project = db.get_project(header.id)?;
        for item in &project.session.items {
            let mut paths: Vec<&str> = item.background.as_deref().into_iter().collect();
            match &item.kind {
                SessionItemKind::Image { path } | SessionItemKind::Video { path, .. } | SessionItemKind::Audio { path, .. } => paths.push(path),
                SessionItemKind::Presentation { path, slides } => {
                    paths.push(path);
                    paths.extend(slides.iter().map(String::as_str));
                }
                SessionItemKind::Text { .. } | SessionItemKind::Capture { .. } => {}
            }
            for path in paths.into_iter().filter(|p| !p.is_empty()) {
                uses.push((path.to_string(), None, Some(project.name.clone())));
            }
        }
    }
    if !logo_path.is_empty() {
        uses.push((logo_path.to_string(), Some("Logo".into()), None));
    }
    let mut entries: BTreeMap<String, MediaEntry> = BTreeMap::new();
    for (path, title, service) in uses {
        let metadata = std::fs::metadata(&path).ok().filter(|m| m.is_file());
        let entry = entries.entry(path.clone()).or_insert_with(|| MediaEntry { path, title: None, exists: metadata.is_some(), bytes: metadata.map(|m| m.len()), services: Vec::new() });
        if entry.title.is_none() {
            entry.title = title;
        }
        if let Some(service) = service.filter(|s| !entry.services.contains(s)) {
            entry.services.push(service);
        }
    }
    Ok(entries.into_values().collect())
}

/// Writes a backup zip to `dest`, replacing it only once complete.
pub fn write(db: &Repository, data_dir: &Path, logo_path: &str, dest: &Path) -> anyhow::Result<()> {
    let work = data_dir.join(FOLDER);
    std::fs::create_dir_all(&work)?;
    let database = work.join(format!(".tmp-{}.db", uuid::Uuid::new_v4()));
    let result = (|| {
        db.backup_to(&database)?;
        let media = media_list(db, logo_path)?;
        let partial = dest.with_extension("zip.partial");
        {
            let file = std::fs::File::create(&partial).with_context(|| format!("create {}", partial.display()))?;
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("app.db", options)?;
            std::io::copy(&mut std::fs::File::open(&database)?, &mut zip)?;
            for name in SETTINGS_FILES {
                if let Ok(bytes) = std::fs::read(data_dir.join(name)) {
                    zip.start_file(name, options)?;
                    zip.write_all(&bytes)?;
                }
            }
            zip.start_file("media.json", options)?;
            zip.write_all(&serde_json::to_vec_pretty(&media)?)?;
            let manifest = serde_json::json!({
                "app": "PALTO",
                "version": env!("CARGO_PKG_VERSION"),
                "created": chrono::Local::now().to_rfc3339(),
                "missing_media": media.iter().filter(|m| !m.exists).count(),
            });
            zip.start_file("manifest.json", options)?;
            zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
            zip.finish()?;
        }
        std::fs::rename(&partial, dest).with_context(|| format!("write {}", dest.display()))?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&database);
    result
}

/// The suggested name for an exported backup.
pub fn export_name() -> String {
    format!("PALTO backup {}.zip", chrono::Local::now().format("%Y-%m-%d %H%M"))
}

/// Automatic backups in `dir`, newest first, with their modification times.
fn automatic(dir: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut found: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with(AUTO_PREFIX) && n.ends_with(".zip")))
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
        .collect();
    found.sort_by(|a, b| b.1.cmp(&a.1));
    found
}

/// When the newest automatic backup was written, if any.
pub fn last_automatic(data_dir: &Path) -> Option<SystemTime> {
    automatic(&data_dir.join(FOLDER)).first().map(|(_, at)| *at)
}

/// Writes an automatic backup when the last one is a week old (or missing),
/// then keeps only the newest few. Returns the new backup's path.
pub fn automatic_if_due(db: &Repository, data_dir: &Path, logo_path: &str, now: SystemTime) -> anyhow::Result<Option<PathBuf>> {
    let dir = data_dir.join(FOLDER);
    if last_automatic(data_dir).is_some_and(|at| now.duration_since(at).unwrap_or_default() < AUTO_EVERY) {
        return Ok(None);
    }
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{AUTO_PREFIX}{}.zip", chrono::Local::now().format("%Y-%m-%d-%H%M%S")));
    write(db, data_dir, logo_path, &path)?;
    for (old, _) in automatic(&dir).into_iter().skip(AUTO_KEEP) {
        let _ = std::fs::remove_file(old);
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{SessionItem, VideoOptions};
    use std::io::Read;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("magi-backups-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_backup_holds_the_database_settings_and_media_list() {
        let dir = temp_dir();
        let present = dir.join("still.png");
        std::fs::write(&present, b"png").unwrap();
        std::fs::write(dir.join("output-settings.json"), r#"{"size":5}"#).unwrap();
        let db = Repository::new(dir.join("app.db")).unwrap();
        db.create_media("Still".into(), protocol::MediaKind::Image, present.to_string_lossy().into()).unwrap();
        let mut service = db.create_project("Sunday", 1).unwrap();
        service.session.items.push(SessionItem { id: 1, title: None, kind: SessionItemKind::Video { path: "/gone/clip.mp4".into(), options: VideoOptions::default() }, song: None, background: Some(present.to_string_lossy().into()) });
        db.save_project(&mut service, 2).unwrap();

        let dest = dir.join("out.zip");
        write(&db, &dir, "", &dest).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        let mut names: Vec<String> = zip.file_names().map(|name| name.unwrap().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["app.db", "manifest.json", "media.json", "output-settings.json"]);
        let mut media = String::new();
        zip.by_name("media.json").unwrap().read_to_string(&mut media).unwrap();
        let media: serde_json::Value = serde_json::from_str(&media).unwrap();
        let entries = media.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        let still = entries.iter().find(|e| e["path"] == present.to_string_lossy().as_ref()).unwrap();
        assert_eq!((still["title"].as_str(), still["exists"].as_bool(), still["services"][0].as_str()), (Some("Still"), Some(true), Some("Sunday")));
        let gone = entries.iter().find(|e| e["path"] == "/gone/clip.mp4").unwrap();
        assert_eq!(gone["exists"], false);
        // No temporary database is left behind.
        assert_eq!(std::fs::read_dir(dir.join(FOLDER)).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn automatic_backups_are_weekly_and_pruned() {
        let dir = temp_dir();
        let db = Repository::new(dir.join("app.db")).unwrap();
        let now = SystemTime::now();
        assert!(automatic_if_due(&db, &dir, "", now).unwrap().is_some());
        assert!(automatic_if_due(&db, &dir, "", now + Duration::from_secs(60)).unwrap().is_none());
        assert!(automatic_if_due(&db, &dir, "", now + AUTO_EVERY + Duration::from_secs(1)).unwrap().is_some());

        for index in 0..AUTO_KEEP + 2 {
            std::fs::write(dir.join(FOLDER).join(format!("{AUTO_PREFIX}old-{index}.zip")), b"").unwrap();
        }
        automatic_if_due(&db, &dir, "", now + AUTO_EVERY * 3).unwrap();
        assert_eq!(automatic(&dir.join(FOLDER)).len(), AUTO_KEEP);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
