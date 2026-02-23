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
/// `chapters` can be either a plain integer (chapter count) or an array of chapter
/// objects — we normalise both into a u32 count.
#[derive(Debug, Deserialize)]
struct BookEntry {
    /// Book number (1-based)
    nr: u32,
    /// Book name (e.g. "Genesis")
    name: String,
    /// Either u32 count OR array of chapter objects — handled by chapters_as_count()
    #[serde(default)]
    chapters: serde_json::Value,
    // Translation metadata fields echoed back — ignored but must be tolerated
    #[serde(default)]
    translation: String,
    #[serde(default)]
    abbreviation: String,
}

/// Normalise the `chapters` field to a chapter count regardless of whether the API
/// returns it as a plain integer or as an array of chapter objects.
fn chapters_as_count(val: &serde_json::Value, book_name: &str) -> u32 {
    match val {
        serde_json::Value::Number(n) => n.as_u64().unwrap_or(0) as u32,
        serde_json::Value::Array(arr) => arr.len() as u32,
        serde_json::Value::Null => {
            eprintln!("[BibleDesk] chapters field missing for book '{}'", book_name);
            0
        }
        other => {
            eprintln!("[BibleDesk] unexpected chapters type for book '{}': {:?}", book_name, other);
            0
        }
    }
}

// ── Chapter content ───────────────────────────────────────────────────────────

/// The getbible.net v2 chapter endpoint returns verses as a JSON array.
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
    /// Verses are returned as an array, not a map.
    verses: Vec<VerseEntry>,
}

#[derive(Debug, Deserialize)]
struct VerseEntry {
    verse: u32,
    text: String,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse a JSON body, printing the full body to stderr on failure (for debugging),
/// and returning a truncated preview in the user-facing error string.
fn parse_body<T: serde::de::DeserializeOwned>(body: &str, context: &str) -> Result<T, String> {
    serde_json::from_str(body).map_err(|e| {
        // Print full body to stderr for untruncated debugging
        eprintln!("[BibleDesk] {} parse error: {}\nFull response body:\n{}", context, e, body);
        let preview = if body.len() > 300 {
            format!("{}...", &body[..300])
        } else {
            body.to_string()
        };
        format!("Parse error: {} (response preview: {})", e, preview)
    })
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
        let body = resp.text()
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        let raw: HashMap<String, TranslationEntry> = parse_body(&body, "translations.json")?;

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
        let body = resp.text()
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        let raw: HashMap<String, BookEntry> = parse_body(&body, &format!("{}/books.json", abbr))?;

        let mut books: Vec<BibleBook> = raw
            .into_values()
            .map(|b| {
                let chapter_count = chapters_as_count(&b.chapters, &b.name);
                BibleBook {
                    id: b.nr.to_string(),
                    book_nr: b.nr,
                    name: b.name,
                    chapters: chapter_count,
                }
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
        let body = resp.text()
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        let content: ChapterContent = parse_body(
            &body,
            &format!("{}/{}/{}.json", abbr, book_nr, chapter_nr),
        )?;

        let book_id = content.book_nr.to_string();
        let book_name = content.book_name.clone();
        let ch_nr = content.chapter_nr;
        let trans = content.abbreviation.clone();

        let mut verses: Vec<Verse> = content
            .verses
            .into_iter()
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

