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

/// Extract a chapter count from a raw book entry, trying every known field name the
/// getbible.net v2 API might use, and handling both an integer count and an array of
/// chapter objects.
///
/// Known field names observed in the wild: "chapters", "chapter_nr".
fn extract_chapter_count(entry: &serde_json::Value, book_name: &str) -> u32 {
    for key in &["chapters", "chapter_nr"] {
        if let Some(val) = entry.get(key) {
            match val {
                serde_json::Value::Number(n) => return n.as_u64().unwrap_or(0) as u32,
                serde_json::Value::Array(arr) => return arr.len() as u32,
                serde_json::Value::Object(obj) => return obj.len() as u32,
                _ => {}
            }
        }
    }
    eprintln!(
        "[BibleDesk] could not determine chapter count for book '{}'; raw entry: {}",
        book_name, entry
    );
    0
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

        // Parse as a map of raw JSON values so we can probe any field name for the chapter count.
        let raw: HashMap<String, serde_json::Value> =
            parse_body(&body, &format!("{}/books.json", abbr))?;

        let mut books: Vec<BibleBook> = raw
            .into_values()
            .filter_map(|entry| {
                let book_nr = entry.get("nr")?.as_u64()? as u32;
                let name = entry.get("name")?.as_str()?.to_string();
                let chapters = extract_chapter_count(&entry, &name);
                Some(BibleBook {
                    id: book_nr.to_string(),
                    book_nr,
                    name,
                    chapters,
                })
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

