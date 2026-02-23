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
//
// GET /{abbr}/books.json → object keyed by ordinal; each value has: abbreviation,
// direction, encoding, lang, language, name, nr, sha, translation, url.
// No chapter count is provided; the chapter count comes from fetching the full book.

// ── Book content ─────────────────────────────────────────────────────────────
//
// GET /{abbr}/{book_nr}.json → the full book with all chapters as an array:
//
//   {
//     "abbreviation": "elberfelder1905",
//     "nr": 36,
//     "name": "Zefanja",
//     "chapters": [
//       { "chapter": 1, "name": "Zefanja 1", "verses": [{ "verse": 1, "text": "..." }, ...] },
//       ...
//     ]
//   }

#[derive(Debug, Deserialize)]
struct BookContent {
    #[serde(default)]
    abbreviation: String,
    #[serde(default)]
    nr: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    chapters: Vec<ChapterEntry>,
}

#[derive(Debug, Deserialize)]
struct ChapterEntry {
    chapter: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
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
            .map(|entry| Translation {
                id: entry.abbreviation,
                name: entry.translation,
                language: entry.language,
            })
            .collect();
        list.sort_by(|a, b| a.language.cmp(&b.language).then(a.name.cmp(&b.name)));
        Ok(list)
    }

    /// Fetch the list of books for a given translation.
    /// Note: the books.json endpoint does not include chapter counts.
    pub fn fetch_books(abbr: &str) -> Result<Vec<BibleBook>, String> {
        let url = format!("{}/{}/books.json", BASE_URL, abbr);
        let resp = reqwest::blocking::get(&url)
            .map_err(|e| format!("Network error: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("API error {}", resp.status()));
        }
        let body = resp.text()
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        let raw: HashMap<String, serde_json::Value> =
            parse_body(&body, &format!("{}/books.json", abbr))?;

        let mut books: Vec<BibleBook> = raw
            .into_values()
            .filter_map(|entry| {
                let book_nr = entry.get("nr")?.as_u64()? as u32;
                let name = entry.get("name")?.as_str()?.to_string();
                Some(BibleBook {
                    id: book_nr.to_string(),
                    book_nr,
                    name,
                    chapters: 0, // chapter count unknown until book is fetched
                })
            })
            .collect();
        books.sort_by_key(|b| b.book_nr);
        Ok(books)
    }

    /// Fetch a chapter from getbible.net v2.
    ///
    /// The API endpoint `/{abbr}/{book_nr}.json` returns the **entire book** with all
    /// chapters. We parse the whole book, extract the requested chapter, and also
    /// return all other chapters' verses so the caller can cache them for free.
    ///
    /// Returns `(requested_chapter, all_other_verses, total_chapter_count)`.
    pub fn fetch_chapter(
        abbr: &str,
        book_nr: u32,
        chapter_nr: u32,
    ) -> Result<(Chapter, Vec<Verse>, u32), String> {
        let url = format!("{}/{}/{}.json", BASE_URL, abbr, book_nr);
        let resp = reqwest::blocking::get(&url)
            .map_err(|e| format!("Network error: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("API error {}", resp.status()));
        }
        let body = resp.text()
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        let content: BookContent = parse_body(&body, &format!("{}/{}.json", abbr, book_nr))?;

        let book_id = content.nr.to_string();
        let book_name = content.name.clone();
        let trans = content.abbreviation.clone();
        let total_chapters = content.chapters.len() as u32;

        let mut requested: Option<Chapter> = None;
        let mut all_other_verses: Vec<Verse> = Vec::new();

        for ch in content.chapters {
            let ch_nr = ch.chapter;
            let mut verses: Vec<Verse> = ch.verses
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

            if ch_nr == chapter_nr {
                requested = Some(Chapter {
                    reference: format!("{} {}", book_name, ch_nr),
                    book_name: book_name.clone(),
                    chapter_num: ch_nr,
                    verses,
                    translation: trans.clone(),
                });
            } else {
                all_other_verses.extend(verses);
            }
        }

        let chapter = requested.ok_or_else(|| {
            format!("Chapter {} not found in {} (book has {} chapters)",
                chapter_nr, book_name, total_chapters)
        })?;

        Ok((chapter, all_other_verses, total_chapters))
    }

    /// Download every chapter of a book and return all verses for caching.
    /// This fetches `/{abbr}/{book_nr}.json` (same endpoint as `fetch_chapter`) and
    /// returns all verses in all chapters — no scripture is generated; it all comes
    /// from the API.
    pub fn download_book(abbr: &str, book_nr: u32) -> Result<Vec<Verse>, String> {
        let url = format!("{}/{}/{}.json", BASE_URL, abbr, book_nr);
        let resp = reqwest::blocking::get(&url)
            .map_err(|e| format!("Network error: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("API error {}", resp.status()));
        }
        let body = resp.text()
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        let content: BookContent = parse_body(&body, &format!("{}/{}.json", abbr, book_nr))?;

        let book_id = content.nr.to_string();
        let book_name = content.name.clone();
        let trans = content.abbreviation.clone();

        let mut all_verses: Vec<Verse> = Vec::new();
        for ch in content.chapters {
            let ch_nr = ch.chapter;
            for v in ch.verses {
                all_verses.push(Verse {
                    book_id: book_id.clone(),
                    book_name: book_name.clone(),
                    chapter: ch_nr,
                    verse: v.verse,
                    text: v.text.trim().to_string(),
                    translation: trans.clone(),
                });
            }
        }
        Ok(all_verses)
    }
}


