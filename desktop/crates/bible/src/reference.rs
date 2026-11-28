use anyhow::{anyhow, bail, Result};

use crate::books::{book, books};

pub const END: u16 = u16::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub book: u8,
    pub from: (u16, u16),
    pub to: (u16, u16),
}

const FILLER: &[&str] = &["mga", "taga", "kay", "ang", "ng", "of", "the", "book", "sulat", "ni", "san", "st", "saint", "sa", "aklat"];

fn ordinal(word: &str) -> Option<&'static str> {
    match word {
        "1" | "i" | "1st" | "first" | "una" | "unang" => Some("1"),
        "2" | "ii" | "2nd" | "second" | "ikalawa" | "ikalawang" => Some("2"),
        "3" | "iii" | "3rd" | "third" | "ikatlo" | "ikatlong" => Some("3"),
        _ => None,
    }
}

pub(crate) fn normalize_book(text: &str) -> String {
    let mut spaced = String::new();
    let mut previous: Option<char> = None;
    for c in text.to_lowercase().chars() {
        let c = if matches!(c, '.' | '-' | '_' | '–') { ' ' } else { c };
        if let Some(p) = previous {
            if p.is_ascii_digit() && c.is_alphabetic() {
                spaced.push(' ');
            }
        }
        spaced.push(c);
        previous = Some(c);
    }
    let words: Vec<&str> = spaced.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    for (i, word) in words.iter().enumerate() {
        if i == 0 && words.len() > 1 {
            if let Some(n) = ordinal(word) {
                out.push(n.to_string());
                continue;
            }
        }
        if FILLER.contains(word) {
            continue;
        }
        out.push(crate::bible::fold(word));
    }
    out.join(" ")
}

fn keys() -> &'static [(String, u8)] {
    static KEYS: std::sync::OnceLock<Vec<(String, u8)>> = std::sync::OnceLock::new();
    KEYS.get_or_init(|| {
        let mut keys = Vec::new();
        for (i, b) in books().iter().enumerate() {
            for name in [&b.en, &b.tl, &b.code, &b.osis].into_iter().chain(b.aliases.iter()) {
                let key = normalize_book(name);
                if !key.is_empty() && !keys.iter().any(|(k, n)| *k == key && *n == i as u8) {
                    keys.push((key, i as u8));
                }
            }
        }
        keys
    })
}

pub(crate) fn find_book(text: &str) -> Result<u8> {
    let query = normalize_book(text);
    if query.is_empty() {
        bail!("Type a book name, like John 3:16");
    }
    if let Some((_, book)) = keys().iter().find(|(k, _)| *k == query) {
        return Ok(*book);
    }
    let letters = query.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 {
        bail!("“{}” is too short for a book name", text.trim());
    }
    let mut found: Vec<u8> = keys().iter().filter(|(k, _)| k.starts_with(&query)).map(|(_, b)| *b).collect();
    found.sort_unstable();
    found.dedup();
    match found.as_slice() {
        [one] => Ok(*one),
        [] => bail!("No book called “{}”", text.trim()),
        many => {
            let names: Vec<&str> = many.iter().take(4).filter_map(|b| book(*b)).map(|b| b.en.as_str()).collect();
            bail!("“{}” could be {}", text.trim(), names.join(", "))
        }
    }
}

fn split_book(segment: &str) -> (&str, &str) {
    let mut seen_letter = false;
    for (i, c) in segment.char_indices() {
        if c.is_alphabetic() {
            seen_letter = true;
        } else if c.is_ascii_digit() && seen_letter {
            return (&segment[..i], &segment[i..]);
        }
    }
    if seen_letter { (segment, "") } else { ("", segment) }
}

fn number(text: &str) -> Result<u16> {
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    let rest = &text[digits.len()..];
    if digits.is_empty() || !rest.chars().all(|c| matches!(c, 'a' | 'b' | 'c')) {
        bail!("“{text}” is not a chapter or verse number");
    }
    let n: u16 = digits.parse().map_err(|_| anyhow!("“{text}” is too large"))?;
    if n == 0 {
        bail!("Chapters and verses start at 1");
    }
    Ok(n)
}

fn point(text: &str) -> Result<(Option<u16>, u16)> {
    match text.split_once(':') {
        Some((c, v)) => Ok((Some(number(c)?), number(v)?)),
        None => Ok((None, number(text)?)),
    }
}

pub fn parse_reference(input: &str) -> Result<Vec<Span>> {
    let mut spans = Vec::new();
    let mut current: Option<u8> = None;
    for segment in input.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (name, numbers) = split_book(segment);
        let index = if name.trim().is_empty() { current.ok_or_else(|| anyhow!("Start with a book name, like John 3:16"))? } else { find_book(name)? };
        current = Some(index);
        let info = book(index).expect("book index");
        let numbers: String = numbers
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| match c {
                '.' | '：' => ':',
                '–' | '—' | '‑' | '‒' => '-',
                c => c.to_ascii_lowercase(),
            })
            .collect();
        if numbers.is_empty() {
            spans.push(Span { book: index, from: (1, 1), to: (info.chapters, END) });
            continue;
        }
        let mut chapter: Option<u16> = None;
        for part in numbers.split(',').filter(|p| !p.is_empty()) {
            let (start, end) = match part.split_once('-') {
                Some((a, b)) => (a, Some(b)),
                None => (part, None),
            };
            let (start_chapter, start_value) = point(start)?;
            let span = match (start_chapter, chapter, info.single_chapter()) {
                (Some(c), _, _) => {
                    let to = match end.map(point).transpose()? {
                        None => (c, start_value),
                        Some((Some(c2), v2)) => (c2, v2),
                        Some((None, v2)) => (c, v2),
                    };
                    chapter = Some(to.0);
                    Span { book: index, from: (c, start_value), to }
                }
                (None, context, single) if context.is_some() || single => {
                    let c = if single { 1 } else { context.unwrap_or(1) };
                    let to = match end.map(point).transpose()? {
                        None => (c, start_value),
                        Some((Some(c2), v2)) => (c2, v2),
                        Some((None, v2)) => (c, v2),
                    };
                    chapter = Some(to.0);
                    Span { book: index, from: (c, start_value), to }
                }
                (None, _, _) => {
                    let last = match end.map(point).transpose()? {
                        None => start_value,
                        Some((None, c2)) => c2,
                        Some((Some(c2), v2)) => {
                            chapter = Some(c2);
                            spans.push(Span { book: index, from: (start_value, 1), to: (c2, v2) });
                            continue;
                        }
                    };
                    Span { book: index, from: (start_value, 1), to: (last, END) }
                }
            };
            if span.from.0 > info.chapters || span.to.0 > info.chapters {
                bail!("{} has {} chapters", info.en, info.chapters);
            }
            if (span.to.0, span.to.1) < (span.from.0, span.from.1) {
                bail!("“{part}” ends before it starts");
            }
            spans.push(span);
        }
    }
    if spans.is_empty() {
        bail!("Type a reference, like John 3:16");
    }
    Ok(spans)
}

pub fn format_reference(spans: &[Span], language: &str) -> String {
    let mut out = String::new();
    let mut previous: Option<(u8, u16)> = None;
    for span in spans {
        let info = book(span.book).expect("book index");
        let whole_chapters = span.from.1 == 1 && span.to.1 == END;
        let same_book = previous.is_some_and(|(b, _)| b == span.book);
        let same_chapter = previous.is_some_and(|(b, c)| b == span.book && c == span.from.0) && !whole_chapters;
        if !out.is_empty() {
            out.push_str(if same_chapter { "," } else { ";" });
            out.push(' ');
        }
        if !same_book {
            out.push_str(info.name(language));
            if !(whole_chapters && span.from.0 == 1 && span.to.0 == info.chapters && info.chapters > 1) {
                out.push(' ');
            }
        }
        let verse = |v: u16| if v == END { String::new() } else { v.to_string() };
        if whole_chapters {
            if !(span.from.0 == 1 && span.to.0 == info.chapters && info.chapters > 1) {
                out.push_str(&span.from.0.to_string());
                if span.to.0 != span.from.0 {
                    out.push_str(&format!("–{}", span.to.0));
                }
            }
        } else if info.single_chapter() {
            out.push_str(&verse(span.from.1));
            if span.to != span.from {
                out.push_str(&format!("–{}", verse(span.to.1)));
            }
        } else {
            if !same_chapter {
                out.push_str(&format!("{}:", span.from.0));
            }
            out.push_str(&verse(span.from.1));
            if span.to.0 != span.from.0 {
                out.push_str(&format!("–{}:{}", span.to.0, verse(span.to.1)));
            } else if span.to.1 != span.from.1 {
                out.push_str(&format!("–{}", verse(span.to.1)));
            }
        }
        previous = Some((span.book, span.to.0));
    }
    out.trim_end().to_string()
}
