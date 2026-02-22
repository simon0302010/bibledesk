use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Translation {
    pub id: String,
    pub name: String,
}

impl Translation {
    pub fn all() -> Vec<Translation> {
        vec![
            Translation { id: "kjv".into(), name: "King James Version".into() },
            Translation { id: "web".into(), name: "World English Bible".into() },
            Translation { id: "asv".into(), name: "American Standard Version".into() },
            Translation { id: "darby".into(), name: "Darby Bible".into() },
            Translation { id: "ylt".into(), name: "Young's Literal Translation".into() },
            Translation { id: "bbe".into(), name: "Bible in Basic English".into() },
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BibleBook {
    pub id: &'static str,
    pub name: &'static str,
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

pub fn bible_books() -> Vec<BibleBook> {
    vec![
        BibleBook { id: "GEN", name: "Genesis", chapters: 50 },
        BibleBook { id: "EXO", name: "Exodus", chapters: 40 },
        BibleBook { id: "LEV", name: "Leviticus", chapters: 27 },
        BibleBook { id: "NUM", name: "Numbers", chapters: 36 },
        BibleBook { id: "DEU", name: "Deuteronomy", chapters: 34 },
        BibleBook { id: "JOS", name: "Joshua", chapters: 24 },
        BibleBook { id: "JDG", name: "Judges", chapters: 21 },
        BibleBook { id: "RUT", name: "Ruth", chapters: 4 },
        BibleBook { id: "1SA", name: "1 Samuel", chapters: 31 },
        BibleBook { id: "2SA", name: "2 Samuel", chapters: 24 },
        BibleBook { id: "1KI", name: "1 Kings", chapters: 22 },
        BibleBook { id: "2KI", name: "2 Kings", chapters: 25 },
        BibleBook { id: "1CH", name: "1 Chronicles", chapters: 29 },
        BibleBook { id: "2CH", name: "2 Chronicles", chapters: 36 },
        BibleBook { id: "EZR", name: "Ezra", chapters: 10 },
        BibleBook { id: "NEH", name: "Nehemiah", chapters: 13 },
        BibleBook { id: "EST", name: "Esther", chapters: 10 },
        BibleBook { id: "JOB", name: "Job", chapters: 42 },
        BibleBook { id: "PSA", name: "Psalms", chapters: 150 },
        BibleBook { id: "PRO", name: "Proverbs", chapters: 31 },
        BibleBook { id: "ECC", name: "Ecclesiastes", chapters: 12 },
        BibleBook { id: "SNG", name: "Song of Solomon", chapters: 8 },
        BibleBook { id: "ISA", name: "Isaiah", chapters: 66 },
        BibleBook { id: "JER", name: "Jeremiah", chapters: 52 },
        BibleBook { id: "LAM", name: "Lamentations", chapters: 5 },
        BibleBook { id: "EZK", name: "Ezekiel", chapters: 48 },
        BibleBook { id: "DAN", name: "Daniel", chapters: 12 },
        BibleBook { id: "HOS", name: "Hosea", chapters: 14 },
        BibleBook { id: "JOL", name: "Joel", chapters: 3 },
        BibleBook { id: "AMO", name: "Amos", chapters: 9 },
        BibleBook { id: "OBA", name: "Obadiah", chapters: 1 },
        BibleBook { id: "JON", name: "Jonah", chapters: 4 },
        BibleBook { id: "MIC", name: "Micah", chapters: 7 },
        BibleBook { id: "NAM", name: "Nahum", chapters: 3 },
        BibleBook { id: "HAB", name: "Habakkuk", chapters: 3 },
        BibleBook { id: "ZEP", name: "Zephaniah", chapters: 3 },
        BibleBook { id: "HAG", name: "Haggai", chapters: 2 },
        BibleBook { id: "ZEC", name: "Zechariah", chapters: 14 },
        BibleBook { id: "MAL", name: "Malachi", chapters: 4 },
        BibleBook { id: "MAT", name: "Matthew", chapters: 28 },
        BibleBook { id: "MRK", name: "Mark", chapters: 16 },
        BibleBook { id: "LUK", name: "Luke", chapters: 24 },
        BibleBook { id: "JHN", name: "John", chapters: 21 },
        BibleBook { id: "ACT", name: "Acts", chapters: 28 },
        BibleBook { id: "ROM", name: "Romans", chapters: 16 },
        BibleBook { id: "1CO", name: "1 Corinthians", chapters: 16 },
        BibleBook { id: "2CO", name: "2 Corinthians", chapters: 13 },
        BibleBook { id: "GAL", name: "Galatians", chapters: 6 },
        BibleBook { id: "EPH", name: "Ephesians", chapters: 6 },
        BibleBook { id: "PHP", name: "Philippians", chapters: 4 },
        BibleBook { id: "COL", name: "Colossians", chapters: 4 },
        BibleBook { id: "1TH", name: "1 Thessalonians", chapters: 5 },
        BibleBook { id: "2TH", name: "2 Thessalonians", chapters: 3 },
        BibleBook { id: "1TI", name: "1 Timothy", chapters: 6 },
        BibleBook { id: "2TI", name: "2 Timothy", chapters: 4 },
        BibleBook { id: "TIT", name: "Titus", chapters: 3 },
        BibleBook { id: "PHM", name: "Philemon", chapters: 1 },
        BibleBook { id: "HEB", name: "Hebrews", chapters: 13 },
        BibleBook { id: "JAS", name: "James", chapters: 5 },
        BibleBook { id: "1PE", name: "1 Peter", chapters: 5 },
        BibleBook { id: "2PE", name: "2 Peter", chapters: 3 },
        BibleBook { id: "1JN", name: "1 John", chapters: 5 },
        BibleBook { id: "2JN", name: "2 John", chapters: 1 },
        BibleBook { id: "3JN", name: "3 John", chapters: 1 },
        BibleBook { id: "JUD", name: "Jude", chapters: 1 },
        BibleBook { id: "REV", name: "Revelation", chapters: 22 },
    ]
}
