//! A segmented control of icons: a row of cells in one rounded frame, the cells that are on drawn on the
//! accent color. The controls of the top and bottom bands of the Finders (navigation, view, panes, theme).
//! Shared by the Python extension (`segmented_buttons`) and the Rust applications.

use egui::{Color32, Image, Rect, Response, Sense, Stroke, StrokeKind, Ui};

/// The height of every control of the bands (buttons, fields, segmented controls)
pub const CONTROL_H: f32 = 28.0;

/// One cell of a segmented control
pub struct Cell {
    /// the icon, not tinted: the control gives it the color of the state of the cell
    pub image: Image<'static>,
    /// drawn on the accent color
    pub on: bool,
    pub enabled: bool,
    pub tip: String,
}

/// What a segmented control returns: the cell clicked, and the response of each cell (for a context menu)
pub struct Segments {
    pub clicked: Option<usize>,
    pub responses: Vec<Response>,
}

/// A row of icons in one rounded frame. `id` tells two controls of one ui apart.
pub fn segmented(ui: &mut Ui, id: &str, size: f32, cells: &[Cell]) -> Segments {
    let cell_size = egui::vec2(size + 16.0, CONTROL_H);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(cell_size.x * cells.len() as f32, cell_size.y), Sense::hover());
    let visuals = ui.visuals().clone();
    // like segmented_control: the cells have the radius of the widgets, the frame two more
    let inner = f32::from(visuals.widgets.inactive.corner_radius.nw).clamp(6.0, 12.0);
    let outer = (inner + 2.0).min(CONTROL_H / 2.0);
    ui.painter().rect_filled(rect, outer, visuals.widgets.inactive.weak_bg_fill.gamma_multiply(0.9));
    ui.painter().rect_stroke(rect, outer, Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color), StrokeKind::Inside);
    let mut out = Segments { clicked: None, responses: Vec::new() };
    for (i, c) in cells.iter().enumerate() {
        let cell_rect = Rect::from_min_size(rect.min + egui::vec2(cell_size.x * i as f32, 0.0), cell_size);
        let sense = if c.enabled { Sense::click() } else { Sense::hover() };
        let mut response = ui.interact(cell_rect, ui.scope_id().with((id, i)), sense);
        if !c.tip.is_empty() {
            response = response.on_hover_text(c.tip.as_str());
        }
        if c.on {
            ui.painter().rect_filled(cell_rect.shrink(2.0), inner, visuals.selection.bg_fill);
        } else if response.hovered() && c.enabled {
            ui.painter().rect_filled(cell_rect.shrink(2.0), inner, visuals.widgets.hovered.weak_bg_fill);
        }
        let tint = if c.on {
            Color32::WHITE
        } else if c.enabled {
            visuals.text_color()
        } else {
            visuals.weak_text_color().gamma_multiply(0.6)
        };
        c.image
            .clone()
            .tint(tint)
            .fit_to_exact_size(egui::vec2(size, size))
            .paint_at(ui, Rect::from_center_size(cell_rect.center(), egui::vec2(size, size)));
        if response.clicked() {
            out.clicked = Some(i);
        }
        out.responses.push(response);
    }
    out
}
