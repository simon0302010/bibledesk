use std::collections::HashMap;
use serde::Deserialize;
use crate::models::{Translation, BibleBook, Verse, Chapter};

const BASE_URL: &str = "https://api.getbible.net/v2";

// ── Translation list ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TranslationEntry {
    translation: String,
    abbreviation: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    direction: String,
    #[serde(default)]
    encoding: String,
    #[serde(default)]
    nr: u32,
    #[serde(default)]
    name: String,
}

// ── Book list ─────────────────────────────────────────────────────────────────

/// Each entry in `/{abbr}/books.json` — field names as returned by getbible.net v2.
/// The response also echoes back translation metadata per entry, which we ignore.
#[derive(Debug, Deserialize)]
struct BookEntry {
    /// Book number (1-based)
    nr: u32,
    /// Book name (e.g. "Genesis")
    name: String,
    /// Total number of chapters in this book
    #[serde(default)]
    chapters: u32,
    // Translation metadata fields echoed back — ignored but must be tolerated
    #[serde(default)]
    translation: String,
    #[serde(default)]
    abbreviation: String,
}

// ── Chapter content ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChapterContent {
    #[serde(default)]
    book_nr: u32,
    #[serde(default)]
    book_name: String,
    #[serde(default)]
    chapter_nr: u32,
    #[serde(default)]
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

        // Get response text for better error reporting
        let body = resp.text()
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        // Try parsing as HashMap (expected format)
        let raw: HashMap<String, TranslationEntry> = serde_json::from_str(&body)
            .map_err(|e| {
                let preview = if body.len() > 200 {
                    format!("{}...", &body[..200])
                } else {
                    body.clone()
                };
                format!("Parse error: {} (response preview: {})", e, preview)
            })?;

        let mut list: Vec<Translation> = raw
            .into_values()
            .map(|entry| Translation { id: entry.abbreviation, name: entry.translation })
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

        // Get response text for better error reporting
        let body = resp.text()
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        // Try parsing as HashMap (expected format)
        let raw: HashMap<String, BookEntry> = serde_json::from_str(&body)
            .map_err(|e| {
                let preview = if body.len() > 200 {
                    format!("{}...", &body[..200])
                } else {
                    body.clone()
                };
                format!("Parse error: {} (response preview: {})", e, preview)
            })?;

        let mut books: Vec<BibleBook> = raw
            .into_values()
            .map(|b| BibleBook {
                id: b.nr.to_string(),
                book_nr: b.nr,
                name: b.name,
                chapters: b.chapters,
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

        // Get response text for better error reporting
        let body = resp.text()
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        // Try parsing as ChapterContent (expected format)
        let content: ChapterContent = serde_json::from_str(&body)
            .map_err(|e| {
                let preview = if body.len() > 200 {
                    format!("{}...", &body[..200])
                } else {
                    body.clone()
                };
                format!("Parse error: {} (response preview: {})", e, preview)
            })?;

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
