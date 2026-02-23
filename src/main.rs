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
            let font_paths: &[&str] = &[
                // DejaVu Sans — very broad Unicode coverage, installed on most Linux distros
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",      // Debian/Ubuntu
                "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",    // Fedora/RHEL
                "/usr/share/fonts/dejavu/DejaVuSans.ttf",               // OpenSUSE / generic
                "/usr/share/fonts/TTF/DejaVuSans.ttf",                  // Arch Linux
                "/usr/share/fonts/truetype/DejaVuSans.ttf",             // fallback
                // Noto Sans — excellent Unicode coverage on modern distros
                "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
                "/usr/share/fonts/noto/NotoSans-Regular.ttf",
                "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
                // Liberation / FreeSans — common Linux fallbacks
                "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
                // macOS
                "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
                "/System/Library/Fonts/Helvetica.ttc",
                "/Library/Fonts/Arial Unicode.ttf",
                // Windows
                "C:\\Windows\\Fonts\\arial.ttf",
                "C:\\Windows\\Fonts\\seguiemj.ttf", // Segoe UI Emoji
            ];
            for path in font_paths {
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
