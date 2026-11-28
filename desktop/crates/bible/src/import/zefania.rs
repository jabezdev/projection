use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;

use super::xml::{attribute, local};
use super::{clean, Parsed};
use crate::bible::Verse;

const SKIPPED: &[&str] = &["NOTE", "CAPTION", "REMARK", "XREF", "DIV", "PROLOG"];

pub(super) fn parse(text: &str) -> Result<Parsed> {
    let mut reader = Reader::from_str(text);
    let mut parsed = Parsed { name: None, abbreviation: None, language: None, verses: Vec::new() };
    let (mut book, mut chapter) = (None::<u8>, 0u16);
    let mut verse: Option<u16> = None;
    let mut buffer = String::new();
    let mut skip = 0usize;
    let mut field: Option<String> = None;
    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let name = local(&e);
                match name.to_uppercase().as_str() {
                    "XMLBIBLE" => parsed.name = attribute(&e, "biblename"),
                    "BIBLEBOOK" => book = attribute(&e, "bnumber").and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=66).contains(n)).map(|n| n - 1),
                    "CHAPTER" => chapter = attribute(&e, "cnumber").and_then(|n| n.parse().ok()).unwrap_or(0),
                    "VERS" => {
                        verse = attribute(&e, "vnumber").and_then(|n| n.parse().ok());
                        buffer.clear();
                    }
                    "TITLE" | "IDENTIFIER" | "LANGUAGE" if verse.is_none() => field = Some(name.to_uppercase()),
                    n if SKIPPED.contains(&n) => skip += 1,
                    _ => {}
                }
            }
            Event::Text(t) => {
                let content = t.xml10_content();
                match field.as_deref() {
                    Some("TITLE") if parsed.name.as_deref().is_none_or(str::is_empty) => parsed.name = Some(content.trim().to_string()),
                    Some("IDENTIFIER") => parsed.abbreviation = Some(content.trim().to_string()),
                    Some("LANGUAGE") => parsed.language = Some(language(content.trim())),
                    _ if skip == 0 && verse.is_some() => buffer.push_str(&content),
                    _ => {}
                }
            }
            Event::GeneralRef(r) if skip == 0 && verse.is_some() => {
                if let Some(c) = r.resolve_char_ref()? {
                    buffer.push(c);
                } else if let Some(value) = quick_xml::escape::resolve_predefined_entity(&r) {
                    buffer.push_str(value);
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_uppercase();
                match name.as_str() {
                    "VERS" => {
                        if let (Some(b), Some(v)) = (book, verse.take()) {
                            let text = clean(&buffer);
                            if !text.is_empty() && chapter > 0 {
                                parsed.verses.push(Verse { book: b, chapter, verse: v, text });
                            }
                        }
                        buffer.clear();
                    }
                    "TITLE" | "IDENTIFIER" | "LANGUAGE" => field = None,
                    n if SKIPPED.contains(&n) && skip > 0 => skip -= 1,
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(parsed)
}

fn language(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "eng" | "english" => "en".into(),
        "tgl" | "tagalog" | "fil" | "filipino" => "tl".into(),
        other => other.chars().take(3).collect(),
    }
}
