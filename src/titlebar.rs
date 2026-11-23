//! A title bar drawn by the app: `run_native(..., custom_titlebar=True)` removes the title bar of the
//! system, `window_controls()` draws the three buttons (close, minimize, maximize) at the left like
//! macOS, `window_drag_area()` is the part of the bar that moves the window, and the edges of the
//! window resize it (`edge_resize`, called once per frame).
//!
//! On macOS the buttons of the system stay (it is `fullsize_content`): `window_controls` draws nothing
//! there and returns False.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Sense, ViewportCommand};
use pyegui_core::titlebar::{maximized, toggle_maximized};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::{current_ui, kwarg, UI};

static CUSTOM: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_custom(custom: bool) {
    CUSTOM.store(custom, Ordering::Relaxed);
}

/// Resize of the window by its edges: the cursor and `BeginResize` once per frame, after the update
/// function, only for a window without a title bar of the system (`custom_titlebar=True`) that is not
/// maximized. Windows only: elsewhere it does nothing.
pub(crate) fn edge_resize(ctx: &egui::Context) {
    if cfg!(windows) && CUSTOM.load(Ordering::Relaxed) {
        pyegui_core::titlebar::edge_resize(ctx);
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
    pyegui_core::titlebar::window_controls(ui, size, gap);
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
///     fill_height (bool): take all the height left in the ui, to make a whole band (the top band
///         of a sidebar, a toolbar) move the window. Default is False
///
/// The double click maximizes the window, and restores the size and the position it had before when
/// it is maximized (the system remembers them).
///
/// Example::
///
///     pyegui.horizontal(lambda: (pyegui.window_controls(), pyegui.window_drag_area(title="My app")))
#[pyfunction]
#[pyo3(signature = (**kwargs))]
fn window_drag_area(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let title = kwarg::<String>(kwargs, "title")?;
    let ui = unsafe { current_ui(&UI)? };
    let fill = kwarg::<bool>(kwargs, "fill_height")?.unwrap_or(false);
    let height = kwarg::<f32>(kwargs, "height")?.unwrap_or_else(|| {
        if fill {
            ui.available_height()
        } else {
            ui.spacing().interact_size.y
        }
    });
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
