//! The Finder of pyegui in pure Rust. `finder [folder]`

mod actions;
mod app;
mod dock;
mod glyphs;
mod model;
mod openapps;
mod tagsui;
mod tagstore;
mod search;
mod terminals;
mod viewer;
mod views;

use eframe::egui;

fn main() -> eframe::Result {
    // `finder --custom-titlebar`: no title bar of the system on Windows and Linux (the app draws its own)
    let custom = std::env::args().any(|a| a == "--custom-titlebar") && !cfg!(target_os = "macos");
    let start = std::env::args().skip(1).find(|a| !a.starts_with("--")).map(std::path::PathBuf::from);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Finder")
        .with_inner_size([1200.0, 760.0])
        .with_min_inner_size([640.0, 400.0])
        .with_transparent(true);
    if cfg!(target_os = "macos") {
        // the content goes under the title bar, the buttons of the system stay
        viewport = viewport.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false);
    }
    if custom {
        viewport = viewport.with_decorations(false);
    }
    let options = eframe::NativeOptions {
        viewport,
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native("Finder", options, Box::new(|cc| Ok(Box::new(app::FinderApp::new(cc, start, custom)))))
}
