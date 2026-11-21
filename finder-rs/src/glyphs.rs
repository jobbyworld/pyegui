//! The icons of the bars: the SVG files of `examples/icons` (the ones of `finder.py`), drawn in
//! the color of the text, or in white on the accent color when selected.

use eframe::egui::{self, Color32, Response};

pub type Glyph = (&'static str, &'static [u8]);

macro_rules! glyph {
    ($name:literal) => {
        ($name, include_bytes!(concat!("../../examples/icons/", $name, ".svg")) as &[u8])
    };
}

pub const SIDEBAR: Glyph = glyph!("sidebar");
pub const CHEVRON_LEFT: Glyph = glyph!("chevron-left");
pub const CHEVRON_RIGHT: Glyph = glyph!("chevron-right");
pub const CHEVRON_UP: Glyph = glyph!("chevron-up");
pub const TREE: Glyph = glyph!("tree");
pub const LIST: Glyph = glyph!("list");
pub const GRID: Glyph = glyph!("grid");
pub const TERMINAL: Glyph = glyph!("terminal");
pub const PANES: Glyph = glyph!("panes");
pub const SUN: Glyph = glyph!("sun");
pub const MOON: Glyph = glyph!("moon");
pub const THEME_SYSTEM: Glyph = glyph!("theme-system");
pub const GEAR: Glyph = glyph!("gear");

pub fn image(glyph: Glyph, size: f32, tint: Color32) -> egui::Image<'static> {
    egui::Image::from_bytes(format!("bytes://glyph/{}.svg", glyph.0), glyph.1).tint(tint).fit_to_exact_size(egui::vec2(size, size))
}

/// The button of an icon: a frame only when selected (white icon on the accent color)
pub fn button_of(ui: &egui::Ui, glyph: Glyph, size: f32, selected: bool) -> egui::Button<'static> {
    let tint = if selected { Color32::WHITE } else { ui.visuals().text_color() };
    egui::Button::image(image(glyph, size, tint)).selected(selected).frame_when_inactive(selected).min_size(egui::vec2(size + 14.0, size + 10.0))
}

pub fn button(ui: &mut egui::Ui, glyph: Glyph, size: f32, selected: bool, tooltip: &str) -> Response {
    let response = ui.add(button_of(ui, glyph, size, selected));
    if tooltip.is_empty() { response } else { response.on_hover_text(tooltip) }
}
