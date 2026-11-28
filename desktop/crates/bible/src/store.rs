use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};

use crate::bible::{Bible, BibleInfo};

const BUNDLED: &[(&str, &[u8])] = &[("kjv", include_bytes!("../data/kjv.tsv.gz")), ("adb1905", include_bytes!("../data/adb1905.tsv.gz"))];
const KEEP_LOADED: usize = 3;

pub struct Bibles {
    dir: Option<PathBuf>,
    loaded: Mutex<Vec<Arc<Bible>>>,
    infos: Mutex<Option<Vec<BibleInfo>>>,
}

impl Bibles {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir, loaded: Mutex::new(Vec::new()), infos: Mutex::new(None) }
    }

    pub fn list(&self) -> Vec<BibleInfo> {
        let mut infos = self.infos.lock().expect("bibles poisoned");
        if let Some(list) = infos.as_ref() {
            return list.clone();
        }
        let mut list: Vec<BibleInfo> = BUNDLED.iter().filter_map(|(_, bytes)| header(bytes, true).ok()).collect();
        if let Some(dir) = &self.dir {
            if let Ok(entries) = std::fs::read_dir(dir) {
                let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with(".tsv.gz")).collect();
                files.sort();
                for path in files {
                    if let Ok(info) = std::fs::read(&path).map_err(anyhow::Error::from).and_then(|b| header(&b, false)) {
                        if !list.iter().any(|i| i.id == info.id) {
                            list.push(info);
                        }
                    }
                }
            }
        }
        *infos = Some(list.clone());
        list
    }

    pub fn get(&self, id: &str) -> Option<Arc<Bible>> {
        let mut loaded = self.loaded.lock().expect("bibles poisoned");
        if let Some(i) = loaded.iter().position(|b| b.info.id == id) {
            let bible = loaded.remove(i);
            loaded.insert(0, bible.clone());
            return Some(bible);
        }
        let bible = Arc::new(self.read(id).ok()?);
        loaded.insert(0, bible.clone());
        loaded.truncate(KEEP_LOADED);
        Some(bible)
    }

    fn read(&self, id: &str) -> Result<Bible> {
        if let Some((_, bytes)) = BUNDLED.iter().find(|(b, _)| *b == id) {
            return Bible::from_tsv_gz(bytes, true);
        }
        let path = self.path(id).context("no folder for imported Bibles")?;
        Bible::from_tsv_gz(&std::fs::read(&path).with_context(|| format!("read {}", path.display()))?, false)
    }

    fn path(&self, id: &str) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join(format!("{id}.tsv.gz")))
    }

    pub fn import(&self, file: &Path) -> Result<BibleInfo> {
        let mut bible = crate::import::import_file(file)?;
        let dir = self.dir.as_ref().context("no folder for imported Bibles")?;
        std::fs::create_dir_all(dir)?;
        let mut id = slug(&bible.info.abbreviation);
        if BUNDLED.iter().any(|(b, _)| *b == id) {
            id = format!("{id}-own");
        }
        bible.info.id = id.clone();
        let path = dir.join(format!("{id}.tsv.gz"));
        let tmp = dir.join(format!("{id}.tsv.gz.tmp"));
        std::fs::write(&tmp, bible.to_tsv_gz()?)?;
        std::fs::rename(&tmp, &path)?;
        self.forget(&id);
        Ok(bible.info.clone())
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        if BUNDLED.iter().any(|(b, _)| *b == id) {
            bail!("Bundled Bibles cannot be removed");
        }
        if let Some(path) = self.path(id) {
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        self.forget(id);
        Ok(())
    }

    fn forget(&self, id: &str) {
        self.loaded.lock().expect("bibles poisoned").retain(|b| b.info.id != id);
        *self.infos.lock().expect("bibles poisoned") = None;
    }

    pub fn fallback(&self, language: Option<&str>) -> Option<String> {
        let list = self.list();
        language
            .and_then(|l| list.iter().find(|b| b.bundled && b.language == l))
            .or_else(|| list.first())
            .map(|b| b.id.clone())
    }
}

fn header(bytes: &[u8], bundled: bool) -> Result<BibleInfo> {
    use std::io::{BufRead, BufReader};
    let mut info = BibleInfo { id: String::new(), name: String::new(), abbreviation: String::new(), language: "en".into(), bundled };
    let reader = BufReader::new(flate2::read::GzDecoder::new(bytes));
    for line in reader.lines() {
        let line = line?;
        let Some(meta) = line.strip_prefix('#') else { break };
        let (key, value) = meta.split_once('\t').unwrap_or((meta, ""));
        match key {
            "id" => info.id = value.into(),
            "name" => info.name = value.into(),
            "abbreviation" => info.abbreviation = value.into(),
            "language" => info.language = value.into(),
            _ => {}
        }
    }
    anyhow::ensure!(!info.id.is_empty(), "bible data has no id");
    Ok(info)
}

fn slug(text: &str) -> String {
    let s: String = text.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "bible".into() } else { s }
}

impl Default for Bibles {
    fn default() -> Self {
        Self::new(None)
    }
}

