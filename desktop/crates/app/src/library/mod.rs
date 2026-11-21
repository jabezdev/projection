pub mod storage;

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MEDIA_FOLDER: &str = "media";
const SEARCH_DEPTH: usize = 6;

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
        let partial = folder.join(format!("{name}.partial"));
        if let Err(e) = std::fs::copy(&source, &partial).and_then(|_| std::fs::rename(&partial, &target)) {
            let _ = std::fs::remove_file(&partial);
            return Err(e);
        }
        return Ok(target);
    }
    unreachable!("the loop returns")
}

pub fn import_bytes(data_dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let folder = data_dir.join(MEDIA_FOLDER);
    std::fs::create_dir_all(&folder)?;
    let name: String = name.chars().map(|c| if c.is_alphanumeric() || " ._-()".contains(c) { c } else { '_' }).collect();
    let name = name.trim_start_matches('.');
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem.to_string(), format!(".{extension}")),
        _ => (if name.is_empty() { "media".to_string() } else { name.to_string() }, String::new()),
    };
    for attempt in 1.. {
        let file = if attempt == 1 { format!("{stem}{extension}") } else { format!("{stem} ({attempt}){extension}") };
        let target = folder.join(&file);
        if target.exists() {
            if std::fs::read(&target)? == bytes {
                return Ok(target);
            }
            continue;
        }
        let partial = folder.join(format!("{file}.partial"));
        if let Err(e) = std::fs::write(&partial, bytes).and_then(|()| std::fs::rename(&partial, &target)) {
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

pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

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
    fn rendered_pages_are_written_once_under_a_safe_name() {
        let dir = std::env::temp_dir().join(format!("magi-import-bytes-{}", uuid::Uuid::new_v4()));
        let first = import_bytes(&dir, "../Sermon slides p1.png", b"page one").unwrap();
        assert_eq!(first.file_name().unwrap(), "_Sermon slides p1.png", "no path escapes the media folder");
        assert_eq!(first.parent().unwrap(), dir.join(MEDIA_FOLDER));
        assert_eq!(import_bytes(&dir, "../Sermon slides p1.png", b"page one").unwrap(), first, "identical bytes reuse the file");
        let other = import_bytes(&dir, "../Sermon slides p1.png", b"another page").unwrap();
        assert_eq!(other.file_name().unwrap(), "_Sermon slides p1 (2).png");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
