//! Library files. Imported media is copied into `<data>/media/`, so the
//! library is self-contained and travels with backups and (later) sync.
//! Media that went missing can be relinked by searching a folder for files
//! with the same names.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MEDIA_FOLDER: &str = "media";
/// How deep a relink search looks below the chosen folder.
const SEARCH_DEPTH: usize = 6;

/// Copies `source` into the library's media folder and returns the copy.
/// A file already in the folder is used as it is; an identical file already
/// imported is reused; a different file with the same name gets " (2)", " (3)"….
pub fn import_file(data_dir: &Path, source: &Path) -> std::io::Result<PathBuf> {
    let folder = data_dir.join(MEDIA_FOLDER);
    std::fs::create_dir_all(&folder)?;
    let folder = std::fs::canonicalize(&folder)?;
    let source = std::fs::canonicalize(source)?;
    if source.starts_with(&folder) {
        return Ok(source);
    }
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("media").to_string();
    let extension = source.extension().and_then(|s| s.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    for attempt in 1.. {
        let name = if attempt == 1 { format!("{stem}{extension}") } else { format!("{stem} ({attempt}){extension}") };
        let target = folder.join(&name);
        if target.exists() {
            if same_contents(&source, &target)? {
                return Ok(target);
            }
            continue;
        }
        // Copied under a temporary name, so an interrupted copy is never used.
        let partial = folder.join(format!("{name}.partial"));
        if let Err(e) = std::fs::copy(&source, &partial).and_then(|_| std::fs::rename(&partial, &target)) {
            let _ = std::fs::remove_file(&partial);
            return Err(e);
        }
        return Ok(target);
    }
    unreachable!("the loop returns")
}

fn same_contents(a: &Path, b: &Path) -> std::io::Result<bool> {
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (std::io::BufReader::new(std::fs::File::open(a)?), std::io::BufReader::new(std::fs::File::open(b)?));
    let (mut left, mut right) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let read = a.read(&mut left)?;
        if read == 0 {
            return Ok(true);
        }
        b.read_exact(&mut right[..read])?;
        if left[..read] != right[..read] {
            return Ok(false);
        }
    }
}

/// The file name of a stored path, whichever system wrote it.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Finds files under `folder` named like each missing path (ignoring case),
/// nearest first. Returns old path → found path.
pub fn search(folder: &Path, missing: &[String]) -> BTreeMap<String, String> {
    let mut wanted: HashMap<String, Vec<&String>> = HashMap::new();
    for path in missing {
        wanted.entry(file_name(path).to_lowercase()).or_default().push(path);
    }
    let mut found = BTreeMap::new();
    let mut level = vec![folder.to_path_buf()];
    for _ in 0..=SEARCH_DEPTH {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_dir() {
                    next.push(path);
                } else if let Some(olds) = entry.file_name().to_str().and_then(|n| wanted.remove(&n.to_lowercase())) {
                    for old in olds {
                        found.insert(old.clone(), path.to_string_lossy().into_owned());
                    }
                }
            }
        }
        if wanted.is_empty() || next.is_empty() {
            break;
        }
        level = next;
    }
    found
}

/// Replaces every string equal to an old path, including inside strings that
/// hold JSON documents (a service item's song). Returns whether anything changed.
pub fn replace_paths(value: &mut serde_json::Value, map: &BTreeMap<String, String>) -> bool {
    match value {
        serde_json::Value::String(text) => {
            if let Some(new) = map.get(text.as_str()) {
                *text = new.clone();
                return true;
            }
            if text.starts_with('{') {
                if let Ok(mut inner) = serde_json::from_str::<serde_json::Value>(text) {
                    if replace_paths(&mut inner, map) {
                        *text = inner.to_string();
                        return true;
                    }
                }
            }
            false
        }
        serde_json::Value::Array(items) => items.iter_mut().fold(false, |changed, item| replace_paths(item, map) || changed),
        serde_json::Value::Object(fields) => fields.values_mut().fold(false, |changed, field| replace_paths(field, map) || changed),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("magi-library-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn imports_copy_once_and_keep_different_files_apart() {
        let dir = temp_dir();
        let (outside, data) = (dir.join("outside"), dir.join("data"));
        std::fs::create_dir_all(outside.join("other")).unwrap();
        std::fs::write(outside.join("clip.mp4"), b"first").unwrap();
        std::fs::write(outside.join("other/clip.mp4"), b"second").unwrap();

        let copy = import_file(&data, &outside.join("clip.mp4")).unwrap();
        assert_eq!(copy.file_name().unwrap(), "clip.mp4");
        assert_eq!(std::fs::read(&copy).unwrap(), b"first");
        assert_eq!(import_file(&data, &outside.join("clip.mp4")).unwrap(), copy, "an identical file is reused");
        let other = import_file(&data, &outside.join("other/clip.mp4")).unwrap();
        assert_eq!(other.file_name().unwrap(), "clip (2).mp4");
        assert_eq!(import_file(&data, &copy).unwrap(), copy, "a library file stays where it is");
        assert_eq!(std::fs::read_dir(data.join(MEDIA_FOLDER)).unwrap().count(), 2, "no partial files remain");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn relinking_finds_files_by_name_in_subfolders() {
        let dir = temp_dir();
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/b/Welcome.MP4"), b"").unwrap();
        std::fs::write(dir.join("a/logo.png"), b"").unwrap();
        let missing = vec![r"C:\Users\church\Videos\welcome.mp4".to_string(), "/old/logo.png".to_string(), "/old/gone.png".to_string()];
        let found = search(&dir, &missing);
        assert_eq!(found.len(), 2);
        assert!(found[&missing[0]].ends_with("Welcome.MP4"));
        assert!(found[&missing[1]].ends_with("logo.png"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn paths_are_replaced_in_items_and_embedded_song_documents() {
        let map = BTreeMap::from([("/old/bg.png".to_string(), "/new/bg.png".to_string())]);
        let mut project = serde_json::json!({
            "session": { "items": [
                { "background": "/old/bg.png", "song": "{\"background\":\"/old/bg.png\",\"parts\":[]}", "kind": { "kind": "image", "data": { "path": "/old/other.png" } } }
            ] }
        });
        assert!(replace_paths(&mut project, &map));
        let item = &project["session"]["items"][0];
        assert_eq!(item["background"], "/new/bg.png");
        assert_eq!(serde_json::from_str::<serde_json::Value>(item["song"].as_str().unwrap()).unwrap()["background"], "/new/bg.png");
        assert_eq!(item["kind"]["data"]["path"], "/old/other.png");
        assert!(!replace_paths(&mut project, &map), "nothing left to replace");
    }
}
