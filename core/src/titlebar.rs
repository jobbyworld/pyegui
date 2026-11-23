//! A title bar drawn by the app, without Python: the three buttons of the window at the left (close,
//! minimize, maximize) like macOS, and the resize of a window without a title bar of the system by its
//! edges. Shared by the Python extension (`custom_titlebar=True`) and the Rust Finder.

use egui::{Color32, Sense, ViewportCommand};

/// Width of the zone along the edges of the window that resizes it
const EDGE: f32 = 5.0;

/// The window is maximized (or fullscreen)
pub fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false))
}

/// Maximizes the window, or restores the size and position it had before
pub fn toggle_maximized(ctx: &egui::Context) {
    let now = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!now));
}

/// Resize of the window by its edges: the cursor and `BeginResize`, to call once per frame for a window
/// without a title bar of the system. Does nothing when the window is maximized.
pub fn edge_resize(ctx: &egui::Context) {
    use egui::viewport::ResizeDirection as D;
    use egui::CursorIcon;
    if maximized(ctx) {
        return;
    }
    let rect = ctx.content_rect();
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    if !rect.contains(pos) {
        return;
    }
    let left = pos.x - rect.min.x < EDGE;
    let right = rect.max.x - pos.x < EDGE;
    let top = pos.y - rect.min.y < EDGE;
    let bottom = rect.max.y - pos.y < EDGE;
    let (dir, icon) = match (left, right, top, bottom) {
        (true, _, true, _) => (D::NorthWest, CursorIcon::ResizeNwSe),
        (_, true, true, _) => (D::NorthEast, CursorIcon::ResizeNeSw),
        (true, _, _, true) => (D::SouthWest, CursorIcon::ResizeNeSw),
        (_, true, _, true) => (D::SouthEast, CursorIcon::ResizeNwSe),
        (true, ..) => (D::West, CursorIcon::ResizeHorizontal),
        (_, true, ..) => (D::East, CursorIcon::ResizeHorizontal),
        (_, _, true, _) => (D::North, CursorIcon::ResizeVertical),
        (_, _, _, true) => (D::South, CursorIcon::ResizeVertical),
        _ => return,
    };
    ctx.set_cursor_icon(icon);
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
    }
}

/// The three buttons of the window, like macOS: close (red), minimize (yellow) and maximize or restore
/// (green), `size` points wide with `gap` between them. Their symbols appear when the pointer is over
/// them; a window that is not active shows grey buttons.
pub fn window_controls(ui: &mut egui::Ui, size: f32, gap: f32) {
    let ctx = ui.ctx().clone();
    let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
    let is_max = maximized(&ctx);

    let total = egui::vec2(size * 3.0 + gap * 2.0, size);
    let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
    let group_hover = ui.rect_contains_pointer(rect);

    let buttons = [
        ("close", Color32::from_rgb(255, 95, 87), "\u{00d7}"),
        ("minimize", Color32::from_rgb(254, 188, 46), "\u{2212}"),
        ("maximize", Color32::from_rgb(40, 200, 64), if is_max { "\u{2199}" } else { "+" }),
    ];
    for (n, (name, color, symbol)) in buttons.into_iter().enumerate() {
        let center = egui::pos2(rect.min.x + size / 2.0 + n as f32 * (size + gap), rect.center().y);
        let hit = egui::Rect::from_center_size(center, egui::Vec2::splat(size));
        let response = ui.interact(hit, ui.id().with(("window_controls", name)), Sense::click());
        let fill = if focused || group_hover { color } else { Color32::from_gray(120) };
        let fill = if response.is_pointer_button_down_on() { fill.gamma_multiply(0.75) } else { fill };
        ui.painter().circle_filled(center, size / 2.0, fill);
        ui.painter().circle_stroke(center, size / 2.0, egui::Stroke::new(0.5, Color32::from_black_alpha(60)));
        if group_hover {
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                symbol,
                egui::FontId::proportional(size * 0.9),
                Color32::from_black_alpha(170),
            );
        }
        if response.clicked() {
            match name {
                "close" => ctx.send_viewport_cmd(ViewportCommand::Close),
                "minimize" => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                _ => toggle_maximized(&ctx),
            }
        }
    }
}
