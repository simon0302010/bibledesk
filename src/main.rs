mod api;
mod app;
mod db;
mod localization;
mod models;
mod settings;

use app::BibleDeskApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BibleDesk")
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "BibleDesk",
        options,
        Box::new(|cc| {
            // Load a system font with broad Unicode coverage for proper symbol display.
            // Falls back gracefully if the font file is not found on the current platform.
            let mut fonts = egui::FontDefinitions::default();
            let font_paths = [
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
                "C:\\Windows\\Fonts\\arial.ttf",
            ];
            for path in &font_paths {
                if let Ok(data) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "unicode_fallback".to_owned(),
                        egui::FontData::from_owned(data),
                    );
                    // Append as a low-priority fallback for both proportional and monospace.
                    fonts
                        .families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .push("unicode_fallback".to_owned());
                    fonts
                        .families
                        .entry(egui::FontFamily::Monospace)
                        .or_default()
                        .push("unicode_fallback".to_owned());
                    break;
                }
            }
            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(BibleDeskApp::new(cc)))
        }),
    )
}
