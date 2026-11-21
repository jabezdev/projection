use super::{Section, SectionKind};

impl Section {
    pub fn slides(&self) -> Vec<String> {
        split_slides(&self.lyrics)
    }

    pub fn code(&self) -> String {
        let prefix = match self.kind {
            SectionKind::Verse => "V".to_string(),
            SectionKind::PreChorus => "PC".to_string(),
            SectionKind::Chorus => "C".to_string(),
            SectionKind::Bridge => "BR".to_string(),
            SectionKind::Intro => "I".to_string(),
            SectionKind::Outro => "O".to_string(),
            SectionKind::Tag => "T".to_string(),
            SectionKind::Other => self.name.as_deref().and_then(|n| n.trim().chars().next()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "P".into()),
        };
        format!("{prefix}{}", self.number.map(|n| n.to_string()).unwrap_or_default())
    }

    pub fn color(&self) -> &str {
        if let Some(color) = self.color.as_deref().filter(|c| !c.is_empty()) {
            return color;
        }
        match self.kind {
            SectionKind::Verse => "#4f91e8",
            SectionKind::Chorus => "#e76774",
            SectionKind::PreChorus => "#efa34a",
            SectionKind::Bridge => "#aa84df",
            SectionKind::Intro => "#50c6d6",
            SectionKind::Outro => "#ddbd53",
            SectionKind::Tag => "#79c56b",
            SectionKind::Other => "#a3a8b2",
        }
    }
}

pub fn split_slides(lyrics: &str) -> Vec<String> {
    let text = lyrics.replace("\r\n", "\n").replace('\r', "\n");
    let mut slides = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        if line.trim().is_empty() {
            if !current.is_empty() {
                slides.push(current.join("\n"));
                current.clear();
            }
        } else {
            current.push(line.trim());
        }
    }
    if !current.is_empty() {
        slides.push(current.join("\n"));
    }
    slides
}
