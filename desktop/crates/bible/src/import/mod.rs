use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::bible::{Bible, BibleInfo, Verse};

mod osis;
mod usfm;
mod zefania;
mod xml;

pub(crate) struct Parsed {
    pub name: Option<String>,
    pub abbreviation: Option<String>,
    pub language: Option<String>,
    pub verses: Vec<Verse>,
}

pub fn import_file(path: &Path) -> Result<Bible> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Bible").to_string();
    let parsed = if bytes.starts_with(b"PK") { from_zip(&bytes)? } else { from_text(&String::from_utf8_lossy(&bytes))? };
    finish(parsed, &stem)
}

pub(crate) fn from_text(text: &str) -> Result<Parsed> {
    let head: String = text.chars().take(4000).collect();
    if head.contains("<osis") {
        osis::parse(text)
    } else if head.contains("<XMLBIBLE") || head.contains("<xmlbible") {
        zefania::parse(text)
    } else if head.contains("\\id ") || head.contains("\\c ") {
        Ok(usfm::parse(&[text.to_string()]))
    } else {
        bail!("This is not an OSIS, Zefania or USFM Bible")
    }
}

fn from_zip(bytes: &[u8]) -> Result<Parsed> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("open zip")?;
    let mut usfm = Vec::new();
    let mut xml = None;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name()?.to_lowercase();
        if file.is_dir() {
            continue;
        }
        let mut raw = Vec::new();
        file.read_to_end(&mut raw)?;
        let text = String::from_utf8_lossy(&raw).into_owned();
        if name.ends_with(".usfm") || name.ends_with(".sfm") || (name.ends_with(".txt") && text.contains("\\id ")) {
            usfm.push(text);
        } else if name.ends_with(".xml") && xml.is_none() {
            xml = Some(text);
        }
    }
    if !usfm.is_empty() {
        return Ok(usfm::parse(&usfm));
    }
    match xml {
        Some(text) => from_text(&text),
        None => bail!("The zip has no USFM, OSIS or Zefania files"),
    }
}

fn finish(parsed: Parsed, stem: &str) -> Result<Bible> {
    if parsed.verses.len() < 100 {
        bail!("Only {} verses were found; is this a whole Bible?", parsed.verses.len());
    }
    let name = parsed.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| stem.to_string());
    let abbreviation = parsed
        .abbreviation
        .filter(|a| !a.trim().is_empty() && a.len() <= 12)
        .unwrap_or_else(|| abbreviate(&name));
    let language = parsed.language.map(|l| l.to_lowercase()).filter(|l| !l.is_empty()).unwrap_or_else(|| "en".into());
    let info = BibleInfo { id: String::new(), name: name.trim().to_string(), abbreviation: abbreviation.trim().to_string(), language, bundled: false };
    Ok(Bible::new(info, parsed.verses))
}

fn abbreviate(name: &str) -> String {
    let letters: String = name.split_whitespace().filter_map(|w| w.chars().next()).filter(|c| c.is_alphanumeric()).collect();
    if letters.len() >= 2 { letters.to_uppercase() } else { name.chars().take(6).collect() }
}

pub(crate) fn clean(text: &str) -> String {
    text.replace('¶', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}
