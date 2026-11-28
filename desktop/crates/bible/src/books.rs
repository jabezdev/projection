use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Book {
    pub code: String,
    pub osis: String,
    pub chapters: u16,
    pub en: String,
    pub tl: String,
    pub aliases: Vec<String>,
}

impl Book {
    pub fn name(&self, language: &str) -> &str {
        if language.starts_with("tl") || language.starts_with("fil") { &self.tl } else { &self.en }
    }

    pub fn single_chapter(&self) -> bool {
        self.chapters == 1
    }
}

pub fn books() -> &'static [Book] {
    static BOOKS: OnceLock<Vec<Book>> = OnceLock::new();
    BOOKS.get_or_init(|| serde_json::from_str(include_str!("../../../fixtures/bible-books.json")).expect("valid bible-books.json"))
}

pub fn book(index: u8) -> Option<&'static Book> {
    books().get(index as usize)
}

pub(crate) fn by_osis(osis: &str) -> Option<u8> {
    books().iter().position(|b| b.osis.eq_ignore_ascii_case(osis)).map(|i| i as u8)
}

pub(crate) fn by_code(code: &str) -> Option<u8> {
    books().iter().position(|b| b.code.eq_ignore_ascii_case(code)).map(|i| i as u8)
}
