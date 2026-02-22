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
        Box::new(|cc| Ok(Box::new(BibleDeskApp::new(cc)))),
    )
}
