use protocol::{FeedDisplay, FeedItemKind, FeedSong, FeedState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Lyrics,
    SongInfo,
    Section,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub blank_when_hidden: bool,
    pub uppercase: bool,
    pub max_lines: usize,
    pub join_lines: bool,
    pub songs_only: bool,
    pub template: String,
    pub license: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            blank_when_hidden: true,
            uppercase: false,
            max_lines: 0,
            join_lines: false,
            songs_only: false,
            template: DEFAULT_TEMPLATE.to_string(),
            license: String::new(),
        }
    }
}

pub const DEFAULT_TEMPLATE: &str = "{title}\n{authors}\n{copyright} · CCLI Song #{ccli} · CCLI License #{license}";

pub fn text(kind: Kind, feed: Option<&FeedState>, options: &Options) -> String {
    let Some(feed) = feed else { return String::new() };
    if options.blank_when_hidden && feed.display != FeedDisplay::Live {
        return String::new();
    }
    let is_song = feed.item.as_ref().is_some_and(|i| i.kind == FeedItemKind::Song);
    let is_scripture = feed.item.as_ref().is_some_and(|i| i.kind == FeedItemKind::Scripture);
    let text = match kind {
        Kind::Lyrics if options.songs_only && !is_song => String::new(),
        Kind::Lyrics => lyrics(&feed.lines, options),
        Kind::Section if is_song => feed.section.clone().unwrap_or_default(),
        Kind::Section if is_scripture => feed.reference.clone().unwrap_or_default(),
        Kind::SongInfo if is_song => feed.song.as_ref().map(|song| fill(&options.template, song, feed, &options.license)).unwrap_or_default(),
        Kind::Section | Kind::SongInfo => String::new(),
    };
    if options.uppercase { text.to_uppercase() } else { text }
}

fn lyrics(lines: &[String], options: &Options) -> String {
    let count = if options.max_lines == 0 { lines.len() } else { options.max_lines.min(lines.len()) };
    lines[..count].join(if options.join_lines { " " } else { "\n" })
}

fn authors(song: &FeedSong, role: Option<&str>) -> String {
    song.authors.iter().filter(|a| role.is_none() || a.role.as_deref() == role).map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
}

fn value(name: &str, song: &FeedSong, feed: &FeedState, license: &str) -> Option<String> {
    Some(match name {
        "title" => song.title.clone(),
        "authors" => authors(song, None),
        "words" => authors(song, Some("words")),
        "music" => authors(song, Some("music")),
        "translation" => authors(song, Some("translation")),
        "copyright" => song.copyright.clone().unwrap_or_default(),
        "ccli" => song.ccli.clone().unwrap_or_default(),
        "publisher" => song.publisher.clone().unwrap_or_default(),
        "year" => song.year.map(|y| y.to_string()).unwrap_or_default(),
        "license" => license.trim().to_string(),
        "section" => feed.section.clone().unwrap_or_default(),
        _ => return None,
    })
}

fn fill_part(part: &str, song: &FeedSong, feed: &FeedState, license: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = part;
    let mut placeholders = 0;
    let mut filled = 0;
    while let Some(start) = rest.find('{') {
        let Some(end) = rest[start..].find('}').map(|e| start + e) else { break };
        out.push_str(&rest[..start]);
        match value(&rest[start + 1..end], song, feed, license) {
            Some(v) => {
                placeholders += 1;
                if !v.is_empty() {
                    filled += 1;
                }
                out.push_str(&v);
            }
            None => out.push_str(&rest[start..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    let out = out.trim().to_string();
    (!out.is_empty() && (placeholders == 0 || filled > 0)).then_some(out)
}

pub fn fill(template: &str, song: &FeedSong, feed: &FeedState, license: &str) -> String {
    template
        .lines()
        .filter_map(|line| {
            let parts: Vec<String> = line.split('·').filter_map(|part| fill_part(part, song, feed, license)).collect();
            (!parts.is_empty()).then(|| parts.join(" · "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
