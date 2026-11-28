use super::{clean, Parsed};
use crate::bible::Verse;
use crate::books::by_code;

const DROPPED_LINES: &[&str] = &["id", "ide", "h", "toc1", "toc2", "toc3", "mt", "mt1", "mt2", "mt3", "ms", "ms1", "ms2", "mr", "s", "s1", "s2", "s3", "sr", "r", "d", "cl", "cp", "rem", "sts", "usfm", "ip", "is", "imt", "io1", "io2", "iot"];
const DROPPED_SPANS: &[&str] = &["f", "fe", "x", "fig", "rq"];

pub(super) fn parse(files: &[String]) -> Parsed {
    let mut parsed = Parsed { name: None, abbreviation: None, language: None, verses: Vec::new() };
    for file in files {
        read_book(file, &mut parsed);
    }
    parsed
}

fn read_book(text: &str, parsed: &mut Parsed) {
    let mut book = None;
    let mut chapter = 0u16;
    let mut verse: Option<u16> = None;
    let mut buffer = String::new();
    let flush = |book: Option<u8>, chapter: u16, verse: &mut Option<u16>, buffer: &mut String, verses: &mut Vec<Verse>| {
        if let (Some(b), Some(v)) = (book, verse.take()) {
            let text = clean(&strip(buffer));
            if !text.is_empty() && chapter > 0 {
                verses.push(Verse { book: b, chapter, verse: v, text });
            }
        }
        buffer.clear();
    };
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix('\\') else {
            if verse.is_some() {
                buffer.push(' ');
                buffer.push_str(line);
            }
            continue;
        };
        let (marker, body) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let marker = marker.trim_end_matches('*');
        match marker {
            "id" => {
                flush(book, chapter, &mut verse, &mut buffer, &mut parsed.verses);
                book = body.split_whitespace().next().and_then(by_code);
                chapter = 0;
            }
            "c" => {
                flush(book, chapter, &mut verse, &mut buffer, &mut parsed.verses);
                chapter = body.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
            }
            "v" => {
                flush(book, chapter, &mut verse, &mut buffer, &mut parsed.verses);
                let (number, text) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
                verse = number.split('-').next().and_then(|n| n.parse().ok());
                buffer.push_str(text);
            }
            m if DROPPED_LINES.contains(&m) => {}
            _ => {
                if verse.is_some() {
                    buffer.push(' ');
                    buffer.push_str(line);
                }
            }
        }
    }
    flush(book, chapter, &mut verse, &mut buffer, &mut parsed.verses);
}

fn strip(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('\\') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let marker: String = after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '+').collect();
        let closing = after[marker.len()..].starts_with('*');
        let name = marker.trim_start_matches('+');
        let mut next = &after[marker.len() + usize::from(closing)..];
        if !closing && DROPPED_SPANS.contains(&name) {
            let end = format!("\\{marker}*");
            next = next.find(&end).map_or("", |i| &next[i + end.len()..]);
        } else if !closing && name == "v" {
            next = next.trim_start().split_once(char::is_whitespace).map_or("", |(_, t)| t);
        } else if !closing {
            next = next.strip_prefix(' ').unwrap_or(next);
        }
        rest = next;
    }
    out.push_str(rest);
    out.split('|').enumerate().map(|(i, part)| if i == 0 { part.to_string() } else { part.split_once(' ').map_or(String::new(), |(_, t)| format!(" {t}")) }).collect()
}
