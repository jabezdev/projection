use std::collections::HashMap;
use std::io::{Read, Write};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BibleInfo {
    pub id: String,
    pub name: String,
    pub abbreviation: String,
    pub language: String,
    pub bundled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verse {
    pub book: u8,
    pub chapter: u16,
    pub verse: u16,
    pub text: String,
}

pub struct Bible {
    pub info: BibleInfo,
    verses: Vec<Verse>,
    chapters: HashMap<(u8, u16), (usize, usize)>,
}

impl Bible {
    pub fn new(info: BibleInfo, mut verses: Vec<Verse>) -> Self {
        verses.sort_by_key(|v| (v.book, v.chapter, v.verse));
        verses.dedup_by_key(|v| (v.book, v.chapter, v.verse));
        let mut chapters: HashMap<(u8, u16), (usize, usize)> = HashMap::new();
        for (i, v) in verses.iter().enumerate() {
            chapters.entry((v.book, v.chapter)).and_modify(|r| r.1 = i + 1).or_insert((i, i + 1));
        }
        Self { info, verses, chapters }
    }

    pub fn len(&self) -> usize {
        self.verses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.verses.is_empty()
    }

    pub fn chapter(&self, book: u8, chapter: u16) -> &[Verse] {
        self.chapters.get(&(book, chapter)).map_or(&[], |&(a, b)| &self.verses[a..b])
    }

    pub fn last_chapter(&self, book: u8) -> u16 {
        self.chapters.keys().filter(|(b, _)| *b == book).map(|(_, c)| *c).max().unwrap_or(0)
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<&Verse> {
        let words: Vec<String> = query.split_whitespace().map(fold).filter(|w| !w.is_empty()).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let phrase = words.join(" ");
        let (mut exact, mut loose) = (Vec::new(), Vec::new());
        for v in &self.verses {
            let text = fold(&v.text);
            if text.contains(&phrase) {
                exact.push(v);
                if exact.len() >= limit {
                    break;
                }
            } else if loose.len() < limit && words.iter().all(|w| text.contains(w.as_str())) {
                loose.push(v);
            }
        }
        exact.extend(loose);
        exact.truncate(limit);
        exact
    }

    pub fn to_tsv_gz(&self) -> Result<Vec<u8>> {
        let mut out = GzEncoder::new(Vec::new(), Compression::best());
        let info = &self.info;
        writeln!(out, "#id\t{}\n#name\t{}\n#abbreviation\t{}\n#language\t{}", info.id, info.name, info.abbreviation, info.language)?;
        for v in &self.verses {
            writeln!(out, "{}\t{}\t{}\t{}", v.book, v.chapter, v.verse, v.text)?;
        }
        Ok(out.finish()?)
    }

    pub fn from_tsv_gz(bytes: &[u8], bundled: bool) -> Result<Self> {
        let mut text = String::new();
        GzDecoder::new(bytes).read_to_string(&mut text).context("read bible data")?;
        let mut info = BibleInfo { id: String::new(), name: String::new(), abbreviation: String::new(), language: "en".into(), bundled };
        let mut verses = Vec::new();
        for line in text.lines() {
            if let Some(meta) = line.strip_prefix('#') {
                let (key, value) = meta.split_once('\t').unwrap_or((meta, ""));
                let value = value.to_string();
                match key {
                    "id" => info.id = value,
                    "name" => info.name = value,
                    "abbreviation" => info.abbreviation = value,
                    "language" => info.language = value,
                    _ => {}
                }
                continue;
            }
            let mut cols = line.splitn(4, '\t');
            let (Some(b), Some(c), Some(v), Some(t)) = (cols.next(), cols.next(), cols.next(), cols.next()) else { continue };
            verses.push(Verse { book: b.parse()?, chapter: c.parse()?, verse: v.parse()?, text: t.to_string() });
        }
        anyhow::ensure!(!info.id.is_empty(), "bible data has no id");
        Ok(Self::new(info, verses))
    }
}

pub(crate) fn fold(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'Á' | 'À' | 'Â' | 'Ä' => Some('a'),
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => Some('e'),
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => Some('i'),
            'ó' | 'ò' | 'ô' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Ö' => Some('o'),
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => Some('u'),
            'ñ' | 'Ñ' => Some('n'),
            '\'' | '’' | '‘' | ',' | '.' | ';' | ':' | '!' | '?' | '"' | '“' | '”' => None,
            c => Some(c.to_ascii_lowercase()),
        })
        .collect::<String>()
        .to_lowercase()
}
