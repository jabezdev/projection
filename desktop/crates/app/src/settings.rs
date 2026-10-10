//! Settings files in the app-data directory: atomic replacement on save, and
//! malformed files set aside rather than silently overwritten.

use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Serialize};

/// Writes through a sibling temporary file and renames it into place, so a
/// crash mid-write leaves the previous file intact.
pub fn write_atomic(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temporary, bytes)?;
    if let Err(e) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(e.into());
    }
    Ok(())
}

pub struct Loaded<T> {
    pub value: T,
    /// Where an unreadable file was moved, so the operator can recover it.
    pub set_aside: Option<PathBuf>,
}

/// Reads `path`, using `T::default()` when it does not exist. A file that
/// fails to parse is renamed to `<name>.invalid-<ms>.json` and reported.
pub fn load_or_set_aside<T: DeserializeOwned + Default>(path: &Path, now_ms: i64) -> Loaded<T> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Loaded { value: T::default(), set_aside: None },
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => Loaded { value, set_aside: None },
        Err(e) => {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("settings");
            let aside = path.with_file_name(format!("{stem}.invalid-{now_ms}.json"));
            match std::fs::rename(path, &aside) {
                Ok(()) => log::error!("{} is unreadable ({e}); kept as {} and using defaults", path.display(), aside.display()),
                Err(rename) => log::error!("{} is unreadable ({e}) and could not be set aside ({rename})", path.display()),
            }
            Loaded { value: T::default(), set_aside: Some(aside) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::OutputConfig;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("magi-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn existing_settings_load_and_save_atomically() {
        let dir = temp_dir();
        let path = dir.join("output-settings.json");
        std::fs::write(&path, r#"{"font":"Inter","size":4.5,"lineHeight":0.9}"#).unwrap();
        let loaded: Loaded<OutputConfig> = load_or_set_aside(&path, 1);
        assert_eq!((loaded.value.font.as_str(), loaded.value.size, loaded.value.line_height), ("Inter", 4.5, 0.9));
        assert!(loaded.set_aside.is_none());

        write_atomic(&path, &OutputConfig { size: 7.0, ..loaded.value }).unwrap();
        let reloaded: Loaded<OutputConfig> = load_or_set_aside(&path, 2);
        assert_eq!((reloaded.value.font.as_str(), reloaded.value.size), ("Inter", 7.0));
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name()).collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("output-settings.json")], "no temporary files remain");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn malformed_settings_are_kept_aside_not_overwritten() {
        let dir = temp_dir();
        let path = dir.join("output-settings.json");
        std::fs::write(&path, "{ this is not json").unwrap();
        let loaded: Loaded<OutputConfig> = load_or_set_aside(&path, 1234);
        assert_eq!(loaded.value, OutputConfig::default());
        let aside = loaded.set_aside.unwrap();
        assert_eq!(aside.file_name().unwrap(), "output-settings.invalid-1234.json");
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "{ this is not json");
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_file_uses_defaults() {
        let dir = temp_dir();
        let loaded: Loaded<OutputConfig> = load_or_set_aside(&dir.join("absent.json"), 1);
        assert_eq!(loaded.value, OutputConfig::default());
        assert!(loaded.set_aside.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
