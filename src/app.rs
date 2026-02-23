use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui::{self, RichText, Color32, ScrollArea, TextEdit, Ui};

use crate::api::BibleClient;
use crate::db::Database;
use crate::localization::{Language, Locale};
use crate::models::{
    BibleBook, Chapter, MemoryCard, ReadingPlan, ReadingPlanEntry,
    SavedVerse, Translation, Verse,
};
use crate::settings::Settings;

#[derive(Debug, PartialEq, Clone)]
enum Tab {
    Reader,
    Search,
    ReadingPlans,
    SavedVerses,
    Flashcards,
    Settings,
}

#[derive(Debug, PartialEq, Clone)]
enum FlashcardMode {
    ReferenceToVerse,
    VerseToReference,
}

#[derive(Debug, PartialEq, Clone)]
enum FlashcardView {
    CardList,
    Study,
}

pub struct BibleDeskApp {
    // Core
    db: Arc<Mutex<Database>>,
    settings: Settings,
    locale: Locale,

    // Navigation
    current_tab: Tab,

    // Catalog loading (translations + books fetched from API)
    catalog_loading: bool,
    catalog_receiver: Option<Receiver<Result<(Vec<Translation>, Vec<BibleBook>), String>>>,
    catalog_error: Option<String>,

    // Reader state
    selected_book_idx: usize,
    selected_chapter: u32,
    current_chapter: Option<Chapter>,
    chapter_loading: bool,
    chapter_receiver: Option<Receiver<Result<(Chapter, u32), String>>>,
    reader_status: String,
    // Book download
    book_downloading: bool,
    book_download_receiver: Option<Receiver<Result<usize, String>>>,

    // Search state
    search_query: String,
    search_results: Vec<Verse>,
    search_translation_filter: String,

    // Reading plan state
    reading_plans: Vec<ReadingPlan>,
    selected_plan_id: Option<i64>,
    plan_entries: Vec<ReadingPlanEntry>,
    new_plan_name: String,
    plan_entry_book_idx: usize,
    plan_entry_chapter: String,
    plan_entry_date: String,

    // Saved verses state
    saved_verses: Vec<SavedVerse>,
    saved_filter: String,
    editing_note_id: Option<i64>,
    editing_note_text: String,

    // Flashcard state
    memory_cards: Vec<MemoryCard>,
    flashcard_view: FlashcardView,
    flashcard_mode: FlashcardMode,
    current_card_idx: Option<usize>,
    flashcard_answer: String,
    flashcard_result: Option<bool>,

    // Settings UI state
    settings_saved_msg: bool,
    settings_saved_timer: f64,
    settings_lang_temp: Language,
    settings_translation_temp: String,
    settings_dark_temp: bool,

    // Static data
    translations: Vec<Translation>,
    books: Vec<BibleBook>,

    // Verse marks: (book_name, chapter, verse, translation) → color name
    verse_marks: std::collections::HashMap<(String, u32, u32, String), String>,
    // Word marks: (book_name, chapter, verse, translation, word_idx) → color name
    word_marks: std::collections::HashMap<(String, u32, u32, String, u32), String>,
    // Currently active marker color (None = marker off)
    active_marker_color: Option<&'static str>,
    // True while the primary mouse button is held during a drag-mark operation
    marker_drag_active: bool,
}

impl BibleDeskApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let db_path = app_db_path();
        let db = match Database::open(db_path.to_str().unwrap_or("bibledesk.db")) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Error: failed to open database at '{}': {e}", db_path.display());
                std::process::exit(1);
            }
        };

        let mut settings = Settings::default();

        if let Ok(Some(lang)) = db.get_setting("language") {
            settings.language = Language::from_code(&lang);
        }
        if let Ok(Some(trans)) = db.get_setting("default_translation") {
            settings.default_translation = trans;
        }
        if let Ok(Some(dark)) = db.get_setting("dark_mode") {
            settings.dark_mode = dark == "true";
        }

        if settings.dark_mode {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
        } else {
            cc.egui_ctx.set_visuals(egui::Visuals::light());
        }

        let locale = Locale::new(settings.language.clone());

        // Load catalog from cache; fetch from API if missing
        let translations = db.get_cached_translations().unwrap_or_default();
        let books = db.get_cached_books(&settings.default_translation).unwrap_or_default();
        let needs_catalog = translations.is_empty() || books.is_empty();

        let settings_lang = settings.language.clone();
        let settings_dark = settings.dark_mode;
        let settings_trans = settings.default_translation.clone();

        let db = Arc::new(Mutex::new(db));

        let saved_verses = db.lock().unwrap().get_saved_verses().unwrap_or_default();
        let reading_plans = db.lock().unwrap().get_reading_plans().unwrap_or_default();
        let memory_cards = db.lock().unwrap().get_memory_cards().unwrap_or_default();
        let verse_marks: std::collections::HashMap<(String, u32, u32, String), String> = db
            .lock().unwrap()
            .get_verse_marks()
            .unwrap_or_default()
            .into_iter()
            .map(|(bn, ch, v, tr, col)| ((bn, ch, v, tr), col))
            .collect();
        let word_marks: std::collections::HashMap<(String, u32, u32, String, u32), String> = db
            .lock().unwrap()
            .get_word_marks()
            .unwrap_or_default()
            .into_iter()
            .map(|(bn, ch, v, tr, wi, col)| ((bn, ch, v, tr, wi), col))
            .collect();

        // Start background catalog fetch if cache is empty
        let (catalog_loading, catalog_receiver) = if needs_catalog {
            let (tx, rx) = mpsc::channel();
            let default_trans = settings.default_translation.clone();
            let db_clone = db.clone();
            thread::spawn(move || {
                let result: Result<(Vec<Translation>, Vec<BibleBook>), String> = (|| {
                    let trans = BibleClient::fetch_translations()?;
                    let books = BibleClient::fetch_books(&default_trans)?;
                    if let Ok(db) = db_clone.lock() {
                        let _ = db.cache_translations(&trans);
                        let _ = db.cache_books(&default_trans, &books);
                    }
                    Ok((trans, books))
                })();
                let _ = tx.send(result);
            });
            (true, Some(rx))
        } else {
            (false, None)
        };

        BibleDeskApp {
            db,
            settings,
            locale,
            current_tab: Tab::Reader,
            catalog_loading,
            catalog_receiver,
            catalog_error: None,
            selected_book_idx: 0,
            selected_chapter: 1,
            current_chapter: None,
            chapter_loading: false,
            chapter_receiver: None,
            reader_status: String::new(),
            book_downloading: false,
            book_download_receiver: None,
            search_query: String::new(),
            search_results: Vec::new(),
            search_translation_filter: "all".to_string(),
            reading_plans,
            selected_plan_id: None,
            plan_entries: Vec::new(),
            new_plan_name: String::new(),
            plan_entry_book_idx: 0,
            plan_entry_chapter: "1".to_string(),
            plan_entry_date: String::new(),
            saved_verses,
            saved_filter: "all".to_string(),
            editing_note_id: None,
            editing_note_text: String::new(),
            memory_cards,
            flashcard_view: FlashcardView::CardList,
            flashcard_mode: FlashcardMode::ReferenceToVerse,
            current_card_idx: None,
            flashcard_answer: String::new(),
            flashcard_result: None,
            settings_saved_msg: false,
            settings_saved_timer: 0.0,
            settings_lang_temp: settings_lang,
            settings_translation_temp: settings_trans,
            settings_dark_temp: settings_dark,
            translations,
            books,
            verse_marks,
            word_marks,
            active_marker_color: None,
            marker_drag_active: false,
        }
    }

    fn load_books_for_current_translation(&mut self) {
        let trans = self.settings.default_translation.clone();
        // Serve from DB cache if available
        if let Ok(books) = self.db.lock().unwrap().get_cached_books(&trans) {
            if !books.is_empty() {
                self.books = books;
                self.selected_book_idx = 0;
                self.selected_chapter = 1;
                return;
            }
        }
        // Avoid duplicate in-flight fetches
        if self.catalog_receiver.is_some() {
            return;
        }
        self.catalog_loading = true;
        let (tx, rx) = mpsc::channel();
        self.catalog_receiver = Some(rx);
        let db_clone = self.db.clone();
        thread::spawn(move || {
            let result: Result<(Vec<Translation>, Vec<BibleBook>), String> =
                BibleClient::fetch_books(&trans).map(|books| {
                    if let Ok(db) = db_clone.lock() {
                        let _ = db.cache_books(&trans, &books);
                    }
                    // Empty translations vector signals "books-only refresh"
                    (Vec::new(), books)
                });
            let _ = tx.send(result);
        });
    }

    fn poll_catalog_load(&mut self) {
        let received = self.catalog_receiver.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = received {
            self.catalog_loading = false;
            self.catalog_receiver = None;
            match result {
                Ok((translations, books)) => {
                    self.catalog_error = None;
                    if !translations.is_empty() {
                        self.translations = translations;
                        // Keep default translation valid
                        if !self.translations.iter().any(|t| t.id == self.settings.default_translation) {
                            if let Some(first) = self.translations.first() {
                                self.settings.default_translation = first.id.clone();
                                self.settings_translation_temp = first.id.clone();
                            }
                        }
                    }
                    if !books.is_empty() {
                        self.books = books;
                        self.selected_book_idx = 0;
                        self.selected_chapter = 1;
                    }
                }
                Err(e) => {
                    self.catalog_error = Some(e);
                }
            }
        }
    }

    fn load_chapter(&mut self) {
        if self.chapter_loading || self.books.is_empty() {
            return;
        }
        let book = &self.books[self.selected_book_idx];
        let book_nr = book.book_nr;
        let book_name = book.name.clone();
        let chapter = self.selected_chapter;
        let translation = self.settings.default_translation.clone();

        // Serve from cache if available
        {
            let db = self.db.lock().unwrap();
            if let Ok(verses) = db.get_cached_chapter(&book_name, chapter, &translation) {
                if !verses.is_empty() {
                    self.current_chapter = Some(Chapter {
                        reference: format!("{} {}", book_name, chapter),
                        book_name: book_name.clone(),
                        chapter_num: chapter,
                        verses,
                        translation: translation.clone(),
                    });
                    // Populate chapter count from the verses cache when not yet known
                    if self.books[self.selected_book_idx].chapters == 0 {
                        if let Ok(max_ch) = db.get_max_chapter(&book_name, &translation) {
                            if max_ch > 0 {
                                self.books[self.selected_book_idx].chapters = max_ch;
                            }
                        }
                    }
                    self.reader_status = String::new();
                    return;
                }
            }
        }

        // Fetch from API in a background thread
        self.chapter_loading = true;
        self.reader_status = "Loading from API...".to_string();
        let (tx, rx) = mpsc::channel();
        self.chapter_receiver = Some(rx);

        let db = self.db.clone();
        thread::spawn(move || {
            let result = BibleClient::fetch_chapter(&translation, book_nr, chapter);
            match &result {
                Ok((chap, other_verses, _total)) => {
                    if let Ok(db) = db.lock() {
                        // Cache the requested chapter
                        let _ = db.cache_verses(&chap.verses);
                        // Also cache all other chapters fetched from the same book response
                        if !other_verses.is_empty() {
                            let _ = db.cache_verses(other_verses);
                        }
                    }
                }
                Err(_) => {}
            }
            let _ = tx.send(result.map(|(chap, _, total)| (chap, total)));
        });
    }

    fn poll_chapter_load(&mut self) {
        let received = self.chapter_receiver.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = received {
            self.chapter_loading = false;
            self.chapter_receiver = None;
            match result {
                Ok((chapter, total_chapters)) => {
                    self.reader_status = String::new();
                    // Store the actual chapter count in the book entry so the selector can use it
                    if total_chapters > 0 && self.selected_book_idx < self.books.len() {
                        self.books[self.selected_book_idx].chapters = total_chapters;
                        // Persist back to cached_books so cold restarts use the right count
                        let translation = chapter.translation.clone();
                        let book_nr = self.books[self.selected_book_idx].book_nr;
                        if let Ok(db) = self.db.lock() {
                            let _ = db.update_book_chapter_count(&translation, book_nr, total_chapters);
                        }
                    }
                    self.current_chapter = Some(chapter);
                }
                Err(e) => {
                    self.reader_status = format!("Error: {}", e);
                }
            }
        }
    }

    fn do_search(&mut self) {
        let query = self.search_query.trim().to_string();
        if query.is_empty() {
            return;
        }
        let filter = if self.search_translation_filter == "all" {
            None
        } else {
            Some(self.search_translation_filter.clone())
        };
        let db = self.db.lock().unwrap();
        self.search_results = db.search_verses(&query, filter.as_deref()).unwrap_or_default();
    }

    fn start_book_download(&mut self) {
        if self.book_downloading || self.books.is_empty() {
            return;
        }
        let book = &self.books[self.selected_book_idx];
        let book_nr = book.book_nr;
        let translation = self.settings.default_translation.clone();
        self.book_downloading = true;
        self.reader_status = self.locale.t("reader.downloading_book").to_string();
        let (tx, rx) = mpsc::channel();
        self.book_download_receiver = Some(rx);
        let db = self.db.clone();
        thread::spawn(move || {
            let result = BibleClient::download_book(&translation, book_nr).map(|verses| {
                let count = verses.len();
                if let Ok(db) = db.lock() {
                    let _ = db.cache_verses(&verses);
                }
                count
            });
            let _ = tx.send(result);
        });
    }

    fn poll_book_download(&mut self) {
        let received = self.book_download_receiver.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = received {
            self.book_downloading = false;
            self.book_download_receiver = None;
            match result {
                Ok(count) => {
                    self.reader_status = format!("{} ({} verses)", self.locale.t("reader.book_downloaded"), count);
                }
                Err(e) => {
                    self.reader_status = format!("Download error: {}", e);
                }
            }
        }
    }

    fn reload_saved(&mut self) {
        self.saved_verses = self.db.lock().unwrap().get_saved_verses().unwrap_or_default();
    }

    fn reload_plans(&mut self) {
        self.reading_plans = self.db.lock().unwrap().get_reading_plans().unwrap_or_default();
    }

    fn reload_plan_entries(&mut self) {
        if let Some(plan_id) = self.selected_plan_id {
            self.plan_entries = self.db.lock().unwrap().get_plan_entries(plan_id).unwrap_or_default();
        }
    }

    fn reload_cards(&mut self) {
        self.memory_cards = self.db.lock().unwrap().get_memory_cards().unwrap_or_default();
    }

    fn reload_marks(&mut self) {
        self.verse_marks = self.db.lock().unwrap()
            .get_verse_marks()
            .unwrap_or_default()
            .into_iter()
            .map(|(bn, ch, v, tr, col)| ((bn, ch, v, tr), col))
            .collect();
    }

    fn reload_word_marks(&mut self) {
        self.word_marks = self.db.lock().unwrap()
            .get_word_marks()
            .unwrap_or_default()
            .into_iter()
            .map(|(bn, ch, v, tr, wi, col)| ((bn, ch, v, tr, wi), col))
            .collect();
    }

    /// Convert a stored color name to an opaque background Color32 for word highlights.
    fn mark_color(name: &str) -> Color32 {
        match name {
            "yellow" => Color32::from_rgb(255, 220,  40),
            "green"  => Color32::from_rgb( 50, 190,  70),
            "blue"   => Color32::from_rgb( 80, 150, 255),
            "pink"   => Color32::from_rgb(255, 110, 170),
            _        => Color32::TRANSPARENT,
        }
    }

    /// Foreground (text) color to use on top of a mark background for readability.
    fn mark_text_color(name: &str) -> Color32 {
        match name {
            "yellow" => Color32::from_rgb(30, 20, 0),
            "green"  => Color32::BLACK,
            "blue"   => Color32::WHITE,
            "pink"   => Color32::BLACK,
            _        => Color32::BLACK,
        }
    }

    /// Color32 for the toolbar button indicator (opaque dot).
    fn mark_color_opaque(name: &str) -> Color32 {
        match name {
            "yellow" => Color32::from_rgb(255, 210,  30),
            "green"  => Color32::from_rgb( 60, 185,  70),
            "blue"   => Color32::from_rgb( 60, 130, 240),
            "pink"   => Color32::from_rgb(240,  90, 150),
            _        => Color32::TRANSPARENT,
        }
    }
}

fn app_db_path() -> std::path::PathBuf {
    let mut dir = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    dir.push("bibledesk");
    std::fs::create_dir_all(&dir).ok();
    dir.push("bibledesk.db");
    dir
}

impl eframe::App for BibleDeskApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_chapter_load();
        self.poll_catalog_load();
        self.poll_book_download();
        if self.chapter_loading || self.catalog_loading || self.book_downloading {
            ctx.request_repaint();
        }

        // Dismiss "settings saved" message after 2 s
        if self.settings_saved_msg {
            self.settings_saved_timer += ctx.input(|i| i.unstable_dt) as f64;
            if self.settings_saved_timer > 2.0 {
                self.settings_saved_msg = false;
                self.settings_saved_timer = 0.0;
            }
        }

        // Tab bar
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("📖 BibleDesk").strong());
                ui.separator();
                for (tab, icon, key) in [
                    (Tab::Reader,       "📖", "tab.reader"),
                    (Tab::Search,       "🔍", "tab.search"),
                    (Tab::ReadingPlans, "📅", "tab.plans"),
                    (Tab::SavedVerses,  "🔖", "tab.saved"),
                    (Tab::Flashcards,   "🃏", "tab.flashcards"),
                    (Tab::Settings,     "⚙",  "tab.settings"),
                ] {
                    let label = format!("{} {}", icon, self.locale.t(key));
                    if ui.selectable_label(self.current_tab == tab, label).clicked() {
                        self.current_tab = tab;
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.current_tab.clone() {
                Tab::Reader       => self.show_reader(ui, ctx),
                Tab::Search       => self.show_search(ui),
                Tab::ReadingPlans => self.show_reading_plans(ui),
                Tab::SavedVerses  => self.show_saved_verses(ui),
                Tab::Flashcards   => self.show_flashcards(ui),
                Tab::Settings     => self.show_settings(ui, ctx),
            }
        });
    }
}

// ── Tab implementations ──────────────────────────────────────────────────────

impl BibleDeskApp {
    fn show_reader(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        // While catalog is loading and no books are available yet, show a spinner
        if self.books.is_empty() {
            ui.centered_and_justified(|ui| {
                if self.catalog_loading {
                    ui.spinner();
                    ui.label("Loading Bible catalog from API...");
                } else if let Some(error) = &self.catalog_error {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Failed to load Bible catalog")
                                .color(Color32::from_rgb(255, 100, 100))
                                .size(16.0)
                        );
                        ui.add_space(8.0);
                        ui.label(format!("Error: {}", error));
                        ui.add_space(8.0);
                        ui.label("Please check your internet connection and restart the application.");
                    });
                } else {
                    ui.label("Book list unavailable. Check your internet connection and restart.");
                }
            });
            return;
        }

        // Controls row
        let selected_book_name = self.books[self.selected_book_idx].name.clone();
        let max_chapters = self.books[self.selected_book_idx].chapters; // 0 = not yet known

        // Track mouse button state for drag-to-mark; reset drag if button released
        let mouse_down = ctx.input(|i| i.pointer.primary_down());
        if !mouse_down {
            self.marker_drag_active = false;
        }

        // Track if the book selection changed (need to auto-load after the closure)
        let mut book_changed = false;

        // Detect chapter ComboBox change (need to auto-load after the closure)
        let prev_chapter = self.selected_chapter;

        // Detect translation change so we can reload books
        let mut new_translation: Option<String> = None;

        ui.horizontal(|ui| {
            egui::ComboBox::from_label(self.locale.t("reader.select_book"))
                .selected_text(&selected_book_name)
                .show_ui(ui, |ui| {
                    for i in 0..self.books.len() {
                        let name = self.books[i].name.clone();
                        if ui.selectable_label(self.selected_book_idx == i, &name).clicked()
                            && self.selected_book_idx != i
                        {
                            self.selected_book_idx = i;
                            self.selected_chapter = 1;
                            book_changed = true;
                        }
                    }
                });

            ui.label(self.locale.t("reader.chapter"));
            // Always show a ComboBox. When max_chapters is not yet known (0), show a
            // single-item placeholder dropdown with the current chapter number.
            let display_max = if max_chapters > 0 { max_chapters } else { self.selected_chapter };
            egui::ComboBox::from_id_salt("chapter_sel")
                .selected_text(self.selected_chapter.to_string())
                .show_ui(ui, |ui| {
                    for c in 1..=display_max {
                        ui.selectable_value(&mut self.selected_chapter, c, c.to_string());
                    }
                });

            ui.label(self.locale.t("reader.translation"));
            let current_name = self.translations.iter()
                .find(|t| t.id == self.settings.default_translation)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| self.settings.default_translation.clone());
            let mut trans_sel = self.settings.default_translation.clone();
            egui::ComboBox::from_id_salt("trans_sel")
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    show_translation_combo_ui(ui, &self.translations, &mut trans_sel, None);
                });
            if trans_sel != self.settings.default_translation {
                new_translation = Some(trans_sel);
            }

            // Download Book button
            let dl_label = if self.book_downloading {
                self.locale.t("reader.downloading_book")
            } else {
                self.locale.t("reader.download_book")
            };
            if ui.add_enabled(
                !self.book_downloading && !self.chapter_loading,
                egui::Button::new(dl_label),
            ).clicked() {
                self.start_book_download();
            }
        });

        // Apply translation change after the closure
        if let Some(id) = new_translation {
            self.settings.default_translation = id;
            self.current_chapter = None;
            self.load_books_for_current_translation();
        }

        // Auto-load when chapter ComboBox selection changed
        if self.selected_chapter != prev_chapter && !self.chapter_loading {
            self.load_chapter();
        }

        // Auto-load chapter 1 when the book selection changed
        if book_changed && !self.chapter_loading {
            self.load_chapter();
        }

        // Prev / Next navigation + right-side marker toolbar
        ui.horizontal(|ui| {
            let can_prev = self.selected_chapter > 1;
            let can_next = max_chapters == 0 || self.selected_chapter < max_chapters;
            if ui.add_enabled(can_prev, egui::Button::new(self.locale.t("reader.previous"))).clicked() {
                self.selected_chapter -= 1;
                self.load_chapter();
            }
            if ui.add_enabled(can_next, egui::Button::new(self.locale.t("reader.next"))).clicked() {
                self.selected_chapter += 1;
                self.load_chapter();
            }
            if !self.reader_status.is_empty() {
                ui.label(RichText::new(&self.reader_status).color(Color32::YELLOW));
            }

            // ── Marker toolbar (right-aligned) ──────────────────────────────
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // "Off" button
                let off_active = self.active_marker_color.is_none();
                let off_btn = egui::Button::new(
                    RichText::new(self.locale.t("reader.marker_off"))
                        .color(if off_active { Color32::BLACK } else { ui.visuals().text_color() }),
                );
                let off_btn = if off_active {
                    off_btn.fill(Color32::from_rgb(200, 200, 200))
                } else {
                    off_btn
                };
                if ui.add(off_btn).clicked() {
                    self.active_marker_color = None;
                }

                for color_name in ["pink", "blue", "green", "yellow"] {
                    let dot_color = BibleDeskApp::mark_color_opaque(color_name);
                    let active = self.active_marker_color == Some(color_name);
                    let btn = egui::Button::new(
                        RichText::new("●").color(dot_color).size(18.0),
                    );
                    let btn = if active {
                        btn.stroke(egui::Stroke::new(2.5, ui.visuals().text_color()))
                    } else {
                        btn
                    };
                    if ui.add(btn).clicked() {
                        self.active_marker_color = if active { None } else { Some(color_name) };
                    }
                }

                ui.label(self.locale.t("reader.marker_mode"));
            });
        });

        ui.separator();

        if let Some(chapter) = self.current_chapter.clone() {
            let trans_name = self.translations.iter()
                .find(|t| t.id == chapter.translation)
                .map(|t| t.name.as_str())
                .unwrap_or(chapter.translation.as_str());
            ui.heading(format!("{} ({})", chapter.reference, trans_name));
            ui.separator();

            // Collect pending actions from context menus / word clicks
            let mut save_verse: Option<Verse> = None;
            let mut unsave_verse_id: Option<i64> = None;
            let mut add_flashcard: Option<Verse> = None;
            let mut set_verse_mark: Option<(Verse, &'static str)> = None;
            let mut clear_verse_mark_v: Option<Verse> = None;
            // Vec so drag can mark multiple words in one frame
            let mut set_words: Vec<(String, u32, u32, String, u32, &'static str)> = Vec::new();
            let mut clear_words: Vec<(String, u32, u32, String, u32)> = Vec::new();
            let mut start_drag = false;

            // Fast lookups
            let saved_map: std::collections::HashMap<(String, u32, u32, String), i64> = self
                .saved_verses
                .iter()
                .map(|sv| ((sv.book_name.clone(), sv.chapter, sv.verse, sv.translation.clone()), sv.id))
                .collect();

            // Localized strings captured before closure (avoids borrow conflict)
            let lbl_save      = self.locale.t("reader.verse_context_save");
            let lbl_unsave    = self.locale.t("reader.verse_context_unsave");
            let lbl_flash     = self.locale.t("reader.verse_context_flashcard");
            let lbl_mark      = self.locale.t("reader.mark");
            let lbl_yellow    = self.locale.t("reader.mark_yellow");
            let lbl_green     = self.locale.t("reader.mark_green");
            let lbl_blue      = self.locale.t("reader.mark_blue");
            let lbl_pink      = self.locale.t("reader.mark_pink");
            let lbl_clr       = self.locale.t("reader.clear_mark");
            let verse_marks   = &self.verse_marks;
            let word_marks    = &self.word_marks;
            let active_color  = self.active_marker_color;
            let drag_active   = self.marker_drag_active;

            ScrollArea::vertical().id_salt("reader_scroll").show(ui, |ui| {
                ui.set_max_width(ui.available_width());

                for verse in &chapter.verses {
                    let vkey = (
                        verse.book_name.clone(),
                        verse.chapter,
                        verse.verse,
                        verse.translation.clone(),
                    );
                    let saved_id  = saved_map.get(&vkey).copied();
                    let verse_mark_col = verse_marks.get(&vkey).map(|c| BibleDeskApp::mark_color(c));
                    let is_verse_marked = verse_mark_col.is_some();

                    // ── Verse number row ──────────────────────────────────────
                    ui.horizontal(|ui| {
                        if let Some(col) = verse_mark_col {
                            ui.label(RichText::new("●").color(col));
                        }
                        ui.label(
                            RichText::new(format!("{}.", verse.verse))
                                .strong()
                                .color(Color32::from_rgb(150, 180, 255)),
                        );
                        if saved_id.is_some() {
                            ui.label(
                                RichText::new("✓")
                                    .color(Color32::from_rgb(100, 200, 120))
                                    .small(),
                            );
                        }
                    });

                    // ── Word-by-word rendering with highlight support ─────────
                    let words: Vec<&str> = verse.text.split_whitespace().collect();

                    // Context menu anchored to a zero-size invisible widget between the verse
                    // number row and the word spans. This lets the user right-click anywhere
                    // in the verse area to get the save/flashcard/mark-verse menu, while the
                    // individual word Labels below handle left-click / drag for word-level marking.
                    let anchor_resp = ui.add(egui::Label::new("").sense(egui::Sense::click()));
                    anchor_resp.context_menu(|ui| {
                        if saved_id.is_some() {
                            if ui.button(lbl_unsave).clicked() {
                                unsave_verse_id = saved_id;
                                ui.close_menu();
                            }
                        } else if ui.button(lbl_save).clicked() {
                            save_verse = Some(verse.clone());
                            ui.close_menu();
                        }
                        if ui.button(lbl_flash).clicked() {
                            add_flashcard = Some(verse.clone());
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.menu_button(lbl_mark, |ui| {
                            if ui.button(lbl_yellow).clicked() { set_verse_mark = Some((verse.clone(), "yellow")); ui.close_menu(); }
                            if ui.button(lbl_green).clicked()  { set_verse_mark = Some((verse.clone(), "green"));  ui.close_menu(); }
                            if ui.button(lbl_blue).clicked()   { set_verse_mark = Some((verse.clone(), "blue"));   ui.close_menu(); }
                            if ui.button(lbl_pink).clicked()   { set_verse_mark = Some((verse.clone(), "pink"));   ui.close_menu(); }
                        });
                        if is_verse_marked && ui.button(lbl_clr).clicked() {
                            clear_verse_mark_v = Some(verse.clone());
                            ui.close_menu();
                        }
                    });

                    // Word spans in a wrapping horizontal flow
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (word_idx, word) in words.iter().enumerate() {
                            let widx = word_idx as u32;
                            let wkey = (
                                verse.book_name.clone(),
                                verse.chapter,
                                verse.verse,
                                verse.translation.clone(),
                                widx,
                            );
                            let word_color = word_marks.get(&wkey);

                            // Build RichText: highlighted words use opaque background + contrasting foreground
                            let rt = if let Some(color_name) = word_color {
                                RichText::new(*word)
                                    .background_color(BibleDeskApp::mark_color(color_name))
                                    .color(BibleDeskApp::mark_text_color(color_name))
                            } else {
                                RichText::new(*word)
                            };

                            let resp = ui.add(
                                egui::Label::new(rt).sense(egui::Sense::click_and_drag()),
                            );

                            // Start of a drag gesture → enable drag-mark for subsequent words
                            if resp.drag_started() && active_color.is_some() {
                                start_drag = true;
                            }

                            // Apply mark on click OR when hovered while dragging with mouse held
                            let should_apply = resp.clicked()
                                || (resp.hovered() && mouse_down && (drag_active || start_drag) && active_color.is_some());

                            if should_apply {
                                if let Some(color) = active_color {
                                    if word_color.map(|c| c.as_str()) == Some(color) {
                                        // Toggle off
                                        clear_words.push((
                                            verse.book_name.clone(),
                                            verse.chapter,
                                            verse.verse,
                                            verse.translation.clone(),
                                            widx,
                                        ));
                                    } else {
                                        set_words.push((
                                            verse.book_name.clone(),
                                            verse.chapter,
                                            verse.verse,
                                            verse.translation.clone(),
                                            widx,
                                            color,
                                        ));
                                    }
                                }
                            }
                        }
                    });

                    ui.add_space(6.0);
                }
            });

            // Persist drag state
            if start_drag {
                self.marker_drag_active = true;
            }

            // Apply deferred actions
            if let Some(v) = save_verse {
                let _ = self.db.lock().unwrap().save_verse(&v, "");
                self.reload_saved();
            }
            if let Some(id) = unsave_verse_id {
                let _ = self.db.lock().unwrap().delete_saved_verse(id);
                self.reload_saved();
            }
            if let Some(v) = add_flashcard {
                let _ = self.db.lock().unwrap().add_memory_card(&v);
                self.reload_cards();
            }
            if let Some((v, color)) = set_verse_mark {
                let _ = self.db.lock().unwrap().set_verse_mark(
                    &v.book_name, v.chapter, v.verse, &v.translation, color,
                );
                self.reload_marks();
            }
            if let Some(v) = clear_verse_mark_v {
                let _ = self.db.lock().unwrap().clear_verse_mark(
                    &v.book_name, v.chapter, v.verse, &v.translation,
                );
                self.reload_marks();
            }
            if !set_words.is_empty() || !clear_words.is_empty() {
                let db = self.db.lock().unwrap();
                for (bn, ch, v, tr, wi, color) in &set_words {
                    let _ = db.set_word_mark(bn, *ch, *v, tr, *wi, color);
                }
                for (bn, ch, v, tr, wi) in &clear_words {
                    let _ = db.clear_word_mark(bn, *ch, *v, tr, *wi);
                }
                drop(db);
                self.reload_word_marks();
            }
        } else if self.chapter_loading {
            ui.centered_and_justified(|ui| { ui.spinner(); });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label(self.locale.t("reader.no_chapter"));
            });
        }
    }

    fn show_search(&mut self, ui: &mut Ui) {
        ui.heading(self.locale.t("tab.search"));
        ui.separator();

        ui.horizontal(|ui| {
            let resp = ui.add(
                TextEdit::singleline(&mut self.search_query)
                    .hint_text(self.locale.t("search.placeholder"))
                    .desired_width(400.0),
            );
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter || ui.button(self.locale.t("search.button")).clicked() {
                self.do_search();
            }
        });

        ui.horizontal(|ui| {
            ui.label(self.locale.t("search.filter_translation"));
            let current = if self.search_translation_filter == "all" {
                self.locale.t("search.all_translations").to_string()
            } else {
                self.search_translation_filter.clone()
            };
            egui::ComboBox::from_id_salt("search_trans_filter")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    let all_label = self.locale.t("search.all_translations").to_string();
                    show_translation_combo_ui(
                        ui,
                        &self.translations,
                        &mut self.search_translation_filter,
                        Some(("all", &all_label)),
                    );
                });
        });

        ui.separator();

        if !self.search_results.is_empty() {
            ui.label(format!("{}: {}", self.locale.t("search.results"), self.search_results.len()));
            ScrollArea::vertical().id_salt("search_scroll").show(ui, |ui| {
                let results = self.search_results.clone();
                for verse in &results {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{} {}:{}", verse.book_name, verse.chapter, verse.verse))
                                    .strong(),
                            );
                            ui.label(
                                RichText::new(format!("[{}]", verse.translation))
                                    .small()
                                    .color(Color32::GRAY),
                            );
                        });
                        ui.label(&verse.text);
                        ui.horizontal(|ui| {
                            if ui.small_button(self.locale.t("reader.save_verse")).clicked() {
                                let v = verse.clone();
                                let _ = self.db.lock().unwrap().save_verse(&v, "");
                                self.reload_saved();
                            }
                            if ui.small_button(self.locale.t("reader.add_flashcard")).clicked() {
                                let v = verse.clone();
                                let _ = self.db.lock().unwrap().add_memory_card(&v);
                                self.reload_cards();
                            }
                        });
                    });
                    ui.add_space(4.0);
                }
            });
        } else if !self.search_query.is_empty() {
            ui.label(self.locale.t("search.no_results"));
        }
    }

    fn show_reading_plans(&mut self, ui: &mut Ui) {
        ui.heading(self.locale.t("plans.title"));
        ui.separator();

        // New plan creation
        ui.group(|ui| {
            ui.label(self.locale.t("plans.new"));
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.new_plan_name);
                if ui.button(self.locale.t("plans.create")).clicked() && !self.new_plan_name.trim().is_empty() {
                    let name = self.new_plan_name.trim().to_string();
                    let id = self.db.lock().unwrap().create_reading_plan(&name).unwrap_or(0);
                    self.new_plan_name.clear();
                    self.reload_plans();
                    self.selected_plan_id = Some(id);
                    self.reload_plan_entries();
                }
            });
        });

        ui.separator();

        if self.reading_plans.is_empty() {
            ui.label(self.locale.t("plans.no_plans"));
            return;
        }

        // Collect state needed inside the columns closure up-front
        let plans        = self.reading_plans.clone();
        let entries      = self.plan_entries.clone();
        let selected_pid = self.selected_plan_id;

        // Split-pane: plan list left, entries right
        // We use a flag struct to communicate mutations back from the closure.
        struct PlanAction {
            select: Option<i64>,
            delete: Option<i64>,
            reload_entries: bool,
            reload_plans: bool,
            toggle_entry: Option<(i64, bool)>,
            delete_entry: Option<i64>,
            add_entry: Option<(i64, String, u32, Option<String>)>,
        }
        let mut action = PlanAction {
            select: None,
            delete: None,
            reload_entries: false,
            reload_plans: false,
            toggle_entry: None,
            delete_entry: None,
            add_entry: None,
        };

        // Clones needed for the closure (avoids capturing &mut self)
        let books                 = self.books.clone();
        let mut plan_entry_book   = self.plan_entry_book_idx;
        let mut plan_entry_chap   = self.plan_entry_chapter.clone();
        let mut plan_entry_date   = self.plan_entry_date.clone();
        let locale_add            = self.locale.t("plans.add");
        let locale_book           = self.locale.t("plans.book");
        let locale_chap           = self.locale.t("plans.chapter");
        let locale_date           = self.locale.t("plans.date");
        let locale_add_chap       = self.locale.t("plans.add_chapter");

        ui.columns(2, |cols| {
            // ── Left column: plan list ──────────────────────────────────────
            cols[0].heading("Plans");
            ScrollArea::vertical().id_salt("plans_list").show(&mut cols[0], |ui| {
                for plan in &plans {
                    let sel = selected_pid == Some(plan.id);
                    ui.horizontal(|ui| {
                        if ui.selectable_label(sel, &plan.name).clicked() {
                            action.select = Some(plan.id);
                            action.reload_entries = true;
                        }
                        if ui.small_button("🗑").clicked() {
                            action.delete = Some(plan.id);
                            action.reload_plans = true;
                        }
                    });
                    // Progress bar for the selected plan
                    if sel && !entries.is_empty() {
                        let done  = entries.iter().filter(|e| e.completed).count();
                        let total = entries.len();
                        ui.add(egui::ProgressBar::new(done as f32 / total as f32)
                            .text(format!("{}/{}", done, total)));
                    }
                }
            });

            // ── Right column: entries for selected plan ──────────────────────
            if let Some(plan_id) = selected_pid {
                // Add-entry form
                cols[1].group(|ui| {
                    if books.is_empty() {
                        ui.label("Book list loading...");
                    } else {
                        ui.label(locale_add_chap);
                        ui.horizontal(|ui| {
                            ui.label(locale_book);
                            let book_label = books[plan_entry_book.min(books.len().saturating_sub(1))].name.clone();
                            egui::ComboBox::from_id_salt("plan_book_sel")
                                .selected_text(book_label)
                                .show_ui(ui, |ui| {
                                    for (i, book) in books.iter().enumerate() {
                                        ui.selectable_value(&mut plan_entry_book, i, book.name.clone());
                                    }
                                });
                            ui.label(locale_chap);
                            ui.text_edit_singleline(&mut plan_entry_chap);
                        });
                        ui.horizontal(|ui| {
                            ui.label(locale_date);
                            ui.text_edit_singleline(&mut plan_entry_date);
                        });
                        if ui.button(locale_add).clicked() && !books.is_empty() {
                            let chapter: u32 = plan_entry_chap.parse().unwrap_or(1);
                            let book_name    = books[plan_entry_book.min(books.len() - 1)].name.clone();
                            let date         = if plan_entry_date.is_empty() {
                                None
                            } else {
                                Some(plan_entry_date.clone())
                            };
                            action.add_entry = Some((plan_id, book_name, chapter, date));
                            action.reload_entries = true;
                        }
                    }
                });

                cols[1].separator();

                // Entry list
                ScrollArea::vertical().id_salt("plan_entries").show(&mut cols[1], |ui| {
                    for entry in &entries {
                        ui.horizontal(|ui| {
                            let mut done = entry.completed;
                            if ui.checkbox(&mut done, "").changed() {
                                action.toggle_entry = Some((entry.id, done));
                                action.reload_entries = true;
                            }
                            let label = match &entry.scheduled_date {
                                Some(d) => format!("{} {} ({})", entry.book_name, entry.chapter, d),
                                None    => format!("{} {}", entry.book_name, entry.chapter),
                            };
                            if entry.completed {
                                ui.label(RichText::new(label).strikethrough().color(Color32::GRAY));
                            } else {
                                ui.label(label);
                            }
                            if ui.small_button("🗑").clicked() {
                                action.delete_entry = Some(entry.id);
                                action.reload_entries = true;
                            }
                        });
                    }
                });
            }
        });

        // ── Apply mutations after the closure ──────────────────────────────
        // Write back edited form fields
        self.plan_entry_book_idx = plan_entry_book;
        self.plan_entry_chapter  = plan_entry_chap;
        self.plan_entry_date     = plan_entry_date;

        if let Some(id) = action.select {
            self.selected_plan_id = Some(id);
        }
        if let Some(id) = action.delete {
            let _ = self.db.lock().unwrap().delete_reading_plan(id);
            if self.selected_plan_id == Some(id) {
                self.selected_plan_id = None;
                self.plan_entries.clear();
            }
        }
        if let Some((id, done)) = action.toggle_entry {
            let _ = self.db.lock().unwrap().toggle_entry_complete(id, done);
        }
        if let Some(id) = action.delete_entry {
            let _ = self.db.lock().unwrap().delete_plan_entry(id);
        }
        if let Some((plan_id, book_name, chapter, date)) = action.add_entry {
            let _ = self.db.lock().unwrap().add_plan_entry(plan_id, &book_name, chapter, date.as_deref());
        }
        if action.reload_plans {
            self.reload_plans();
        }
        if action.reload_entries {
            self.reload_plan_entries();
        }
    }

    fn show_saved_verses(&mut self, ui: &mut Ui) {
        ui.heading(self.locale.t("saved.title"));
        ui.separator();

        ui.horizontal(|ui| {
            ui.label(self.locale.t("saved.filter"));
            let current = if self.saved_filter == "all" {
                self.locale.t("search.all_translations").to_string()
            } else {
                self.saved_filter.clone()
            };
            egui::ComboBox::from_id_salt("saved_trans_filter")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    let all_label = self.locale.t("search.all_translations").to_string();
                    show_translation_combo_ui(
                        ui,
                        &self.translations,
                        &mut self.saved_filter,
                        Some(("all", &all_label)),
                    );
                });
        });

        ui.separator();

        let filtered: Vec<SavedVerse> = self.saved_verses.iter()
            .filter(|v| self.saved_filter == "all" || v.translation == self.saved_filter)
            .cloned()
            .collect();

        if filtered.is_empty() {
            ui.label(self.locale.t("saved.no_saved"));
            return;
        }

        let mut to_delete: Option<i64>            = None;
        let mut to_add_card: Option<SavedVerse>   = None;
        let mut save_note: Option<(i64, String)>  = None;
        let mut edit_note: Option<(i64, String)>  = None;

        ScrollArea::vertical().id_salt("saved_scroll").show(ui, |ui| {
            for sv in &filtered {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        // Mark color dot (if this verse is marked)
                        let mark_key = (sv.book_name.clone(), sv.chapter, sv.verse, sv.translation.clone());
                        if let Some(color_name) = self.verse_marks.get(&mark_key) {
                            ui.label(
                                RichText::new("●")
                                    .color(BibleDeskApp::mark_color(color_name))
                            );
                        }
                        ui.label(
                            RichText::new(format!("{} {}:{}", sv.book_name, sv.chapter, sv.verse))
                                .strong(),
                        );
                        ui.label(
                            RichText::new(format!("[{}]", sv.translation))
                                .small()
                                .color(Color32::GRAY),
                        );
                        if ui.small_button(self.locale.t("saved.delete")).clicked() {
                            to_delete = Some(sv.id);
                        }
                        if ui.small_button(self.locale.t("reader.add_flashcard")).clicked() {
                            to_add_card = Some(sv.clone());
                        }
                    });
                    ui.add(egui::Label::new(&sv.text).wrap());

                    if self.editing_note_id == Some(sv.id) {
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut self.editing_note_text);
                            if ui.button(self.locale.t("saved.save_note")).clicked() {
                                save_note = Some((sv.id, self.editing_note_text.clone()));
                            }
                        });
                    } else {
                        ui.horizontal(|ui| {
                            if !sv.note.is_empty() {
                                ui.label(
                                    RichText::new(format!("📝 {}", sv.note))
                                        .color(Color32::LIGHT_YELLOW),
                                );
                            }
                            if ui.small_button("✏ Note").clicked() {
                                edit_note = Some((sv.id, sv.note.clone()));
                            }
                        });
                    }
                });
                ui.add_space(4.0);
            }
        });

        // Apply mutations
        if let Some(id) = to_delete {
            let _ = self.db.lock().unwrap().delete_saved_verse(id);
            self.reload_saved();
        }
        if let Some(sv) = to_add_card {
            let verse = Verse {
                book_id:    sv.book_id,
                book_name:  sv.book_name,
                chapter:    sv.chapter,
                verse:      sv.verse,
                text:       sv.text,
                translation: sv.translation,
            };
            let _ = self.db.lock().unwrap().add_memory_card(&verse);
            self.reload_cards();
        }
        if let Some((id, note)) = save_note {
            let _ = self.db.lock().unwrap().update_saved_verse_note(id, &note);
            self.editing_note_id = None;
            self.reload_saved();
        }
        if let Some((id, note)) = edit_note {
            self.editing_note_id   = Some(id);
            self.editing_note_text = note;
        }
    }

    fn show_flashcards(&mut self, ui: &mut Ui) {
        ui.heading(self.locale.t("flashcards.title"));
        ui.separator();

        match self.flashcard_view.clone() {
            FlashcardView::CardList => self.show_flashcard_list(ui),
            FlashcardView::Study    => self.show_flashcard_study(ui),
        }
    }

    fn show_flashcard_list(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Mode:");
            ui.selectable_value(
                &mut self.flashcard_mode,
                FlashcardMode::ReferenceToVerse,
                self.locale.t("flashcards.mode_ref"),
            );
            ui.selectable_value(
                &mut self.flashcard_mode,
                FlashcardMode::VerseToReference,
                self.locale.t("flashcards.mode_verse"),
            );
            if !self.memory_cards.is_empty()
                && ui.button(self.locale.t("flashcards.review")).clicked()
            {
                self.flashcard_view  = FlashcardView::Study;
                self.current_card_idx = Some(0);
                self.flashcard_answer.clear();
                self.flashcard_result = None;
                self.reload_cards();
            }
        });
        ui.separator();

        if self.memory_cards.is_empty() {
            ui.label(self.locale.t("flashcards.no_cards"));
            return;
        }

        let mut to_delete: Option<i64> = None;
        ScrollArea::vertical().id_salt("cards_list").show(ui, |ui| {
            let cards = self.memory_cards.clone();
            for card in &cards {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{} {}:{}", card.book_name, card.chapter, card.verse))
                                .strong(),
                        );
                        ui.label(
                            RichText::new(format!("[{}]", card.translation))
                                .small()
                                .color(Color32::GRAY),
                        );
                        ui.label(format!("✓{} ✗{}", card.successes, card.failures));
                        if let Some(next) = &card.next_review {
                            ui.label(
                                RichText::new(format!("Next: {}", &next[..next.len().min(10)]))
                                    .small()
                                    .color(Color32::LIGHT_BLUE),
                            );
                        }
                        if ui.small_button(self.locale.t("flashcards.delete")).clicked() {
                            to_delete = Some(card.id);
                        }
                    });
                    ui.label(RichText::new(&card.text).small());
                });
                ui.add_space(2.0);
            }
        });

        if let Some(id) = to_delete {
            let _ = self.db.lock().unwrap().delete_memory_card(id);
            self.reload_cards();
        }
    }

    fn show_flashcard_study(&mut self, ui: &mut Ui) {
        if ui.button(self.locale.t("flashcards.back_to_list")).clicked() {
            self.flashcard_view = FlashcardView::CardList;
            return;
        }
        ui.separator();

        let cards = self.memory_cards.clone();
        if cards.is_empty() {
            ui.label(self.locale.t("flashcards.no_cards"));
            return;
        }

        let idx = self.current_card_idx.unwrap_or(0).min(cards.len() - 1);
        self.current_card_idx = Some(idx);
        let card = cards[idx].clone();

        ui.label(format!("Card {} / {}", idx + 1, cards.len()));
        ui.separator();

        ui.group(|ui| {
            match self.flashcard_mode {
                FlashcardMode::ReferenceToVerse => {
                    ui.label(RichText::new(self.locale.t("flashcards.show_reference")).strong());
                    ui.heading(format!(
                        "{} {}:{} [{}]",
                        card.book_name, card.chapter, card.verse, card.translation
                    ));
                    ui.separator();
                    ui.label(self.locale.t("flashcards.your_answer"));
                    ui.text_edit_multiline(&mut self.flashcard_answer);
                }
                FlashcardMode::VerseToReference => {
                    ui.label(RichText::new(self.locale.t("flashcards.show_verse")).strong());
                    ui.label(RichText::new(&card.text).size(16.0));
                    ui.separator();
                    ui.label(self.locale.t("flashcards.your_answer"));
                    ui.text_edit_singleline(&mut self.flashcard_answer);
                }
            }
        });

        ui.horizontal(|ui| {
            if ui.button(self.locale.t("flashcards.check")).clicked() {
                let answer = self.flashcard_answer.trim().to_lowercase();
                let correct = match self.flashcard_mode {
                    FlashcardMode::ReferenceToVerse => {
                        let expected = card.text.trim().to_lowercase();
                        let exp_prefix = &expected[..expected.len().min(20)];
                        let ans_prefix = &answer[..answer.len().min(20)];
                        answer.contains(exp_prefix) || expected.contains(ans_prefix)
                    }
                    FlashcardMode::VerseToReference => {
                        let expected = format!(
                            "{} {}:{}",
                            card.book_name.to_lowercase(),
                            card.chapter,
                            card.verse
                        );
                        answer == expected || answer.contains(&card.book_name.to_lowercase())
                    }
                };
                self.flashcard_result = Some(correct);
                let _ = self.db.lock().unwrap().update_memory_card_result(card.id, correct);
                self.reload_cards();
            }

            if let Some(correct) = self.flashcard_result {
                if correct {
                    ui.label(
                        RichText::new(self.locale.t("flashcards.correct"))
                            .color(Color32::GREEN)
                            .size(18.0),
                    );
                } else {
                    ui.label(
                        RichText::new(self.locale.t("flashcards.incorrect"))
                            .color(Color32::RED)
                            .size(18.0),
                    );
                    ui.label(
                        RichText::new(format!("Answer: {}", &card.text))
                            .color(Color32::LIGHT_YELLOW),
                    );
                }
                if ui.button(self.locale.t("flashcards.next_card")).clicked() {
                    let next_idx = (idx + 1) % cards.len();
                    self.current_card_idx = Some(next_idx);
                    self.flashcard_answer.clear();
                    self.flashcard_result = None;
                }
            }
        });
    }

    fn show_settings(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.heading(self.locale.t("settings.title"));
        ui.separator();

        egui::Grid::new("settings_grid").num_columns(2).spacing([20.0, 10.0]).show(ui, |ui| {
            ui.label(self.locale.t("settings.language"));
            egui::ComboBox::from_id_salt("lang_sel")
                .selected_text(self.settings_lang_temp.name())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.settings_lang_temp, Language::English, "English");
                    ui.selectable_value(&mut self.settings_lang_temp, Language::German, "Deutsch");
                });
            ui.end_row();

            ui.label(self.locale.t("settings.default_translation"));
            let trans_name = self.translations.iter()
                .find(|t| t.id == self.settings_translation_temp)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| self.settings_translation_temp.clone());
            egui::ComboBox::from_id_salt("default_trans_sel")
                .selected_text(trans_name)
                .show_ui(ui, |ui| {
                    show_translation_combo_ui(
                        ui,
                        &self.translations,
                        &mut self.settings_translation_temp,
                        None,
                    );
                });
            ui.end_row();

            ui.label(self.locale.t("settings.theme"));
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.settings_dark_temp, false,
                    self.locale.t("settings.theme_light"),
                );
                ui.selectable_value(
                    &mut self.settings_dark_temp, true,
                    self.locale.t("settings.theme_dark"),
                );
            });
            ui.end_row();
        });

        ui.separator();
        ui.label(self.locale.t("settings.cache_info"));
        ui.separator();

        if ui.button(self.locale.t("settings.save")).clicked() {
            self.settings.language            = self.settings_lang_temp.clone();
            self.settings.default_translation = self.settings_translation_temp.clone();
            self.settings.dark_mode           = self.settings_dark_temp;
            self.locale = Locale::new(self.settings.language.clone());

            if self.settings.dark_mode {
                ctx.set_visuals(egui::Visuals::dark());
            } else {
                ctx.set_visuals(egui::Visuals::light());
            }

            let db = self.db.lock().unwrap();
            let _ = db.set_setting("language",             self.settings.language.code());
            let _ = db.set_setting("default_translation",  &self.settings.default_translation);
            let _ = db.set_setting("dark_mode", if self.settings.dark_mode { "true" } else { "false" });
            drop(db);

            self.settings_saved_msg   = true;
            self.settings_saved_timer = 0.0;
        }

        if self.settings_saved_msg {
            ui.label(RichText::new(self.locale.t("settings.saved")).color(Color32::GREEN));
        }
    }
}

/// Render a translation picker with collapsible language groups.
///
/// * `translations` — list sorted by `(language, name)` (as returned by the API / DB)
/// * `selected_id`  — mutable borrow of the currently-selected abbreviation string
/// * `none_entry`   — optional `(id_value, display_label)` to show as the first "all" option
fn show_translation_combo_ui(
    ui: &mut Ui,
    translations: &[Translation],
    selected_id: &mut String,
    none_entry: Option<(&str, &str)>,
) {
    if let Some((id_val, label)) = none_entry {
        ui.selectable_value(selected_id, id_val.to_string(), label);
        ui.separator();
    }

    let mut i = 0;
    while i < translations.len() {
        // Collect all translations in the same language group
        let lang = translations[i].language.clone();
        let group_start = i;
        while i < translations.len() && translations[i].language == lang {
            i += 1;
        }
        let group = &translations[group_start..i];

        egui::CollapsingHeader::new(egui::RichText::new(&lang).strong())
            .id_salt(&lang)
            .show(ui, |ui| {
                for t in group {
                    ui.selectable_value(selected_id, t.id.clone(), &t.name);
                }
            });
    }
}
