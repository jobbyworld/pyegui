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
pub const CLOSE: Glyph = glyph!("close");
pub const TAG: Glyph = glyph!("tag");
pub const DRIVE: Glyph = glyph!("drive");
pub const FOLDER: Glyph = glyph!("folder");
pub const IMAGE: Glyph = glyph!("image");
pub const DOWNLOAD: Glyph = glyph!("download");
pub const DOCUMENT: Glyph = glyph!("document");
pub const DESKTOP: Glyph = glyph!("desktop");
pub const HOME: Glyph = glyph!("home");

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

/// One cell of a segmented control
pub struct Cell {
    pub glyph: Glyph,
    /// drawn on the accent color
    pub on: bool,
    pub enabled: bool,
    pub tip: &'static str,
}

pub fn cell(glyph: Glyph, on: bool, tip: &'static str) -> Cell {
    Cell { glyph, on, enabled: true, tip }
}

/// What a segmented control returns: the cell clicked, and the response of each cell (for a context menu)
pub struct Segments {
    pub clicked: Option<usize>,
    pub responses: Vec<Response>,
}

/// A row of icons in one rounded frame, the cells that are on drawn on the accent color: the
/// segmented control of examples/finder.py
pub fn segmented(ui: &mut egui::Ui, id: &str, size: f32, cells: &[Cell]) -> Segments {
    let cell_size = egui::vec2(size + 16.0, size + 10.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(cell_size.x * cells.len() as f32, cell_size.y), egui::Sense::hover());
    let visuals = ui.visuals().clone();
    ui.painter().rect_filled(rect, 8.0, visuals.widgets.inactive.weak_bg_fill.gamma_multiply(0.9));
    ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color), egui::StrokeKind::Inside);
    let mut out = Segments { clicked: None, responses: Vec::new() };
    for (i, c) in cells.iter().enumerate() {
        let cell_rect = egui::Rect::from_min_size(rect.min + egui::vec2(cell_size.x * i as f32, 0.0), cell_size);
        let sense = if c.enabled { egui::Sense::click() } else { egui::Sense::hover() };
        let mut response = ui.interact(cell_rect, ui.id().with((id, i)), sense);
        if !c.tip.is_empty() {
            response = response.on_hover_text(c.tip);
        }
        if c.on {
            ui.painter().rect_filled(cell_rect.shrink(2.0), 6.0, visuals.selection.bg_fill);
        } else if response.hovered() && c.enabled {
            ui.painter().rect_filled(cell_rect.shrink(2.0), 6.0, visuals.widgets.hovered.weak_bg_fill);
        }
        let tint = if c.on {
            Color32::WHITE
        } else if c.enabled {
            visuals.text_color()
        } else {
            visuals.weak_text_color().gamma_multiply(0.6)
        };
        image(c.glyph, size, tint).paint_at(ui, egui::Rect::from_center_size(cell_rect.center(), egui::vec2(size, size)));
        if response.clicked() {
            out.clicked = Some(i);
        }
        out.responses.push(response);
    }
    out
}
