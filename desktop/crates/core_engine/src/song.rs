//! `magi.song.v1` documents normalized for scenes. Mirrors the operator's
//! `song.ts` exactly; both are checked against `desktop/fixtures/song-normalization.json`.

use serde::Deserialize;

pub const SONG_FORMAT: &str = "magi.song.v1";
const DEFAULT_PART_COLOR: &str = "#a3a8b2";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SongPart {
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "type")]
    pub part_type: String,
    #[serde(default)]
    pub number: Option<serde_json::Number>,
    #[serde(default)]
    pub lyrics: String,
    /// Stored slides override splitting `lyrics`, even when empty.
    #[serde(default, rename = "slideTexts")]
    pub slide_texts: Option<Vec<String>>,
    #[serde(default)]
    pub color: Option<String>,
}

impl SongPart {
    pub fn slides(&self) -> Vec<String> {
        self.slide_texts.clone().unwrap_or_else(|| split_slides(&self.lyrics))
    }

    /// Short label such as `V1`, `PC1` or `BR2`.
    pub fn code(&self) -> String {
        let prefix = match self.part_type.as_str() {
            "Bridge" => "BR".to_string(),
            "Pre-chorus" => "PC".to_string(),
            other => other.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default(),
        };
        let number = self.number.as_ref().map(ToString::to_string).unwrap_or_default();
        format!("{prefix}{number}")
    }

    pub fn color(&self) -> &str {
        if let Some(color) = self.color.as_deref().filter(|c| !c.is_empty()) {
            return color;
        }
        match self.part_type.as_str() {
            "Verse" => "#4f91e8",
            "Chorus" => "#e76774",
            "Pre-chorus" => "#efa34a",
            "Bridge" => "#aa84df",
            "Intro" => "#50c6d6",
            "Outro" => "#ddbd53",
            "Tag" => "#79c56b",
            _ => DEFAULT_PART_COLOR,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Song {
    pub parts: Vec<SongPart>,
    /// Part ids in performance order; repeats are separate occurrences.
    pub arrangement: Vec<String>,
    pub background: String,
}

/// One appearance of a part in the arrangement, with its place in the
/// flattened step list. Scene identity is `(item, occurrence, slide)`.
#[derive(Debug, Clone, PartialEq)]
pub struct PartOccurrence<'a> {
    pub occurrence: usize,
    pub part: &'a SongPart,
    pub slides: Vec<String>,
    /// Index of this occurrence's first slide in [`Song::steps`].
    pub first_step: usize,
}

#[derive(Deserialize)]
struct SongDocument {
    format: String,
    #[serde(default, rename = "slideSeparator")]
    slide_separator: Option<String>,
    parts: Vec<SongPart>,
    arrangement: Vec<String>,
    #[serde(default)]
    background: String,
}

impl Song {
    /// Parses a stored song. Anything that is not a song document is treated
    /// as plain lyrics in a single verse, as older libraries stored them.
    pub fn decode(text: &str) -> Self {
        match serde_json::from_str::<SongDocument>(text) {
            Ok(doc) if doc.format == SONG_FORMAT => {
                let migrate = doc.slide_separator.as_deref() != Some("blank-line");
                let parts = doc
                    .parts
                    .into_iter()
                    .map(|part| if migrate { migrate_single_line_part(part) } else { part })
                    .collect();
                Song { parts, arrangement: doc.arrangement, background: doc.background }
            }
            _ => Song {
                parts: vec![SongPart {
                    id: "legacy".into(),
                    part_type: "Verse".into(),
                    number: Some(1.into()),
                    lyrics: text.to_string(),
                    slide_texts: Some(split_slides(text)),
                    color: None,
                }],
                arrangement: vec!["legacy".into()],
                background: String::new(),
            },
        }
    }

    /// Arranged occurrences; ids missing from `parts` are skipped.
    pub fn occurrences(&self) -> Vec<PartOccurrence<'_>> {
        let mut first_step = 0;
        let mut result = Vec::new();
        for id in &self.arrangement {
            let Some(part) = self.parts.iter().find(|part| &part.id == id) else { continue };
            let slides = part.slides();
            let occurrence = PartOccurrence { occurrence: result.len(), part, first_step, slides };
            first_step += occurrence.slides.len();
            result.push(occurrence);
        }
        result
    }

    /// Flattened slides in performance order: what the service item steps through.
    pub fn steps(&self) -> Vec<String> {
        self.occurrences().into_iter().flat_map(|o| o.slides).collect()
    }

    /// The `(occurrence, slide within it)` shown at a flattened step.
    pub fn locate(&self, step: usize) -> Option<(usize, usize)> {
        self.occurrences()
            .into_iter()
            .find(|o| step >= o.first_step && step < o.first_step + o.slides.len())
            .map(|o| (o.occurrence, step - o.first_step))
    }
}

/// Songs saved by the former single-line editor stored one slide per line.
fn migrate_single_line_part(mut part: SongPart) -> SongPart {
    let slides = part.slide_texts.take().unwrap_or_else(|| {
        part.lyrics.split('\n').map(|line| js_trim(line).to_string()).filter(|line| !line.is_empty()).collect()
    });
    part.lyrics = slides.join("\n\n");
    part
}

/// Lines within a slide stay together; a blank or space/tab-only line starts
/// the next slide. Empty slides are dropped.
pub fn split_slides(lyrics: &str) -> Vec<String> {
    let text = lyrics.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut slides = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        // Only lines between two line breaks separate slides (as in `\n[ \t]*\n`).
        let interior = index > 0 && index + 1 < lines.len();
        if interior && line.chars().all(|c| c == ' ' || c == '\t') {
            slides.push(current.join("\n"));
            current.clear();
        } else {
            current.push(line);
        }
    }
    slides.push(current.join("\n"));
    slides.into_iter().map(|slide| js_trim(&slide).to_string()).filter(|slide| !slide.is_empty()).collect()
}

/// `String.prototype.trim`: Unicode whitespace plus the byte-order mark.
fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Fixtures {
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        song: String,
        steps: Vec<String>,
        occurrences: Vec<ExpectedOccurrence>,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct ExpectedOccurrence {
        part: String,
        code: String,
        color: String,
        slides: Vec<String>,
    }

    #[test]
    fn shared_fixtures_match_the_operator_parser() {
        let text = include_str!("../../../fixtures/song-normalization.json");
        let fixtures: Fixtures = serde_json::from_str(text).unwrap();
        assert!(!fixtures.cases.is_empty());
        for case in fixtures.cases {
            let song = Song::decode(&case.song);
            assert_eq!(song.steps(), case.steps, "{}", case.name);
            let occurrences: Vec<_> = song
                .occurrences()
                .into_iter()
                .map(|o| ExpectedOccurrence {
                    part: o.part.id.clone(),
                    code: o.part.code(),
                    color: o.part.color().to_string(),
                    slides: o.slides,
                })
                .collect();
            assert_eq!(occurrences, case.occurrences, "{}", case.name);
        }
    }

    #[test]
    fn repeated_parts_have_distinct_occurrence_identity() {
        let song = Song::decode(
            r#"{"format":"magi.song.v1","slideSeparator":"blank-line","background":"",
                "parts":[{"id":"v","type":"Verse","number":1,"lyrics":"a\n\nb"},{"id":"c","type":"Chorus","number":1,"lyrics":"c"}],
                "arrangement":["v","c","v"]}"#,
        );
        assert_eq!(song.steps(), ["a", "b", "c", "a", "b"]);
        assert_eq!(song.locate(1), Some((0, 1)));
        assert_eq!(song.locate(2), Some((1, 0)));
        // The second verse is a different occurrence of the same part.
        assert_eq!(song.locate(4), Some((2, 1)));
        assert_eq!(song.locate(5), None);
    }
}
