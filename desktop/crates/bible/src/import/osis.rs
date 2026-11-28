use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;

use super::xml::{attribute, local};
use super::{clean, Parsed};
use crate::bible::Verse;
use crate::books::by_osis;

const SKIPPED: &[&str] = &["note", "title", "rdg", "header", "reference", "milestone"];

fn locate(id: &str) -> Option<(u8, u16, u16)> {
    let first = id.split_whitespace().next()?;
    let mut parts = first.split('.');
    let book = by_osis(parts.next()?)?;
    let chapter = parts.next()?.parse().ok()?;
    let verse = parts.next()?.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()?;
    Some((book, chapter, verse))
}

pub(super) fn parse(text: &str) -> Result<Parsed> {
    let mut reader = Reader::from_str(text);
    let mut parsed = Parsed { name: None, abbreviation: None, language: None, verses: Vec::new() };
    let mut current: Option<(u8, u16, u16)> = None;
    let mut buffer = String::new();
    let mut skip = 0usize;
    let mut in_work_title = false;
    let mut in_header = false;
    let flush = |current: &mut Option<(u8, u16, u16)>, buffer: &mut String, verses: &mut Vec<Verse>| {
        if let Some((book, chapter, verse)) = current.take() {
            let text = clean(buffer);
            if !text.is_empty() {
                verses.push(Verse { book, chapter, verse, text });
            }
        }
        buffer.clear();
    };
    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let name = local(&e);
                match name.as_str() {
                    "osisText" => {
                        parsed.language = attribute(&e, "xml:lang");
                        if parsed.abbreviation.is_none() {
                            parsed.abbreviation = attribute(&e, "osisIDWork");
                        }
                    }
                    "header" => in_header = true,
                    "title" if in_header && parsed.name.is_none() => in_work_title = true,
                    "verse" => {
                        flush(&mut current, &mut buffer, &mut parsed.verses);
                        current = attribute(&e, "osisID").and_then(|id| locate(&id));
                    }
                    n if SKIPPED.contains(&n) && !in_header => skip += 1,
                    _ => {}
                }
            }
            Event::Empty(e) => {
                let name = local(&e);
                if name == "verse" {
                    if attribute(&e, "eID").is_some() {
                        flush(&mut current, &mut buffer, &mut parsed.verses);
                    } else if let Some(id) = attribute(&e, "osisID") {
                        flush(&mut current, &mut buffer, &mut parsed.verses);
                        current = locate(&id);
                    }
                } else if name == "lb" || name == "l" {
                    buffer.push(' ');
                }
            }
            Event::Text(t) => {
                let content = t.xml10_content();
                if in_work_title {
                    parsed.name = Some(content.trim().to_string());
                } else if skip == 0 && current.is_some() {
                    buffer.push_str(&content);
                }
            }
            Event::GeneralRef(r) if skip == 0 && current.is_some() => {
                if let Some(c) = r.resolve_char_ref()? {
                    buffer.push(c);
                } else if let Some(value) = quick_xml::escape::resolve_predefined_entity(&r) {
                    buffer.push_str(value);
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_string();
                match name.as_str() {
                    "header" => in_header = false,
                    "title" if in_work_title => in_work_title = false,
                    "verse" => flush(&mut current, &mut buffer, &mut parsed.verses),
                    "chapter" | "div" | "osisText" => flush(&mut current, &mut buffer, &mut parsed.verses),
                    n if SKIPPED.contains(&n) && !in_header && skip > 0 => skip -= 1,
                    "p" | "l" | "lg" | "q" => buffer.push(' '),
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    flush(&mut current, &mut buffer, &mut parsed.verses);
    Ok(parsed)
}
