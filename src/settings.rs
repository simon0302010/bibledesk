use crate::localization::Language;

#[derive(Debug, Clone)]
pub struct Settings {
    pub language: Language,
    pub default_translation: String,
    pub dark_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: Language::English,
            default_translation: "kjv".to_string(),
            dark_mode: true,
        }
    }
}
