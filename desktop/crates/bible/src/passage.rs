use crate::books::book;
use crate::reference::{format_reference, parse_reference, END};
use crate::store::Bibles;

const MAX_SLIDES: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Slide {
    pub text: String,
    pub parallel: String,
    pub caption: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Passage {
    pub title: String,
    pub slides: Vec<Slide>,
    pub warning: Option<String>,
}

pub fn passage(bibles: &Bibles, reference: &str, translation: &str, parallel: Option<&str>) -> Passage {
    let spans = match parse_reference(reference) {
        Ok(spans) => spans,
        Err(e) => return Passage { title: reference.trim().to_string(), slides: Vec::new(), warning: Some(e.to_string()) },
    };
    let mut warnings = Vec::new();
    let primary = match bibles.get(translation) {
        Some(bible) => Some(bible),
        None => {
            let fallback = bibles.fallback(None).and_then(|id| bibles.get(&id));
            if let Some(f) = &fallback {
                warnings.push(format!("{} is not on this computer; showing {}", translation.to_uppercase(), f.info.abbreviation));
            }
            fallback
        }
    };
    let Some(primary) = primary else {
        return Passage { title: reference.trim().to_string(), slides: Vec::new(), warning: Some("No Bible is installed".into()) };
    };
    let second = parallel.filter(|p| !p.is_empty() && *p != primary.info.id).and_then(|id| {
        let found = bibles.get(id);
        if found.is_none() {
            warnings.push(format!("{} is not on this computer", id.to_uppercase()));
        }
        found
    });
    let language = primary.info.language.clone();
    let names = match &second {
        Some(s) => format!("{} · {}", primary.info.abbreviation, s.info.abbreviation),
        None => primary.info.abbreviation.clone(),
    };
    let mut slides = Vec::new();
    'spans: for span in &spans {
        let Some(info) = book(span.book) else { continue };
        for chapter in span.from.0..=span.to.0 {
            for verse in primary.chapter(span.book, chapter) {
                let after_start = chapter > span.from.0 || verse.verse >= span.from.1;
                let before_end = chapter < span.to.0 || span.to.1 == END || verse.verse <= span.to.1;
                if !(after_start && before_end) {
                    continue;
                }
                if slides.len() >= MAX_SLIDES {
                    warnings.push(format!("Only the first {MAX_SLIDES} verses are shown"));
                    break 'spans;
                }
                let parallel = second
                    .as_ref()
                    .and_then(|s| s.chapter(span.book, chapter).iter().find(|v| v.verse == verse.verse).map(|v| v.text.clone()))
                    .unwrap_or_default();
                let place = if info.single_chapter() { format!("{}", verse.verse) } else { format!("{chapter}:{}", verse.verse) };
                slides.push(Slide { text: verse.text.clone(), parallel, caption: format!("{} {place} · {names}", info.name(&language)) });
            }
        }
    }
    if slides.is_empty() && warnings.is_empty() {
        warnings.push(format!("{} has no verses for {}", primary.info.abbreviation, reference.trim()));
    }
    Passage { title: format!("{} · {names}", format_reference(&spans, &language)), slides, warning: (!warnings.is_empty()).then(|| warnings.join(". ")) }
}
