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
pub const OPEN: Glyph = glyph!("open");
pub const MINIMIZE: Glyph = glyph!("minimize");
pub const MAXIMIZE: Glyph = glyph!("maximize");
pub const RESTORE: Glyph = glyph!("restore");
pub const SEARCH: Glyph = glyph!("search");
pub const ROTATE: Glyph = glyph!("rotate");
pub const ZOOM_IN: Glyph = glyph!("zoom-in");
pub const ZOOM_OUT: Glyph = glyph!("zoom-out");
pub const COPY: Glyph = glyph!("copy");
pub const CHEVRON_DOWN: Glyph = glyph!("chevron-down");

/// An icon by the name of its file in `examples/icons`
pub fn by_name(name: &str) -> Glyph {
    match name {
        "checkbox-mixed" => glyph!("checkbox-mixed"),
        "checkbox-off" => glyph!("checkbox-off"),
        "checkbox-on" => glyph!("checkbox-on"),
        "chevron-down" => glyph!("chevron-down"),
        "chevron-left" => glyph!("chevron-left"),
        "chevron-right" => glyph!("chevron-right"),
        "chevron-up" => glyph!("chevron-up"),
        "claude" => glyph!("claude"),
        "close" => glyph!("close"),
        "continuous" => glyph!("continuous"),
        "copy" => glyph!("copy"),
        "cut" => glyph!("cut"),
        "desktop" => glyph!("desktop"),
        "dock" => glyph!("dock"),
        "document" => glyph!("document"),
        "dot" => glyph!("dot"),
        "download" => glyph!("download"),
        "drive" => glyph!("drive"),
        "duplicate" => glyph!("duplicate"),
        "eye" => glyph!("eye"),
        "eye-off" => glyph!("eye-off"),
        "fit-width" => glyph!("fit-width"),
        "folder" => glyph!("folder"),
        "folder-plus" => glyph!("folder-plus"),
        "folder-up" => glyph!("folder-up"),
        "gear" => glyph!("gear"),
        "grid" => glyph!("grid"),
        "group" => glyph!("group"),
        "hand" => glyph!("hand"),
        "home" => glyph!("home"),
        "image" => glyph!("image"),
        "list" => glyph!("list"),
        "maximize" => glyph!("maximize"),
        "minimize" => glyph!("minimize"),
        "moon" => glyph!("moon"),
        "note" => glyph!("note"),
        "open" => glyph!("open"),
        "page" => glyph!("page"),
        "panes" => glyph!("panes"),
        "paste" => glyph!("paste"),
        "refresh" => glyph!("refresh"),
        "rename" => glyph!("rename"),
        "restore" => glyph!("restore"),
        "reveal" => glyph!("reveal"),
        "rotate" => glyph!("rotate"),
        "search" => glyph!("search"),
        "sidebar" => glyph!("sidebar"),
        "sun" => glyph!("sun"),
        "tag" => glyph!("tag"),
        "terminal" => glyph!("terminal"),
        "text-select" => glyph!("text-select"),
        "theme-system" => glyph!("theme-system"),
        "trash" => glyph!("trash"),
        "tree" => glyph!("tree"),
        "zed" => glyph!("zed"),
        "zoom-fit" => glyph!("zoom-fit"),
        "zoom-in" => glyph!("zoom-in"),
        "zoom-out" => glyph!("zoom-out"),
        "bookmark" => glyph!("bookmark"),
        "sidebar-open" => glyph!("sidebar-open"),
        "sidebar-closed" => glyph!("sidebar-closed"),
        "dock-open" => glyph!("dock-open"),
        "dock-closed" => glyph!("dock-closed"),
        _ => glyph!("dot"),
    }
}

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

/// The height of every control of the bands (buttons, fields, segmented controls)
pub const CONTROL_H: f32 = 28.0;

/// A row of icons in one rounded frame, the cells that are on drawn on the accent color: the
/// segmented control of examples/finder.py
pub fn segmented(ui: &mut egui::Ui, id: &str, size: f32, cells: &[Cell]) -> Segments {
    let cell_size = egui::vec2(size + 16.0, CONTROL_H);
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

/// A menu or pane button: the icon of an action followed by its label
pub fn action_button(ui: &egui::Ui, icon: &str, label: &str) -> egui::Button<'static> {
    egui::Button::image_and_text(image(by_name(icon), 16.0, ui.visuals().text_color()), label.to_string())
}

/// A small icon button without frame (tooltip on hover)
pub fn small(ui: &mut egui::Ui, glyph: Glyph, tooltip: &str) -> Response {
    let tint = ui.visuals().text_color();
    ui.add(egui::Button::image(image(glyph, 14.0, tint)).frame(false)).on_hover_text(tooltip)
}
