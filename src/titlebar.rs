//! A title bar drawn by the app: `run_native(..., custom_titlebar=True)` removes the title bar of the
//! system, `window_controls()` draws the three buttons (close, minimize, maximize) at the left like
//! macOS, `window_drag_area()` is the part of the bar that moves the window, and the edges of the
//! window resize it (`edge_resize`, called once per frame).
//!
//! On macOS the buttons of the system stay (it is `fullsize_content`): `window_controls` draws nothing
//! there and returns False.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, CursorIcon, Sense, ViewportCommand};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::{current_ui, kwarg, UI};

static CUSTOM: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_custom(custom: bool) {
    CUSTOM.store(custom, Ordering::Relaxed);
}

fn custom() -> bool {
    CUSTOM.load(Ordering::Relaxed)
}

/// Width of the zone along the edges of the window that resizes it
const EDGE: f32 = 5.0;

fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false))
}

fn toggle_maximized(ctx: &egui::Context) {
    let now = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!now));
}

/// Resize of the window by its edges: the cursor and `BeginResize` once per frame, after the update
/// function, only for a window without a title bar of the system (`custom_titlebar=True`) that is not
/// maximized.
pub(crate) fn edge_resize(ctx: &egui::Context) {
    #[cfg(target_os = "macos")]
    return;
    #[cfg(not(target_os = "macos"))]
    {
        use egui::viewport::ResizeDirection as D;
        if !custom() || maximized(ctx) {
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
}

/// The three buttons of the window at the left of the bar, like macOS: close (red), minimize
/// (yellow) and maximize or restore (green). Their symbols appear when the pointer is over them.
///
/// Put it first in a horizontal row of the top bar, then `window_drag_area()` for the rest of the
/// row. Use it with `run_native(..., custom_titlebar=True)`. On macOS the window keeps the buttons of
/// the system (`fullsize_content`): nothing is drawn and False is returned.
///
/// Keyword args:
///
///     size (float): diameter of a button. Default is 12
///
///     gap (float): space between the buttons. Default is 8
///
/// Returns:
///     bool: True when the buttons were drawn
///
/// Example::
///
///     pyegui.run_native("App", update, custom_titlebar=True, transparent=True)
///
///     def update(ctx):
///         def bar():
///             pyegui.window_controls()
///             pyegui.window_drag_area(title="My app")
///         pyegui.topbar_panel(lambda: pyegui.horizontal(bar))
#[pyfunction]
#[pyo3(signature = (**kwargs))]
fn window_controls(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<bool> {
    if cfg!(target_os = "macos") {
        return Ok(false);
    }
    let size = kwarg::<f32>(kwargs, "size")?.unwrap_or(12.0).max(6.0);
    let gap = kwarg::<f32>(kwargs, "gap")?.unwrap_or(8.0).max(0.0);
    let ui = unsafe { current_ui(&UI)? };
    let ctx = ui.ctx().clone();
    let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
    let is_max = maximized(&ctx);

    let total = egui::vec2(size * 3.0 + gap * 2.0, size);
    let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
    // the pointer is over the group: the three symbols are shown, like macOS
    let group_hover = ui.rect_contains_pointer(rect);

    let buttons = [
        ("close", Color32::from_rgb(255, 95, 87), "\u{00d7}"),
        ("minimize", Color32::from_rgb(254, 188, 46), "\u{2212}"),
        ("maximize", Color32::from_rgb(40, 200, 64), if is_max { "\u{2199}" } else { "+" }),
    ];
    for (n, (name, color, symbol)) in buttons.into_iter().enumerate() {
        let center = egui::pos2(rect.min.x + size / 2.0 + n as f32 * (size + gap), rect.center().y);
        let hit = egui::Rect::from_center_size(center, egui::Vec2::splat(size));
        #[allow(deprecated)]
        let id = egui::Id::new(("window_controls", name));
        let response = ui.interact(hit, id, Sense::click());
        // a window that is not active shows grey buttons until the pointer comes over them
        let fill = if focused || group_hover { color } else { Color32::from_gray(120) };
        let fill = if response.is_pointer_button_down_on() { fill.gamma_multiply(0.75) } else { fill };
        ui.painter().circle_filled(center, size / 2.0, fill);
        ui.painter()
            .circle_stroke(center, size / 2.0, egui::Stroke::new(0.5, Color32::from_black_alpha(60)));
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
    Ok(true)
}

/// The part of the bar that moves the window: dragging it moves the window, a double click maximizes
/// or restores it. It takes the rest of the width of the row it is in (or `width`), and can show the title of the
/// window in its middle.
///
/// Keyword args:
///
///     title (str): a text drawn in the middle of the area. Default is none
///
///     width (float): width of the area. Default is the rest of the row
///
///     height (float): height of the area. Default is the height of a row of the ui
///
/// Example::
///
///     pyegui.horizontal(lambda: (pyegui.window_controls(), pyegui.window_drag_area(title="My app")))
#[pyfunction]
#[pyo3(signature = (**kwargs))]
fn window_drag_area(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let title = kwarg::<String>(kwargs, "title")?;
    let ui = unsafe { current_ui(&UI)? };
    let height = kwarg::<f32>(kwargs, "height")?.unwrap_or_else(|| ui.spacing().interact_size.y);
    let width = kwarg::<f32>(kwargs, "width")?.unwrap_or_else(|| ui.available_width());
    let size = egui::vec2(width.max(8.0), height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    if let Some(title) = title {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().weak_text_color(),
        );
    }
    let ctx = ui.ctx().clone();
    if response.double_clicked() {
        toggle_maximized(&ctx);
    } else if response.drag_started_by(egui::PointerButton::Primary) {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    Ok(())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(window_controls, m)?)?;
    m.add_function(wrap_pyfunction!(window_drag_area, m)?)?;
    Ok(())
}
