//! The icons of the bars: the SVG files of `examples/icons` (the ones of `finder.py`), drawn in
//! the color of the text, or in white on the accent color when selected.

use eframe::egui::{self, Color32, Response};

pub type Glyph = (&'static str, &'static [u8]);

macro_rules! glyph {
    ($name:literal) => {
        ($name, include_bytes!(concat!("../../examples/icons/", $name, ".svg")) as &[u8])
    };
}

pub const CHEVRON_LEFT: Glyph = glyph!("chevron-left");
pub const CHEVRON_RIGHT: Glyph = glyph!("chevron-right");
pub const CHEVRON_UP: Glyph = glyph!("chevron-up");
pub const TREE: Glyph = glyph!("tree");
pub const LIST: Glyph = glyph!("list");
pub const GRID: Glyph = glyph!("grid");
pub const TERMINAL: Glyph = glyph!("terminal");
pub const SUN: Glyph = glyph!("sun");
pub const MOON: Glyph = glyph!("moon");
pub const THEME_SYSTEM: Glyph = glyph!("theme-system");
pub const GEAR: Glyph = glyph!("gear");
pub const LINK: Glyph = glyph!("link");
pub const FOLDER_ENTER: Glyph = glyph!("folder-enter");
pub const REFRESH: Glyph = glyph!("refresh");
pub const CLOSE: Glyph = glyph!("close");
pub const DRIVE: Glyph = glyph!("drive");
pub const OPEN: Glyph = glyph!("open");
pub const SEARCH: Glyph = glyph!("search");
pub const ROTATE: Glyph = glyph!("rotate");
pub const ZOOM_IN: Glyph = glyph!("zoom-in");
pub const ZOOM_OUT: Glyph = glyph!("zoom-out");
pub const COPY: Glyph = glyph!("copy");
pub const CHEVRON_DOWN: Glyph = glyph!("chevron-down");
pub const TAG: Glyph = glyph!("tag");
pub const PROPERTY: Glyph = glyph!("property");
pub const TRASH: Glyph = glyph!("trash");

/// The icon at the left of the title of a column
pub fn column_icon(glyph: Glyph) -> pyegui_core::columns::ColumnIcon {
    pyegui_core::columns::ColumnIcon { name: glyph.0.to_owned(), bytes: glyph.1.into() }
}

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
        "link" => glyph!("link"),
        "folder-enter" => glyph!("folder-enter"),
        "sidebar-open" => glyph!("sidebar-open"),
        "sidebar-closed" => glyph!("sidebar-closed"),
        "dock-open" => glyph!("dock-open"),
        "dock-closed" => glyph!("dock-closed"),
        "property" => glyph!("property"),
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
pub use pyegui_core::segmented::Segments;

/// The height of every control of the bands (buttons, fields, segmented controls)
pub use pyegui_core::segmented::CONTROL_H;

/// A row of icons in one rounded frame, the cells that are on drawn on the accent color (the control of
/// pyegui-core, fed with the glyphs of the app)
pub fn segmented(ui: &mut egui::Ui, id: &str, size: f32, cells: &[Cell]) -> Segments {
    let cells: Vec<pyegui_core::segmented::Cell> = cells
        .iter()
        .map(|c| pyegui_core::segmented::Cell {
            image: egui::Image::from_bytes(format!("bytes://glyph/{}.svg", c.glyph.0), c.glyph.1),
            on: c.on,
            enabled: c.enabled,
            tip: c.tip.to_owned(),
        })
        .collect();
    pyegui_core::segmented::segmented(ui, id, size, &cells)
}

/// A menu or pane button: the icon of an action followed by its label
pub fn action_button(ui: &egui::Ui, icon: &str, label: &str) -> egui::Button<'static> {
    egui::Button::image_and_text(image(by_name(icon), 16.0, ui.visuals().text_color()), label.to_string())
}

/// A popup. In the light theme its window is darker than the glass of the bars, so that it stands out
pub fn popup_frame(ctx: &egui::Context) -> egui::Frame {
    let style = ctx.global_style();
    let mut frame = egui::Frame::popup(&style);
    if !style.visuals.dark_mode {
        frame = frame.fill(Color32::from_rgb(222, 224, 233)).stroke(egui::Stroke::new(1.0, Color32::from_rgb(140, 143, 160)));
    }
    frame
}

pub fn modal(ctx: &egui::Context, id: &'static str) -> egui::Modal {
    egui::Modal::new(egui::Id::unique(id)).frame(popup_frame(ctx))
}

/// The fields and buttons of a popup get visible fills and outlines (the glass style makes them transparent).
/// Light: opaque fills and marked outlines. Dark: white at 10 % (14 % outline), 18 % / 24 % when hovered.
pub fn modal_style(v: &mut egui::Visuals) {
    if v.dark_mode {
        let white = |a: u8| Color32::from_white_alpha(a);
        v.extreme_bg_color = Color32::from_white_alpha(12);
        v.text_edit_bg_color = Some(Color32::from_black_alpha(70));
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, white(28));
        for (w, fill, stroke) in [(&mut v.widgets.inactive, 26, 36), (&mut v.widgets.hovered, 46, 61), (&mut v.widgets.active, 60, 80)] {
            w.bg_fill = white(fill);
            w.weak_bg_fill = white(fill);
            w.bg_stroke = egui::Stroke::new(1.0, white(stroke));
        }
        return;
    }
    let field = Color32::WHITE;
    v.extreme_bg_color = field;
    v.text_edit_bg_color = Some(field);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(150, 153, 170));
    for (w, fill, stroke) in [
        (&mut v.widgets.inactive, Color32::from_rgb(246, 247, 251), Color32::from_rgb(110, 113, 130)),
        (&mut v.widgets.hovered, Color32::from_rgb(255, 255, 255), Color32::from_rgb(70, 73, 95)),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = egui::Stroke::new(1.0, stroke);
    }
}

/// `modal_style` on the `Ui` of a popup
pub fn modal_visuals(ui: &mut egui::Ui) {
    modal_style(ui.visuals_mut());
}

/// Runs `f` with the style of the popups on the whole context (for the parts a library draws itself, such as
/// the buttons of `settings_window`), then puts the style back
pub fn with_modal_style<R>(ctx: &egui::Context, f: impl FnOnce() -> R) -> R {
    let saved = ctx.global_style();
    ctx.global_style_mut(|s| modal_style(&mut s.visuals));
    let r = f();
    ctx.set_global_style(saved);
    r
}

/// The title of a popup, with a cross (the SVG `close`) at the top left to close it: true when the cross is clicked
pub fn modal_title(ui: &mut egui::Ui, title: &str) -> bool {
    modal_visuals(ui);
    ui.horizontal(|ui| {
        let tint = ui.visuals().strong_text_color();
        let cross = ui.add(egui::Button::image(image(CLOSE, 14.0, tint)).frame(false).min_size(egui::vec2(24.0, 24.0)));
        ui.heading(title);
        cross.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Fermer (Échap)").clicked()
    })
    .inner
}

/// A small icon button without frame (tooltip on hover)
pub fn small(ui: &mut egui::Ui, glyph: Glyph, tooltip: &str) -> Response {
    let tint = ui.visuals().text_color();
    ui.add(egui::Button::image(image(glyph, 14.0, tint)).frame(false)).on_hover_text(tooltip)
}
