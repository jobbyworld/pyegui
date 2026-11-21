//! The Finder of pyegui in pure Rust. `finder [folder]`

mod app;
mod model;
mod views;

use eframe::egui;

fn main() -> eframe::Result {
    let start = std::env::args().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Finder")
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([640.0, 400.0]),
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native("Finder", options, Box::new(|cc| Ok(Box::new(app::FinderApp::new(cc, start)))))
}
