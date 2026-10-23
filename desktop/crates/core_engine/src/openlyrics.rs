//! OpenLyrics (https://docs.openlyrics.org) import: one XML song becomes a
//! `magi.song.v1` document. Each `<verse>` becomes a part and each of its
//! `<lines>` a slide; `<br/>` is a line break. Chords, comments and formatting
//! tags keep only their text. The `verseOrder` property becomes the
//! arrangement (document order when it is missing). Only the first language
//! present is imported.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::song::SONG_FORMAT;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedSong {
    pub title: String,
    /// A `magi.song.v1` document, serialized as stored in the library.
    pub document: String,
}

struct Verse {
    name: String,
    slides: Vec<String>,
}

/// Parses one OpenLyrics document.
pub fn parse(xml: &str) -> anyhow::Result<ImportedSong> {
    let mut reader = Reader::from_str(xml);
    let mut path: Vec<String> = Vec::new();
    let mut title: Option<String> = None;
    let mut verse_order = String::new();
    let mut verses: Vec<Verse> = Vec::new();
    let mut language: Option<String> = None;
    // Text collects inside <title>, <verseOrder> and <lines>; skipping counts
    // nested elements whose text is dropped (comments, other languages).
    let mut text = String::new();
    let mut skip_depth = 0usize;
    let mut saw_song = false;
    loop {
        match reader.read_event()? {
            Event::Start(element) => {
                if skip_depth > 0 {
                    skip_depth += 1;
                    continue;
                }
                let name = local(&element);
                match name.as_str() {
                    "song" => saw_song = true,
                    "comment" if in_lines(&path) => {
                        skip_depth = 1;
                        continue;
                    }
                    "verse" => {
                        let lang = attribute(&element, "lang");
                        if language.is_none() {
                            language = lang.clone();
                        }
                        if lang.is_some() && lang != language {
                            skip_depth = 1;
                            continue;
                        }
                        verses.push(Verse { name: attribute(&element, "name").unwrap_or_default(), slides: Vec::new() });
                    }
                    "title" | "verseOrder" | "lines" => text.clear(),
                    _ => {}
                }
                path.push(name);
            }
            Event::Empty(element) => {
                if skip_depth == 0 && local(&element) == "br" && in_lines(&path) {
                    text.push('\n');
                }
            }
            // Only <br/> breaks a line; the file's own line breaks and
            // indentation are insignificant whitespace.
            Event::Text(content) if skip_depth == 0 => push_text(&mut text, &content.xml10_content()),
            Event::CData(content) if skip_depth == 0 => push_text(&mut text, &content.xml10_content()),
            Event::GeneralRef(reference) if skip_depth == 0 => {
                if let Some(c) = reference.resolve_char_ref()? {
                    text.push(c);
                } else if let Some(value) = quick_xml::escape::resolve_predefined_entity(&reference) {
                    text.push_str(value);
                }
            }
            Event::End(element) => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                let name = element.local_name().as_ref().to_string();
                match name.as_str() {
                    "title" if path.iter().any(|n| n == "titles") && title.is_none() => title = Some(text.trim().to_string()),
                    "verseOrder" => verse_order = text.trim().to_string(),
                    "lines" => {
                        let slide = clean(&text);
                        if let (Some(verse), false) = (verses.last_mut(), slide.is_empty()) {
                            verse.slides.push(slide);
                        }
                    }
                    _ => {}
                }
                path.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    anyhow::ensure!(saw_song, "not an OpenLyrics song (no <song> element)");
    verses.retain(|v| !v.slides.is_empty());
    anyhow::ensure!(!verses.is_empty(), "the song has no lyrics");
    let title = title.filter(|t| !t.is_empty()).unwrap_or_else(|| "Untitled".into());
    Ok(ImportedSong { title, document: document(&verses, &verse_order).to_string() })
}

fn local(element: &BytesStart) -> String {
    element.local_name().as_ref().to_string()
}

fn attribute(element: &BytesStart, name: &str) -> Option<String> {
    let value = element.try_get_attribute(name).ok().flatten()?;
    value.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok().map(|v| v.into_owned())
}

fn in_lines(path: &[String]) -> bool {
    path.iter().any(|n| n == "lines")
}

/// Appends text with every whitespace run collapsed to one space.
fn push_text(text: &mut String, content: &str) {
    for (index, word) in content.split(char::is_whitespace).enumerate() {
        if index > 0 && !text.ends_with([' ', '\n']) {
            text.push(' ');
        }
        text.push_str(word);
    }
}

/// One line per `<br/>`, trimmed. Empty lines are dropped: a blank line
/// inside a slide would split it in two.
fn clean(text: &str) -> String {
    text.split('\n').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

/// Part type and number from an OpenLyrics verse name such as `v1`, `c`, `p2` or `b1a`.
fn part_kind(name: &str) -> (&'static str, Option<u32>) {
    let mut chars = name.chars();
    let kind = match chars.next().map(|c| c.to_ascii_lowercase()) {
        Some('c') => "Chorus",
        Some('p') => "Pre-chorus",
        Some('b') => "Bridge",
        Some('i') => "Intro",
        Some('e') => "Outro",
        Some('o') => "Tag",
        _ => "Verse",
    };
    let digits: String = chars.take_while(|c| c.is_ascii_digit()).collect();
    (kind, digits.parse().ok())
}

fn document(verses: &[Verse], verse_order: &str) -> serde_json::Value {
    let mut used: Vec<(&str, u32)> = Vec::new();
    let parts: Vec<serde_json::Value> = verses
        .iter()
        .enumerate()
        .map(|(index, verse)| {
            let (kind, number) = part_kind(&verse.name);
            // Numbers stay as written unless two parts would share a label.
            let mut number = number.unwrap_or(1);
            while used.contains(&(kind, number)) {
                number += 1;
            }
            used.push((kind, number));
            serde_json::json!({ "id": part_id(index, verse), "type": kind, "number": number, "lyrics": verse.slides.join("\n\n") })
        })
        .collect();
    let by_name = |name: &str| verses.iter().position(|v| v.name.eq_ignore_ascii_case(name));
    let mut arrangement: Vec<String> = verse_order.split_whitespace().filter_map(by_name).map(|i| part_id(i, &verses[i])).collect();
    if arrangement.is_empty() {
        arrangement = verses.iter().enumerate().map(|(i, v)| part_id(i, v)).collect();
    }
    serde_json::json!({ "format": SONG_FORMAT, "slideSeparator": "blank-line", "parts": parts, "arrangement": arrangement, "background": "" })
}

fn part_id(index: usize, verse: &Verse) -> String {
    format!("ol-{index}-{}", verse.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::Song;

    const AMAZING_GRACE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<song xmlns="http://openlyrics.info/namespace/2009/song" version="0.9" createdIn="OpenLP 3.0">
  <properties>
    <titles><title>Amazing Grace</title><title lang="es">Sublime Gracia</title></titles>
    <authors><author>John Newton</author></authors>
    <verseOrder>v1 c v2 c</verseOrder>
  </properties>
  <lyrics>
    <verse name="v1" lang="en">
      <lines>Amazing grace how <chord name="D"/>sweet the sound<br/>That saved a wretch like me</lines>
      <lines break="optional">I once was lost &amp; now am found<br/><comment>slower</comment>Was blind but now I see</lines>
    </verse>
    <verse name="v1" lang="es"><lines>Sublime gracia</lines></verse>
    <verse name="c" lang="en"><lines>My chains are gone</lines></verse>
    <verse name="v2" lang="en"><lines>
        'Twas grace that taught<br/>
        My heart to fear
    </lines></verse>
  </lyrics>
</song>"#;

    #[test]
    fn verses_become_parts_and_lines_become_slides() {
        let song = parse(AMAZING_GRACE).unwrap();
        assert_eq!(song.title, "Amazing Grace");
        let parsed = Song::decode(&song.document);
        let codes: Vec<String> = parsed.parts.iter().map(|p| p.code()).collect();
        assert_eq!(codes, ["V1", "C1", "V2"]);
        assert_eq!(parsed.parts[0].slides(), ["Amazing grace how sweet the sound\nThat saved a wretch like me", "I once was lost & now am found\nWas blind but now I see"]);
        assert_eq!(parsed.parts[2].slides(), ["'Twas grace that taught\nMy heart to fear"]);
        let order: Vec<&str> = parsed.arrangement.iter().map(|id| parsed.parts.iter().find(|p| &p.id == id).unwrap().part_type.as_str()).collect();
        assert_eq!(order, ["Verse", "Chorus", "Verse", "Chorus"]);
    }

    #[test]
    fn without_a_verse_order_parts_play_in_document_order() {
        let xml = r#"<song><properties><titles><title>Short</title></titles></properties><lyrics>
            <verse name="v1"><lines>One</lines></verse><verse name="v1a"><lines>Two</lines></verse></lyrics></song>"#;
        let song = Song::decode(&parse(xml).unwrap().document);
        assert_eq!(song.parts.iter().map(|p| p.code()).collect::<Vec<_>>(), ["V1", "V2"], "duplicate labels are renumbered");
        assert_eq!(song.arrangement.len(), 2);
    }

    #[test]
    fn other_files_are_rejected() {
        assert!(parse("<html><body/></html>").is_err());
        assert!(parse("<song><lyrics/></song>").is_err());
        assert!(parse("not xml <<<").is_err());
    }
}
