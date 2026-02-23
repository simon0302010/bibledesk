use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Translation {
    pub id: String,
    pub name: String,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BibleBook {
    pub id: String,
    pub book_nr: u32,
    pub name: String,
    pub chapters: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verse {
    pub book_id: String,
    pub book_name: String,
    pub chapter: u32,
    pub verse: u32,
    pub text: String,
    pub translation: String,
}

#[derive(Debug, Clone)]
pub struct Chapter {
    pub reference: String,
    pub book_name: String,
    pub chapter_num: u32,
    pub verses: Vec<Verse>,
    pub translation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedVerse {
    pub id: i64,
    pub book_id: String,
    pub book_name: String,
    pub chapter: u32,
    pub verse: u32,
    pub text: String,
    pub translation: String,
    pub note: String,
    pub saved_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingPlan {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingPlanEntry {
    pub id: i64,
    pub plan_id: i64,
    pub book_name: String,
    pub chapter: u32,
    pub scheduled_date: Option<String>,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCard {
    pub id: i64,
    pub book_id: String,
    pub book_name: String,
    pub chapter: u32,
    pub verse: u32,
    pub text: String,
    pub translation: String,
    pub successes: i64,
    pub failures: i64,
    pub next_review: Option<String>,
    pub last_reviewed: Option<String>,
}

