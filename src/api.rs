use std::collections::HashMap;
use serde::Deserialize;
use crate::models::{Translation, BibleBook, Verse, Chapter};

const BASE_URL: &str = "https://api.getbible.net/v2";

// ── Translation list ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TranslationEntry {
    translation: String,
}

// ── Book list ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct BookEntry {
    book_nr: u32,
    book_name: String,
    /// Total number of chapters in this book
    chapter_nr: u32,
}

// ── Chapter content ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChapterContent {
    book_nr: u32,
    book_name: String,
    chapter_nr: u32,
    abbreviation: String,
    verses: HashMap<String, VerseEntry>,
}

#[derive(Debug, Deserialize)]
struct VerseEntry {
    verse: u32,
    text: String,
}

// ── Public client ─────────────────────────────────────────────────────────────

pub struct BibleClient;

impl BibleClient {
    /// Fetch the full list of available translations from getbible.net v2.
    pub fn fetch_translations() -> Result<Vec<Translation>, String> {
        let url = format!("{}/translations.json", BASE_URL);
        let resp = reqwest::blocking::get(&url)
            .map_err(|e| format!("Network error: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("API error {}", resp.status()));
        }
        let raw: HashMap<String, TranslationEntry> = resp.json()
            .map_err(|e| format!("Parse error: {}", e))?;

        let mut list: Vec<Translation> = raw
            .into_iter()
            .map(|(abbr, entry)| Translation { id: abbr, name: entry.translation })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(list)
    }

    /// Fetch the list of books (with chapter counts) for a given translation.
    pub fn fetch_books(abbr: &str) -> Result<Vec<BibleBook>, String> {
        let url = format!("{}/{}/books.json", BASE_URL, abbr);
        let resp = reqwest::blocking::get(&url)
            .map_err(|e| format!("Network error: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("API error {}", resp.status()));
        }
        let raw: HashMap<String, BookEntry> = resp.json()
            .map_err(|e| format!("Parse error: {}", e))?;

        let mut books: Vec<BibleBook> = raw
            .into_values()
            .map(|b| BibleBook {
                id: b.book_nr.to_string(),
                book_nr: b.book_nr,
                name: b.book_name,
                chapters: b.chapter_nr,
            })
            .collect();
        books.sort_by_key(|b| b.book_nr);
        Ok(books)
    }

    /// Fetch a single chapter from getbible.net v2.
    pub fn fetch_chapter(abbr: &str, book_nr: u32, chapter_nr: u32) -> Result<Chapter, String> {
        let url = format!("{}/{}/{}/{}.json", BASE_URL, abbr, book_nr, chapter_nr);
        let resp = reqwest::blocking::get(&url)
            .map_err(|e| format!("Network error: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("API error {}", resp.status()));
        }
        let content: ChapterContent = resp.json()
            .map_err(|e| format!("Parse error: {}", e))?;

        let book_id = content.book_nr.to_string();
        let book_name = content.book_name.clone();
        let ch_nr = content.chapter_nr;
        let trans = content.abbreviation.clone();

        let mut verses: Vec<Verse> = content
            .verses
            .into_values()
            .map(|v| Verse {
                book_id: book_id.clone(),
                book_name: book_name.clone(),
                chapter: ch_nr,
                verse: v.verse,
                text: v.text.trim().to_string(),
                translation: trans.clone(),
            })
            .collect();
        verses.sort_by_key(|v| v.verse);

        Ok(Chapter {
            reference: format!("{} {}", book_name, ch_nr),
            book_name,
            chapter_num: ch_nr,
            verses,
            translation: trans,
        })
    }
}
