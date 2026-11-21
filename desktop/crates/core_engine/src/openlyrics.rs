use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use protocol::model::{new_id, Author, AuthorRole, Credits, Section, SectionKind, Song, SongVersion};

struct Verse {
    name: String,
    slides: Vec<String>,
}

pub fn parse(xml: &str) -> anyhow::Result<Song> {
    let mut reader = Reader::from_str(xml);
    let mut path: Vec<String> = Vec::new();
    let mut title: Option<String> = None;
    let mut verse_order = String::new();
    let mut verses: Vec<Verse> = Vec::new();
    let mut credits = Credits::default();
    let mut role: Option<AuthorRole> = None;
    let mut language: Option<String> = None;
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
                    "author" => {
                        role = match attribute(&element, "type").as_deref() {
                            Some("words") => Some(AuthorRole::Words),
                            Some("music") => Some(AuthorRole::Music),
                            Some("translation") => Some(AuthorRole::Translation),
                            _ => None,
                        };
                        text.clear();
                    }
                    "title" | "verseOrder" | "lines" | "copyright" | "ccliNo" | "publisher" | "released" => text.clear(),
                    _ => {}
                }
                path.push(name);
            }
            Event::Empty(element) => {
                if skip_depth == 0 && local(&element) == "br" && in_lines(&path) {
                    text.push('\n');
                }
            }
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
                    "author" if !text.trim().is_empty() => credits.authors.push(Author { name: text.trim().to_string(), role: role.take() }),
                    "copyright" => credits.copyright = non_empty(&text),
                    "ccliNo" => credits.ccli = non_empty(&text),
                    "publisher" => credits.publisher = non_empty(&text),
                    "released" => credits.year = text.trim().get(..4).and_then(|y| y.parse().ok()),
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
    let (sections, arrangement) = sections(&verses, &verse_order);
    let current = SongVersion { id: new_id(), number: 1, sections, arrangement, background: None };
    Ok(Song { id: new_id(), title, archived: false, credits, key: None, tags: Vec::new(), current })
}

fn non_empty(text: &str) -> Option<String> {
    Some(text.trim().to_string()).filter(|t| !t.is_empty())
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

fn push_text(text: &mut String, content: &str) {
    for (index, word) in content.split(char::is_whitespace).enumerate() {
        if index > 0 && !text.ends_with([' ', '\n']) {
            text.push(' ');
        }
        text.push_str(word);
    }
}

fn clean(text: &str) -> String {
    text.split('\n').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

fn part_kind(name: &str) -> (SectionKind, Option<u32>) {
    let mut chars = name.chars();
    let kind = match chars.next().map(|c| c.to_ascii_lowercase()) {
        Some('c') => SectionKind::Chorus,
        Some('p') => SectionKind::PreChorus,
        Some('b') => SectionKind::Bridge,
        Some('i') => SectionKind::Intro,
        Some('e') => SectionKind::Outro,
        Some('o') => SectionKind::Tag,
        _ => SectionKind::Verse,
    };
    let digits: String = chars.take_while(|c| c.is_ascii_digit()).collect();
    (kind, digits.parse().ok())
}

fn sections(verses: &[Verse], verse_order: &str) -> (Vec<Section>, Vec<String>) {
    let mut used: Vec<(SectionKind, u32)> = Vec::new();
    let sections: Vec<Section> = verses
        .iter()
        .enumerate()
        .map(|(index, verse)| {
            let (kind, number) = part_kind(&verse.name);
            let mut number = number.unwrap_or(1);
            while used.contains(&(kind, number)) {
                number += 1;
            }
            used.push((kind, number));
            Section { id: part_id(index, verse), kind, number: Some(number), name: None, color: None, lyrics: verse.slides.join("\n\n") }
        })
        .collect();
    let by_name = |name: &str| verses.iter().position(|v| v.name.eq_ignore_ascii_case(name));
    let mut arrangement: Vec<String> = verse_order.split_whitespace().filter_map(by_name).map(|i| part_id(i, &verses[i])).collect();
    if arrangement.is_empty() {
        arrangement = verses.iter().enumerate().map(|(i, v)| part_id(i, v)).collect();
    }
    (sections, arrangement)
}

fn part_id(index: usize, verse: &Verse) -> String {
    format!("ol-{index}-{}", verse.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::model::AuthorRole;

    const AMAZING_GRACE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<song xmlns="http://openlyrics.info/namespace/2009/song" version="0.9" createdIn="OpenLP 3.0">
  <properties>
    <titles><title>Amazing Grace</title><title lang="es">Sublime Gracia</title></titles>
    <authors><author type="words">John Newton</author><author>Edwin Excell</author></authors>
    <copyright>Public Domain</copyright>
    <ccliNo>22025</ccliNo>
    <released>1779</released>
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
        let version = &song.current;
        let codes: Vec<String> = version.sections.iter().map(|p| p.code()).collect();
        assert_eq!(codes, ["V1", "C1", "V2"]);
        assert_eq!(version.sections[0].slides(), ["Amazing grace how sweet the sound\nThat saved a wretch like me", "I once was lost & now am found\nWas blind but now I see"]);
        assert_eq!(version.sections[2].slides(), ["'Twas grace that taught\nMy heart to fear"]);
        let order: Vec<String> = version.arrangement.iter().map(|id| version.section(id).unwrap().code()).collect();
        assert_eq!(order, ["V1", "C1", "V2", "C1"]);
    }

    #[test]
    fn credits_come_from_the_properties() {
        let credits = parse(AMAZING_GRACE).unwrap().credits;
        assert_eq!(credits.authors.iter().map(|a| (a.name.as_str(), a.role)).collect::<Vec<_>>(), [("John Newton", Some(AuthorRole::Words)), ("Edwin Excell", None)]);
        assert_eq!((credits.copyright.as_deref(), credits.ccli.as_deref(), credits.year), (Some("Public Domain"), Some("22025"), Some(1779)));
    }

    #[test]
    fn without_a_verse_order_parts_play_in_document_order() {
        let xml = r#"<song><properties><titles><title>Short</title></titles></properties><lyrics>
            <verse name="v1"><lines>One</lines></verse><verse name="v1a"><lines>Two</lines></verse></lyrics></song>"#;
        let song = parse(xml).unwrap().current;
        assert_eq!(song.sections.iter().map(|p| p.code()).collect::<Vec<_>>(), ["V1", "V2"], "duplicate labels are renumbered");
        assert_eq!(song.arrangement.len(), 2);
    }

    #[test]
    fn other_files_are_rejected() {
        assert!(parse("<html><body/></html>").is_err());
        assert!(parse("<song><lyrics/></song>").is_err());
        assert!(parse("not xml <<<").is_err());
    }
}
