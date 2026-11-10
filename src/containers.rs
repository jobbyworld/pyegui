use eframe::egui;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};

use crate::style::{apply_frame_kwargs, has_frame_kwargs, kwarg_color};
use crate::{
    current_ui, kwarg, panel_parent_ui, run_nested_update_func, with_bool_copy, Bool, UI,
};

// panels

unsafe fn show_panel(
    mut panel: egui::Panel,
    update_fun: Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    if let Some(resizable) = kwarg::<bool>(kwargs, "resizable")? {
        panel = panel.resizable(resizable);
    }
    if let Some(show) = kwarg::<bool>(kwargs, "show_separator_line")? {
        panel = panel.show_separator_line(show);
    }
    if let Some(size) = kwarg::<f32>(kwargs, "default_size")? {
        panel = panel.default_size(size);
    }
    if let Some(size) = kwarg::<f32>(kwargs, "min_size")? {
        panel = panel.min_size(size);
    }
    if let Some(size) = kwarg::<f32>(kwargs, "max_size")? {
        panel = panel.max_size(size);
    }
    if let Some(size) = kwarg::<f32>(kwargs, "exact_size")? {
        panel = panel.exact_size(size);
    }

    let ui = panel_parent_ui(&UI)?;

    if has_frame_kwargs(kwargs)? {
        panel = panel.frame(apply_frame_kwargs(egui::Frame::side_top_panel(ui.style()), kwargs)?);
    }

    match kwarg::<Py<Bool>>(kwargs, "expanded")? {
        Some(expanded) => with_bool_copy(update_fun.py(), &expanded, |is_expanded| {
            panel
                .show_collapsible(ui, is_expanded, |ui| run_nested_update_func(ui, update_fun))
                .map_or(Ok(()), |r| r.inner)
        }),
        None => {
            panel
                .show(ui, |ui| run_nested_update_func(ui, update_fun))
                .inner
        }
    }
}

/// A panel on the left side of the window. The rest of the window is the central panel.
///
/// Panels must be added before any other widget in update_func, otherwise they are placed inside the current ui.
///
/// Keyword args:
///
///     resizable (bool): whether the user can resize the panel. Default is True
///
///     show_separator_line (bool): show a line between the panel and the rest of the window
///
///     default_size (float): the initial width of the panel
///
///     min_size (float): min width of the panel
///
///     max_size (float): max width of the panel
///
///     exact_size (float): fixed width of the panel
///
///     expanded (Bool): show or hide the panel with an animation
///
///     fill, stroke, rounding, inner_margin, outer_margin, shadow: look of the panel, see the frame function
///
/// Example::
///
///     def update_func(ctx):
///         left_panel("navigation", lambda: label("Navigation"), default_size=150)
///         heading("Content")
#[pyfunction]
#[pyo3(signature = (id, update_fun, **kwargs))]
unsafe fn left_panel(
    id: &str,
    update_fun: Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    show_panel(egui::Panel::left(id), update_fun, kwargs)
}

/// A panel on the right side of the window. Accepts the same keyword args as left_panel.
///
/// Example::
///
///     def update_func(ctx):
///         right_panel("properties", lambda: label("Properties"))
///         heading("Content")
#[pyfunction]
#[pyo3(signature = (id, update_fun, **kwargs))]
unsafe fn right_panel(
    id: &str,
    update_fun: Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    show_panel(egui::Panel::right(id), update_fun, kwargs)
}

/// A panel on the top of the window. Accepts the same keyword args as left_panel, sizes being heights.
///
/// Example::
///
///     def update_func(ctx):
///         top_panel("menu", lambda: menu_bar(menus))
///         heading("Content")
#[pyfunction]
#[pyo3(signature = (id, update_fun, **kwargs))]
unsafe fn top_panel(
    id: &str,
    update_fun: Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    show_panel(egui::Panel::top(id), update_fun, kwargs)
}

/// A panel on the bottom of the window. Accepts the same keyword args as left_panel, sizes being heights.
///
/// Example::
///
///     def update_func(ctx):
///         bottom_panel("status", lambda: small("Ready"))
///         heading("Content")
#[pyfunction]
#[pyo3(signature = (id, update_fun, **kwargs))]
unsafe fn bottom_panel(
    id: &str,
    update_fun: Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    show_panel(egui::Panel::bottom(id), update_fun, kwargs)
}

// windows

/// A floating window which can be moved, resized and collapsed.
///
/// Args:
///     title (str): title of the window, also used as its id
///
///     update_func (Callable[[], None]): draws the contents of the window
///
///     open (Bool): if given, the window shows a close button and is shown only when open.value is True
///
/// Keyword args:
///
///     id (str): id of the window, if the title is not unique or changes
///
///     resizable (bool), collapsible (bool), movable (bool), title_bar (bool), scroll (bool)
///
///     default_width (float), default_height (float), min_width (float), min_height (float)
///
///     default_pos (tuple[float, float]): initial position of the window
///
///     fixed_pos (tuple[float, float]): the window cannot be moved from this position
///
///     fill, stroke, rounding, inner_margin, outer_margin, shadow: look of the window, see the frame function
///
/// Example::
///
///     settings_open = Bool(True)
///
///     def update_func(ctx):
///         toggle_value(settings_open, "Settings")
///         window("Settings", lambda: checkbox(dark, "Dark mode"), open=settings_open)
#[pyfunction]
#[pyo3(signature = (title, update_fun, open=None, **kwargs))]
unsafe fn window(
    py: Python<'_>,
    title: &str,
    update_fun: Bound<'_, PyAny>,
    open: Option<Py<Bool>>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let ctx = panel_parent_ui(&UI)?.ctx().clone();
    let mut w = egui::Window::new(title);

    if let Some(id) = kwarg::<String>(kwargs, "id")? {
        w = w.id(egui::Id::unique(id));
    }
    if let Some(resizable) = kwarg::<bool>(kwargs, "resizable")? {
        w = w.resizable(resizable);
    }
    if let Some(collapsible) = kwarg::<bool>(kwargs, "collapsible")? {
        w = w.collapsible(collapsible);
    }
    if let Some(movable) = kwarg::<bool>(kwargs, "movable")? {
        w = w.movable(movable);
    }
    if let Some(title_bar) = kwarg::<bool>(kwargs, "title_bar")? {
        w = w.title_bar(title_bar);
    }
    if let Some(scroll) = kwarg::<bool>(kwargs, "scroll")? {
        w = w.scroll(scroll);
    }
    if let Some(width) = kwarg::<f32>(kwargs, "default_width")? {
        w = w.default_width(width);
    }
    if let Some(height) = kwarg::<f32>(kwargs, "default_height")? {
        w = w.default_height(height);
    }
    if let Some(width) = kwarg::<f32>(kwargs, "min_width")? {
        w = w.min_width(width);
    }
    if let Some(height) = kwarg::<f32>(kwargs, "min_height")? {
        w = w.min_height(height);
    }
    if let Some(pos) = kwarg::<(f32, f32)>(kwargs, "default_pos")? {
        w = w.default_pos(pos);
    }
    if let Some(pos) = kwarg::<(f32, f32)>(kwargs, "fixed_pos")? {
        w = w.fixed_pos(pos);
    }
    if has_frame_kwargs(kwargs)? {
        w = w.frame(apply_frame_kwargs(egui::Frame::window(&ctx.global_style()), kwargs)?);
    }

    let show = |w: egui::Window<'_>| {
        w.show(&ctx, |ui| run_nested_update_func(ui, update_fun))
            .and_then(|r| r.inner)
            .unwrap_or(Ok(()))
    };

    match open {
        Some(open) => with_bool_copy(py, &open, |is_open| show(w.open(is_open))),
        None => show(w),
    }
}

/// A modal dialog. It blocks the interaction with the rest of the app.
///
/// Returns True when the user wants to close it, by pressing escape or clicking outside of the modal.
///
/// Keyword args:
///
///     backdrop_color (color): color covering the rest of the app
///
///     fill, stroke, rounding, inner_margin, outer_margin, shadow: look of the modal, see the frame function
///
/// Example::
///
///     confirm = Bool(False)
///
///     def confirm_ui():
///         heading("Are you sure?")
///         if button_clicked("Yes"):
///             confirm.value = False
///
///     def update_func(ctx):
///         if button_clicked("Delete"):
///             confirm.value = True
///         if confirm.value and modal("confirm", confirm_ui):
///             confirm.value = False
#[pyfunction]
#[pyo3(signature = (id, update_fun, **kwargs))]
unsafe fn modal(
    id: &str,
    update_fun: Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<bool> {
    let ctx = panel_parent_ui(&UI)?.ctx().clone();
    let mut m = egui::Modal::new(egui::Id::unique(id));

    if let Some(color) = kwarg_color(kwargs, "backdrop_color")? {
        m = m.backdrop_color(color);
    }
    if has_frame_kwargs(kwargs)? {
        m = m.frame(apply_frame_kwargs(egui::Frame::popup(&ctx.global_style()), kwargs)?);
    }

    let response = m.show(&ctx, |ui| run_nested_update_func(ui, update_fun));

    let should_close = response.should_close();
    response.inner?;
    Ok(should_close)
}

// grid

/// A simple grid layout. Widgets are added left to right, call end_row to start a new row.
///
/// Keyword args:
///
///     striped (bool): every other row has a different background color
///
///     num_columns (int): the number of columns, needed for striped grids to fill the width
///
///     min_col_width (float), min_row_height (float)
///
///     spacing (tuple[float, float]): space between columns and rows
///
/// Example::
///
///     def grid_ui():
///         label("Name")
///         text_edit_singleline(name)
///         end_row()
///         label("Age")
///         drag_int(age, 0, 150, 1)
///         end_row()
///
///     grid("form", grid_ui, striped=True)
#[pyfunction]
#[pyo3(signature = (id, update_fun, **kwargs))]
unsafe fn grid(
    id: &str,
    update_fun: Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let mut g = egui::Grid::new(id);

    if let Some(striped) = kwarg::<bool>(kwargs, "striped")? {
        g = g.striped(striped);
    }
    if let Some(num_columns) = kwarg::<usize>(kwargs, "num_columns")? {
        g = g.num_columns(num_columns);
    }
    if let Some(width) = kwarg::<f32>(kwargs, "min_col_width")? {
        g = g.min_col_width(width);
    }
    if let Some(height) = kwarg::<f32>(kwargs, "min_row_height")? {
        g = g.min_row_height(height);
    }
    if let Some(spacing) = kwarg::<(f32, f32)>(kwargs, "spacing")? {
        g = g.spacing(spacing);
    }

    g.show(current_ui(&UI)?, |ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// Moves to the next row of a grid.
#[pyfunction]
unsafe fn end_row() -> PyResult<()> {
    current_ui(&UI)?.end_row();
    Ok(())
}

// menus

/// A horizontal bar of menus, usually shown in a top_panel.
///
/// Example::
///
///     def file_menu():
///         if button_clicked("Quit"):
///             exit()
///
///     def menus():
///         menu_button("File", file_menu)
///
///     def update_func(ctx):
///         top_panel("menu", lambda: menu_bar(menus))
#[pyfunction]
unsafe fn menu_bar(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    egui::MenuBar::new()
        .style(crate::style::menu_style())
        .config(crate::style::menu_config())
        .ui(current_ui(&UI)?, |ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// A button showing a menu when clicked. Inside another menu, it shows a sub menu.
///
/// The menu is closed when a button inside it is clicked.
///
/// Example::
///
///     def edit_menu():
///         if button_clicked("Copy"):
///             print("copy")
///         menu_button("More", lambda: button_clicked("Paste special"))
///
///     menu_button("Edit", edit_menu)
///     menu_button("⚙", settings_menu, size=18)
///
/// Args:
///     text (str): the text of the button
///
///     update_func (Callable[[], None]): draws the menu
///
///     size (float): font size of the text of the button. Default is the size of the buttons
#[pyfunction]
#[pyo3(signature = (text, update_fun, size=None))]
unsafe fn menu_button(text: &str, update_fun: Bound<'_, PyAny>, size: Option<f32>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let text = match size {
        Some(size) => egui::RichText::new(text).size(size),
        None => egui::RichText::new(text),
    };
    if egui::containers::menu::is_in_menu(ui) {
        // a sub menu, styled by the menu it is in
        return ui
            .menu_button(text, |ui| run_nested_update_func(ui, update_fun))
            .inner
            .unwrap_or(Ok(()));
    }
    egui::containers::menu::MenuButton::new(text)
        .config(crate::style::menu_config())
        .ui(ui, |ui| run_nested_update_func(ui, update_fun))
        .1
        .map(|inner| inner.inner)
        .unwrap_or(Ok(()))
}

/// Closes the menu, popup or modal the current ui is in.
///
/// Example::
///
///     def menu():
///         checkbox(data, "Option")
///         if button_clicked("Done"):
///             close_menu()
#[pyfunction]
unsafe fn close_menu() -> PyResult<()> {
    current_ui(&UI)?.close();
    Ok(())
}

/// Shows a menu when the contents are right-clicked.
///
/// Example::
///
///     def menu():
///         if button_clicked("Delete"):
///             print("deleted")
///
///     context_menu(lambda: label("Right click me"), menu)
#[pyfunction]
unsafe fn context_menu(update_fun: Bound<'_, PyAny>, menu_fun: Bound<'_, PyAny>) -> PyResult<()> {
    let response = current_ui(&UI)?.scope_builder(
        egui::UiBuilder::new().sense(egui::Sense::click()),
        |ui| run_nested_update_func(ui, update_fun),
    );
    response.inner?;

    let mut result = Ok(());
    egui::Popup::context_menu(&response.response)
        .style(crate::style::menu_style())
        .show(|ui| {
            result = run_nested_update_func(ui, menu_fun);
        });
    result
}

// tooltips

/// Shows a text when the contents are hovered.
///
/// Example::
///
///     tooltip("Saves the file", lambda: button_clicked("Save"))
#[pyfunction]
unsafe fn tooltip(text: &str, update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    let response = current_ui(&UI)?.scope(|ui| run_nested_update_func(ui, update_fun));
    response.response.on_hover_text(text);
    response.inner
}

/// Shows a custom ui when the contents are hovered.
///
/// Example::
///
///     def tooltip_ui():
///         heading("Save")
///         weak("Ctrl+S")
///
///     tooltip_ui(lambda: button_clicked("Save"), tooltip_ui)
#[pyfunction]
unsafe fn tooltip_ui(update_fun: Bound<'_, PyAny>, tooltip_fun: Bound<'_, PyAny>) -> PyResult<()> {
    let response = current_ui(&UI)?.scope(|ui| run_nested_update_func(ui, update_fun));
    response.inner?;

    let mut result = Ok(());
    response.response.on_hover_ui(|ui| {
        result = run_nested_update_func(ui, tooltip_fun);
    });
    result
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(left_panel, m)?)?;
    m.add_function(wrap_pyfunction!(right_panel, m)?)?;
    m.add_function(wrap_pyfunction!(top_panel, m)?)?;
    m.add_function(wrap_pyfunction!(bottom_panel, m)?)?;
    m.add_function(wrap_pyfunction!(window, m)?)?;
    m.add_function(wrap_pyfunction!(modal, m)?)?;
    m.add_function(wrap_pyfunction!(grid, m)?)?;
    m.add_function(wrap_pyfunction!(end_row, m)?)?;
    m.add_function(wrap_pyfunction!(menu_bar, m)?)?;
    m.add_function(wrap_pyfunction!(menu_button, m)?)?;
    m.add_function(wrap_pyfunction!(close_menu, m)?)?;
    m.add_function(wrap_pyfunction!(context_menu, m)?)?;
    m.add_function(wrap_pyfunction!(tooltip, m)?)?;
    m.add_function(wrap_pyfunction!(tooltip_ui, m)?)?;
    Ok(())
}
