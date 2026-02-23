use rusqlite::{Connection, Result, params};
use crate::models::{Verse, SavedVerse, ReadingPlan, ReadingPlanEntry, MemoryCard};
use chrono::Local;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Database { conn };
        db.initialize()?;
        Ok(db)
    }

    fn initialize(&self) -> Result<()> {
        self.conn.execute_batch("
            PRAGMA journal_mode=WAL;

            CREATE TABLE IF NOT EXISTS verses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                book_id TEXT NOT NULL,
                book_name TEXT NOT NULL,
                chapter INTEGER NOT NULL,
                verse INTEGER NOT NULL,
                text TEXT NOT NULL,
                translation TEXT NOT NULL,
                cached_at TEXT NOT NULL,
                UNIQUE(book_id, chapter, verse, translation)
            );
            CREATE INDEX IF NOT EXISTS idx_verses_ref ON verses(book_id, chapter, verse, translation);
            CREATE INDEX IF NOT EXISTS idx_verses_text ON verses(text);
            CREATE INDEX IF NOT EXISTS idx_verses_translation ON verses(translation);

            CREATE TABLE IF NOT EXISTS saved_verses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                book_id TEXT NOT NULL,
                book_name TEXT NOT NULL,
                chapter INTEGER NOT NULL,
                verse INTEGER NOT NULL,
                text TEXT NOT NULL,
                translation TEXT NOT NULL,
                note TEXT NOT NULL DEFAULT '',
                saved_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS reading_plans (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS reading_plan_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                plan_id INTEGER NOT NULL REFERENCES reading_plans(id) ON DELETE CASCADE,
                book_name TEXT NOT NULL,
                chapter INTEGER NOT NULL,
                scheduled_date TEXT,
                completed INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS memory_cards (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                book_id TEXT NOT NULL,
                book_name TEXT NOT NULL,
                chapter INTEGER NOT NULL,
                verse INTEGER NOT NULL,
                text TEXT NOT NULL,
                translation TEXT NOT NULL,
                successes INTEGER NOT NULL DEFAULT 0,
                failures INTEGER NOT NULL DEFAULT 0,
                next_review TEXT,
                last_reviewed TEXT,
                UNIQUE(book_id, chapter, verse, translation)
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
        ")?;
        Ok(())
    }

    // Verse caching
    pub fn cache_verses(&self, verses: &[Verse]) -> Result<()> {
        let now = Local::now().to_rfc3339();
        for v in verses {
            self.conn.execute(
                "INSERT OR REPLACE INTO verses (book_id, book_name, chapter, verse, text, translation, cached_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![v.book_id, v.book_name, v.chapter, v.verse, v.text, v.translation, now],
            )?;
        }
        Ok(())
    }

    pub fn get_cached_chapter(&self, book_name: &str, chapter: u32, translation: &str) -> Result<Vec<Verse>> {
        let mut stmt = self.conn.prepare(
            "SELECT book_id, book_name, chapter, verse, text, translation FROM verses
             WHERE book_name = ?1 AND chapter = ?2 AND translation = ?3
             ORDER BY verse"
        )?;
        let verses = stmt.query_map(params![book_name, chapter, translation], |row| {
            Ok(Verse {
                book_id: row.get(0)?,
                book_name: row.get(1)?,
                chapter: row.get(2)?,
                verse: row.get(3)?,
                text: row.get(4)?,
                translation: row.get(5)?,
            })
        })?.collect::<Result<Vec<_>>>()?;
        Ok(verses)
    }

    // Search — two separate branches to avoid conflicting stmt borrows
    pub fn search_verses(&self, query: &str, translation: Option<&str>) -> Result<Vec<Verse>> {
        let like_query = format!("%{}%", query);
        if let Some(trans) = translation {
            let mut stmt = self.conn.prepare(
                "SELECT book_id, book_name, chapter, verse, text, translation FROM verses
                 WHERE text LIKE ?1 AND translation = ?2
                 ORDER BY book_id, chapter, verse LIMIT 200"
            )?;
            let verses = stmt.query_map(params![like_query, trans], |row| {
                Ok(Verse {
                    book_id: row.get(0)?,
                    book_name: row.get(1)?,
                    chapter: row.get(2)?,
                    verse: row.get(3)?,
                    text: row.get(4)?,
                    translation: row.get(5)?,
                })
            })?.collect::<Result<Vec<_>>>()?;
            Ok(verses)
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT book_id, book_name, chapter, verse, text, translation FROM verses
                 WHERE text LIKE ?1
                 ORDER BY book_id, chapter, verse LIMIT 200"
            )?;
            let verses = stmt.query_map(params![like_query], |row| {
                Ok(Verse {
                    book_id: row.get(0)?,
                    book_name: row.get(1)?,
                    chapter: row.get(2)?,
                    verse: row.get(3)?,
                    text: row.get(4)?,
                    translation: row.get(5)?,
                })
            })?.collect::<Result<Vec<_>>>()?;
            Ok(verses)
        }
    }

    // Saved verses
    pub fn save_verse(&self, verse: &Verse, note: &str) -> Result<i64> {
        let now = Local::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO saved_verses (book_id, book_name, chapter, verse, text, translation, note, saved_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![verse.book_id, verse.book_name, verse.chapter, verse.verse, verse.text, verse.translation, note, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_saved_verses(&self) -> Result<Vec<SavedVerse>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, book_id, book_name, chapter, verse, text, translation, note, saved_at
             FROM saved_verses ORDER BY saved_at DESC"
        )?;
        let result = stmt.query_map([], |row| {
            Ok(SavedVerse {
                id: row.get(0)?,
                book_id: row.get(1)?,
                book_name: row.get(2)?,
                chapter: row.get(3)?,
                verse: row.get(4)?,
                text: row.get(5)?,
                translation: row.get(6)?,
                note: row.get(7)?,
                saved_at: row.get(8)?,
            })
        })?.collect::<Result<Vec<_>>>()?;
        Ok(result)
    }

    pub fn update_saved_verse_note(&self, id: i64, note: &str) -> Result<()> {
        self.conn.execute("UPDATE saved_verses SET note = ?1 WHERE id = ?2", params![note, id])?;
        Ok(())
    }

    pub fn delete_saved_verse(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM saved_verses WHERE id = ?1", params![id])?;
        Ok(())
    }

    // Reading plans
    pub fn create_reading_plan(&self, name: &str) -> Result<i64> {
        let now = Local::now().to_rfc3339();
        self.conn.execute("INSERT INTO reading_plans (name, created_at) VALUES (?1, ?2)", params![name, now])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_reading_plans(&self) -> Result<Vec<ReadingPlan>> {
        let mut stmt = self.conn.prepare("SELECT id, name, created_at FROM reading_plans ORDER BY created_at DESC")?;
        let result = stmt.query_map([], |row| {
            Ok(ReadingPlan { id: row.get(0)?, name: row.get(1)?, created_at: row.get(2)? })
        })?.collect::<Result<Vec<_>>>()?;
        Ok(result)
    }

    pub fn delete_reading_plan(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM reading_plan_entries WHERE plan_id = ?1", params![id])?;
        self.conn.execute("DELETE FROM reading_plans WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn add_plan_entry(&self, plan_id: i64, book_name: &str, chapter: u32, date: Option<&str>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO reading_plan_entries (plan_id, book_name, chapter, scheduled_date, completed)
             VALUES (?1, ?2, ?3, ?4, 0)",
            params![plan_id, book_name, chapter, date],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_plan_entries(&self, plan_id: i64) -> Result<Vec<ReadingPlanEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, plan_id, book_name, chapter, scheduled_date, completed
             FROM reading_plan_entries WHERE plan_id = ?1 ORDER BY id"
        )?;
        let result = stmt.query_map(params![plan_id], |row| {
            Ok(ReadingPlanEntry {
                id: row.get(0)?,
                plan_id: row.get(1)?,
                book_name: row.get(2)?,
                chapter: row.get(3)?,
                scheduled_date: row.get(4)?,
                completed: row.get::<_, i64>(5)? != 0,
            })
        })?.collect::<Result<Vec<_>>>()?;
        Ok(result)
    }

    pub fn toggle_entry_complete(&self, entry_id: i64, completed: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE reading_plan_entries SET completed = ?1 WHERE id = ?2",
            params![completed as i64, entry_id],
        )?;
        Ok(())
    }

    pub fn delete_plan_entry(&self, entry_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM reading_plan_entries WHERE id = ?1", params![entry_id])?;
        Ok(())
    }

    // Memory cards (flashcards)
    pub fn add_memory_card(&self, verse: &Verse) -> Result<i64> {
        self.conn.execute(
            "INSERT OR IGNORE INTO memory_cards (book_id, book_name, chapter, verse, text, translation, successes, failures)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0)",
            params![verse.book_id, verse.book_name, verse.chapter, verse.verse, verse.text, verse.translation],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_memory_cards(&self) -> Result<Vec<MemoryCard>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, book_id, book_name, chapter, verse, text, translation, successes, failures, next_review, last_reviewed
             FROM memory_cards ORDER BY CASE WHEN next_review IS NULL THEN 0 ELSE 1 END, next_review ASC"
        )?;
        let result = stmt.query_map([], |row| {
            Ok(MemoryCard {
                id: row.get(0)?,
                book_id: row.get(1)?,
                book_name: row.get(2)?,
                chapter: row.get(3)?,
                verse: row.get(4)?,
                text: row.get(5)?,
                translation: row.get(6)?,
                successes: row.get(7)?,
                failures: row.get(8)?,
                next_review: row.get(9)?,
                last_reviewed: row.get(10)?,
            })
        })?.collect::<Result<Vec<_>>>()?;
        Ok(result)
    }

    pub fn update_memory_card_result(&self, id: i64, success: bool) -> Result<()> {
        let now = Local::now().to_rfc3339();
        if success {
            let mut stmt = self.conn.prepare("SELECT successes FROM memory_cards WHERE id = ?1")?;
            let successes: i64 = stmt.query_row(params![id], |r| r.get(0))?;
            let days = (successes + 1).min(30);
            let next = (Local::now() + chrono::Duration::days(days)).to_rfc3339();
            self.conn.execute(
                "UPDATE memory_cards SET successes = successes + 1, last_reviewed = ?1, next_review = ?2 WHERE id = ?3",
                params![now, next, id],
            )?;
        } else {
            let next = (Local::now() + chrono::Duration::days(1)).to_rfc3339();
            self.conn.execute(
                "UPDATE memory_cards SET failures = failures + 1, last_reviewed = ?1, next_review = ?2 WHERE id = ?3",
                params![now, next, id],
            )?;
        }
        Ok(())
    }

    pub fn delete_memory_card(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM memory_cards WHERE id = ?1", params![id])?;
        Ok(())
    }

    // Settings
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let result = self.conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        );
        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }
}
