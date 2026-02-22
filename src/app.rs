use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui::{self, RichText, Color32, ScrollArea, TextEdit, Ui};

use crate::api::BibleApiClient;
use crate::db::Database;
use crate::localization::{Language, Locale};
use crate::models::{
    bible_books, BibleBook, Chapter, MemoryCard, ReadingPlan, ReadingPlanEntry,
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

    // Reader state
    selected_book_idx: usize,
    selected_chapter: u32,
    current_chapter: Option<Chapter>,
    chapter_loading: bool,
    chapter_receiver: Option<Receiver<Result<Chapter, String>>>,
    reader_status: String,

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
}

impl BibleDeskApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let db_path = app_db_path();
        let db = Database::open(&db_path).expect("Failed to open database");

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
        let translations = Translation::all();
        let books = bible_books();

        let settings_lang = settings.language.clone();
        let settings_dark = settings.dark_mode;
        let settings_trans = settings.default_translation.clone();

        let db = Arc::new(Mutex::new(db));

        let saved_verses = db.lock().unwrap().get_saved_verses().unwrap_or_default();
        let reading_plans = db.lock().unwrap().get_reading_plans().unwrap_or_default();
        let memory_cards = db.lock().unwrap().get_memory_cards().unwrap_or_default();

        BibleDeskApp {
            db,
            settings,
            locale,
            current_tab: Tab::Reader,
            selected_book_idx: 0,
            selected_chapter: 1,
            current_chapter: None,
            chapter_loading: false,
            chapter_receiver: None,
            reader_status: String::new(),
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
        }
    }

    fn load_chapter(&mut self) {
        if self.chapter_loading {
            return;
        }
        let book_name = self.books[self.selected_book_idx].name.to_string();
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
            let result = BibleApiClient::fetch_chapter(&book_name, chapter, &translation);
            if let Ok(ref chap) = result {
                if let Ok(db) = db.lock() {
                    let _ = db.cache_verses(&chap.verses);
                }
            }
            let _ = tx.send(result);
        });
    }

    fn poll_chapter_load(&mut self) {
        let received = self.chapter_receiver.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = received {
            self.chapter_loading = false;
            self.chapter_receiver = None;
            match result {
                Ok(chapter) => {
                    self.reader_status = String::new();
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
}

fn app_db_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    format!("{}/bibledesk.db", home)
}

impl eframe::App for BibleDeskApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_chapter_load();
        if self.chapter_loading {
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
    fn show_reader(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        // Controls row
        ui.horizontal(|ui| {
            egui::ComboBox::from_label(self.locale.t("reader.select_book"))
                .selected_text(self.books[self.selected_book_idx].name)
                .show_ui(ui, |ui| {
                    for i in 0..self.books.len() {
                        let name = self.books[i].name;
                        ui.selectable_value(&mut self.selected_book_idx, i, name);
                    }
                });

            ui.label(self.locale.t("reader.chapter"));
            let max_chapters = self.books[self.selected_book_idx].chapters;
            egui::ComboBox::from_id_salt("chapter_sel")
                .selected_text(self.selected_chapter.to_string())
                .show_ui(ui, |ui| {
                    for c in 1..=max_chapters {
                        ui.selectable_value(&mut self.selected_chapter, c, c.to_string());
                    }
                });

            ui.label(self.locale.t("reader.translation"));
            let current_name = self.translations.iter()
                .find(|t| t.id == self.settings.default_translation)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| self.settings.default_translation.clone());
            egui::ComboBox::from_id_salt("trans_sel")
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    for i in 0..self.translations.len() {
                        let id   = self.translations[i].id.clone();
                        let name = self.translations[i].name.clone();
                        let sel  = self.settings.default_translation == id;
                        if ui.selectable_label(sel, &name).clicked() {
                            self.settings.default_translation = id;
                            self.current_chapter = None;
                        }
                    }
                });

            let load_label = if self.chapter_loading {
                self.locale.t("reader.loading")
            } else {
                self.locale.t("reader.load")
            };
            if ui.add_enabled(!self.chapter_loading, egui::Button::new(load_label)).clicked() {
                self.load_chapter();
            }
        });

        // Prev / Next navigation
        ui.horizontal(|ui| {
            let can_prev = self.selected_chapter > 1;
            let can_next = self.selected_chapter < self.books[self.selected_book_idx].chapters;
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
        });

        ui.separator();

        if let Some(chapter) = self.current_chapter.clone() {
            let trans_name = self.translations.iter()
                .find(|t| t.id == chapter.translation)
                .map(|t| t.name.as_str())
                .unwrap_or(chapter.translation.as_str());
            ui.heading(format!("{} ({})", chapter.reference, trans_name));
            ui.separator();

            ScrollArea::vertical().id_salt("reader_scroll").show(ui, |ui| {
                for verse in &chapter.verses {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{}.", verse.verse))
                                .strong()
                                .color(Color32::from_rgb(150, 180, 255)),
                        );
                        ui.label(&verse.text);
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
                    ui.add_space(4.0);
                }
            });
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
                    ui.selectable_value(
                        &mut self.search_translation_filter,
                        "all".to_string(),
                        self.locale.t("search.all_translations"),
                    );
                    for i in 0..self.translations.len() {
                        let id   = self.translations[i].id.clone();
                        let name = self.translations[i].name.clone();
                        ui.selectable_value(&mut self.search_translation_filter, id, name);
                    }
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
                    ui.label(locale_add_chap);
                    ui.horizontal(|ui| {
                        ui.label(locale_book);
                        egui::ComboBox::from_id_salt("plan_book_sel")
                            .selected_text(books[plan_entry_book].name)
                            .show_ui(ui, |ui| {
                                for (i, book) in books.iter().enumerate() {
                                    ui.selectable_value(&mut plan_entry_book, i, book.name);
                                }
                            });
                        ui.label(locale_chap);
                        ui.text_edit_singleline(&mut plan_entry_chap);
                    });
                    ui.horizontal(|ui| {
                        ui.label(locale_date);
                        ui.text_edit_singleline(&mut plan_entry_date);
                    });
                    // Use `ui` (not `cols[1]`) inside the group closure
                    if ui.button(locale_add).clicked() {
                        let chapter: u32 = plan_entry_chap.parse().unwrap_or(1);
                        let book_name    = books[plan_entry_book].name.to_string();
                        let date         = if plan_entry_date.is_empty() {
                            None
                        } else {
                            Some(plan_entry_date.clone())
                        };
                        action.add_entry = Some((plan_id, book_name, chapter, date));
                        action.reload_entries = true;
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
                    ui.selectable_value(
                        &mut self.saved_filter,
                        "all".to_string(),
                        self.locale.t("search.all_translations"),
                    );
                    for i in 0..self.translations.len() {
                        let id   = self.translations[i].id.clone();
                        let name = self.translations[i].name.clone();
                        ui.selectable_value(&mut self.saved_filter, id, name);
                    }
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
                    ui.label(&sv.text);

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
                    for i in 0..self.translations.len() {
                        let id   = self.translations[i].id.clone();
                        let name = self.translations[i].name.clone();
                        ui.selectable_value(&mut self.settings_translation_temp, id, name);
                    }
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
