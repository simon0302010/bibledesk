use crate::models::{Verse, Chapter};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ApiVerse {
    book_id: String,
    book_name: String,
    chapter: u32,
    verse: u32,
    text: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    reference: String,
    verses: Vec<ApiVerse>,
    translation_id: String,
}

pub struct BibleApiClient;

impl BibleApiClient {
    pub fn fetch_chapter(book_name: &str, chapter: u32, translation: &str) -> Result<Chapter, String> {
        let query = format!("{}+{}", book_name.replace(' ', "+"), chapter);
        let url = format!("https://bible-api.com/{}?translation={}", query, translation);

        let response = reqwest::blocking::get(&url)
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("API error: {}", response.status()));
        }

        let api_resp: ApiResponse = response.json()
            .map_err(|e| format!("Parse error: {}", e))?;

        let verses: Vec<Verse> = api_resp.verses.into_iter().map(|v| Verse {
            book_id: v.book_id,
            book_name: v.book_name,
            chapter: v.chapter,
            verse: v.verse,
            text: v.text.trim().to_string(),
            translation: translation.to_string(),
        }).collect();

        let book_name_out = verses.first().map(|v| v.book_name.clone()).unwrap_or_default();

        Ok(Chapter {
            reference: api_resp.reference,
            book_name: book_name_out,
            chapter_num: chapter,
            verses,
            translation: translation.to_string(),
        })
    }

    pub fn fetch_verse(book_name: &str, chapter: u32, verse: u32, translation: &str) -> Result<Verse, String> {
        let query = format!("{}+{}:{}", book_name.replace(' ', "+"), chapter, verse);
        let url = format!("https://bible-api.com/{}?translation={}", query, translation);

        let response = reqwest::blocking::get(&url)
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("API error: {}", response.status()));
        }

        let api_resp: ApiResponse = response.json()
            .map_err(|e| format!("Parse error: {}", e))?;

        api_resp.verses.into_iter().next()
            .map(|v| Verse {
                book_id: v.book_id,
                book_name: v.book_name,
                chapter: v.chapter,
                verse: v.verse,
                text: v.text.trim().to_string(),
                translation: translation.to_string(),
            })
            .ok_or_else(|| "No verses returned".to_string())
    }
}
