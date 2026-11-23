// no console window behind the app on Windows
#![cfg_attr(windows, windows_subsystem = "windows")]

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
    // no title bar of the system on Windows (the app draws its own: the buttons of the window in its top
    // band); `--custom-titlebar` does the same on Linux, `--system-titlebar` keeps the one of the system
    let args: Vec<String> = std::env::args().collect();
    let custom = !cfg!(target_os = "macos")
        && !args.iter().any(|a| a == "--system-titlebar")
        && (cfg!(windows) || args.iter().any(|a| a == "--custom-titlebar"));
    let start = std::env::args().skip(1).find(|a| !a.starts_with("--")).map(std::path::PathBuf::from);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Finder")
        .with_inner_size([1200.0, 760.0])
        .with_min_inner_size([640.0, 400.0])
        .with_transparent(true);
    // the icon of the window (taskbar, alt-tab): the one of the app
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../examples/app_icon/finder_256.png")) {
        viewport = viewport.with_icon(icon);
    }
    if cfg!(target_os = "macos") {
        // the content goes under the title bar, the buttons of the system stay
        viewport = viewport.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false);
    }
    if custom {
        viewport = viewport.with_decorations(false);
    }
    // the views take part in the drag and drop of the system (out to the Finder, onto a folder)
    pyegui_core::native_dnd::register();
    let options = eframe::NativeOptions {
        viewport,
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native("Finder", options, Box::new(|cc| Ok(Box::new(app::FinderApp::new(cc, start, custom)))))
}
