use eframe::egui;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};

use crate::style::{self, apply_frame_kwargs, has_frame_kwargs, kwarg_color, kwarg_stroke};
use crate::{
    current_ui, kwarg, panel_parent_ui, run_nested_update_func, with_bool_copy, Bool, UI,
};

// panels

unsafe fn show_panel(
    mut panel: egui::Panel,
    side: char,
    surface: style::Surface,
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

    // a level of glass given to set_glass_style for this side, unless the call gives a fill
    // the style token of the sidebar (`layout.sidebar.sheet`) comes before the glass
    let token = (surface == style::Surface::Sidebar)
        .then(|| crate::layout::token_sheet("sidebar", ui.style().visuals.dark_mode))
        .flatten();
    let glass = token.or_else(|| style::surface_fill(&ui.style().visuals, surface));
    // the region of the tokens of the style sheet: a top or bottom bar is a box only when the sheet gives it a
    // sheet (`layout.topbar.sheet`), with the outline and the rounding of its tokens; otherwise it is the glass
    let region = match side {
        't' => "topbar",
        'b' => "bottombar",
        _ => "sidebar",
    };
    let dark = ui.style().visuals.dark_mode;
    let bar_box = matches!(side, 't' | 'b').then(|| pyegui_core::sheet::bar_box(region, dark)).flatten();
    let glass = bar_box.map(|b| b.fill).or(glass);
    // the color under the separator line: the line takes a strip of the outer margin that the fill of
    // the frame does not cover, and a line that is not opaque would show the window behind it
    let strip_fill = if kwarg_color(kwargs, "fill")?.is_some() { kwarg_color(kwargs, "fill")? } else { glass };
    if has_frame_kwargs(kwargs)? || glass.is_some() {
        let mut frame = egui::Frame::side_top_panel(ui.style());
        if let Some(fill) = glass {
            frame = frame.fill(fill);
        }
        if let Some(b) = bar_box {
            frame = frame.stroke(b.outline).corner_radius(b.rounding.round().clamp(0.0, 255.0) as u8);
            // a box has its outline: no separator line too, unless the call asks for one
            if kwarg::<bool>(kwargs, "show_separator_line")?.is_none() {
                panel = panel.show_separator_line(false);
            }
        }
        panel = panel.frame(apply_frame_kwargs(frame, kwargs)?);
    }

    // the line between the panel and the rest is drawn with the stroke of the noninteractive
    // widgets: given, it is that stroke while the panel is shown (its content draws with it too).
    // Not given, the token `layout.<region>.outline` of the style sheet gives it ("none": no line)
    let given_separator = kwarg_stroke(kwargs, "separator_stroke")?;
    let separator = match given_separator {
        Some(stroke) => Some(stroke),
        None => pyegui_core::sheet::layout_outline(region, dark),
    };
    let previous = ui.visuals().widgets.noninteractive.bg_stroke;
    if let Some(stroke) = separator {
        ui.visuals_mut().widgets.noninteractive.bg_stroke = stroke;
    }
    // the resize handle: `layout.<region>.resize_stroke` (the line at rest, after `separator_stroke`),
    // `resize_hover_stroke` and `resize_active_stroke` of the style sheet. egui draws it with the strokes of the
    // parent ui; the content gets the strokes it had back
    let resize = pyegui_core::sheet::resize_strokes(region, dark).map(|mut r| {
        if given_separator.is_some() {
            r.idle = None;
        }
        r
    });
    let separator_kwarg = kwarg::<bool>(kwargs, "show_separator_line")?;
    if let (Some(r), None) = (resize, separator_kwarg) {
        panel = r.panel(panel);
    }
    let saved = resize.map(|r| r.apply(ui.visuals_mut()));
    let line = ui.visuals().widgets.noninteractive.bg_stroke;
    let shows_line = separator_kwarg
        .or_else(|| resize.and_then(|r| r.idle).map(|s| s.width > 0.0))
        .unwrap_or(bar_box.is_none());
    // the strip under the line is painted under what the panel draws (its line at rest, under the mouse or dragged)
    let strip_slot = ui.painter().add(egui::Shape::Noop);
    let mut rect = None;
    let result = match kwarg::<Py<Bool>>(kwargs, "expanded")? {
        Some(expanded) => with_bool_copy(update_fun.py(), &expanded, |is_expanded| {
            panel
                .show_collapsible(ui, is_expanded, |ui| {
                    if let Some(saved) = saved {
                        saved.restore(ui.visuals_mut());
                    }
                    run_nested_update_func(ui, update_fun)
                })
                .map_or(Ok(()), |r| {
                    rect = Some(r.response.rect);
                    r.inner
                })
        }),
        None => {
            let response = panel.show(ui, |ui| {
                if let Some(saved) = saved {
                    saved.restore(ui.visuals_mut());
                }
                run_nested_update_func(ui, update_fun)
            });
            rect = Some(response.response.rect);
            response.inner
        }
    };
    if let Some(saved) = saved {
        saved.restore(ui.visuals_mut());
    }
    if let (Some(rect), Some(fill)) = (rect, strip_fill) {
        if shows_line && line.width > 0.0 && fill.a() < 255 {
            // the strip of the line, at the outer edge of the panel, under the line: the line takes a strip of
            // the outer margin that the fill of the frame does not cover
            let w = line.width.round().max(1.0);
            let strip = match side {
                'l' => egui::Rect::from_min_max(egui::pos2(rect.right() - w, rect.top()), rect.right_bottom()),
                'r' => egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + w, rect.bottom())),
                't' => egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - w), rect.right_bottom()),
                _ => egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.right(), rect.top() + w)),
            };
            ui.painter().set(strip_slot, egui::Shape::rect_filled(strip, 0.0, fill));
        }
    }
    if separator.is_some() {
        ui.visuals_mut().widgets.noninteractive.bg_stroke = previous;
    }
    result
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
///     separator_stroke (tuple[float, color]): width and color of that line. Default is the token
///         `layout.<region>.resize_stroke` of the style sheet, then `layout.<region>.outline` (the region is "sidebar"
///         for left_panel and right_panel, "topbar" for top_panel, "bottombar" for bottom_panel; "none" removes the
///         line), else the line of the theme
///
///     The style sheet gives the line of the edge that resizes the panel: `layout.<region>.resize_stroke` at rest
///     (a stroke, or "none" for no line; it also shows or hides the separator line when show_separator_line is not
///     given), `layout.<region>.resize_hover_stroke` under the mouse and `layout.<region>.resize_active_stroke` while
///     it is dragged (`layout.resize_stroke`... for all the regions). Not given, the line is the stroke of the
///     widgets of the theme (`widgets.hovered.fg_stroke`, `widgets.active.fg_stroke`); the widgets inside the panel
///     keep the strokes of the theme.
///
///     The style sheet can make a top or bottom panel a box: with `layout.topbar.sheet` (or `layout.bottombar.sheet`)
///     its fill is that color, its outline `layout.topbar.outline` and its rounding `layout.topbar.rounding`. Without
///     a sheet token the panel keeps its glass, with no box. The keyword args of the call win over the tokens.
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
    show_panel(egui::Panel::left(id), 'l', style::Surface::Sidebar, update_fun, kwargs)
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
    show_panel(egui::Panel::right(id), 'r', style::Surface::Sidebar, update_fun, kwargs)
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
    show_panel(egui::Panel::top(id), 't', style::Surface::Bars, update_fun, kwargs)
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
    show_panel(egui::Panel::bottom(id), 'b', style::Surface::Bars, update_fun, kwargs)
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
///     anchor (str): the window is anchored to a corner or a side of the screen and cannot be moved: one of
///         "left_top", "center_top", "right_top", "left_center", "center_center", "right_center",
///         "left_bottom", "center_bottom", "right_bottom"
///
///     anchor_offset (tuple[float, float]): the distance from the anchor, e.g. (-8, -40) from "right_bottom"
///         puts the window 8 points from the right edge and 40 above the bottom one
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
    if let Some(anchor) = kwarg::<String>(kwargs, "anchor")? {
        let align = match anchor.as_str() {
            "left_top" => egui::Align2::LEFT_TOP,
            "center_top" => egui::Align2::CENTER_TOP,
            "right_top" => egui::Align2::RIGHT_TOP,
            "left_center" => egui::Align2::LEFT_CENTER,
            "center_center" => egui::Align2::CENTER_CENTER,
            "right_center" => egui::Align2::RIGHT_CENTER,
            "left_bottom" => egui::Align2::LEFT_BOTTOM,
            "center_bottom" => egui::Align2::CENTER_BOTTOM,
            "right_bottom" => egui::Align2::RIGHT_BOTTOM,
            other => return Err(pyo3::exceptions::PyValueError::new_err(format!("Unknown anchor '{other}'"))),
        };
        let (x, y) = kwarg::<(f32, f32)>(kwargs, "anchor_offset")?.unwrap_or((0.0, 0.0));
        w = w.anchor(align, [x, y]);
    }
    let window_fill = pyegui_core::popup_style::fill(&ctx.global_style(), pyegui_core::popup_style::Kind::Window);
    if has_frame_kwargs(kwargs)? || window_fill.is_some() {
        let mut base = egui::Frame::window(&ctx.global_style());
        if let Some(color) = window_fill {
            base = base.fill(color);
        }
        w = w.frame(apply_frame_kwargs(base, kwargs)?);
    }

    let show = |w: egui::Window<'_>| {
        w.show(&ctx, |ui| {
            pyegui_core::context_style::apply_ui(ui, pyegui_core::context_style::Context::Window);
            run_nested_update_func(ui, update_fun)
        })
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
    let window_fill = pyegui_core::popup_style::fill(&ctx.global_style(), pyegui_core::popup_style::Kind::Window);
    if has_frame_kwargs(kwargs)? || window_fill.is_some() {
        let mut base = egui::Frame::popup(&ctx.global_style());
        if let Some(color) = window_fill {
            base = base.fill(color);
        }
        m = m.frame(apply_frame_kwargs(base, kwargs)?);
    }

    let response = m.show(&ctx, |ui| {
        pyegui_core::context_style::apply_ui(ui, pyegui_core::context_style::Context::Window);
        run_nested_update_func(ui, update_fun)
    });

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
    let ctx = response.response.ctx.clone();
    pyegui_core::popup_style::with_tooltip_fill(&ctx, || response.response.on_hover_text(text));
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
    let ctx = response.response.ctx.clone();
    pyegui_core::popup_style::with_tooltip_fill(&ctx, || {
        response.response.on_hover_ui(|ui| {
            result = run_nested_update_func(ui, tooltip_fun);
        });
    });
    result
}

/// A row with a left part, a central part and a right part, laid out in one pass: the right part sticks
/// to the right edge, the left part to the left edge, and the central part takes the room that is left
/// between them. Nothing is measured at a previous frame, so the parts do not move when the window is
/// resized or when a part changes.
///
/// The width of a part that is placed by its right side (right, and center unless it is "left") is found
/// by drawing it once without showing it, so those functions run twice per frame (the first time
/// their widgets do nothing). They are called in this order: right, left, center. `available_width()`
/// in a left central part is the room left between the two sides, which is what a path or a search
/// field should take (`path_breadcrumb(max_width=available_width())`). A part may be None.
///
/// Args:
///     left (callable): drawn from the left edge, left to right
///
///     center (callable): drawn in the room between the left and the right parts
///
///     right (callable): drawn left to right like the others, and placed against the right edge
///
/// Keyword args:
///
///     height (float): height of the row, the parts are centered in it. Default is the height of a
///         button; `available_height()` fills a panel
///
///     gap (float): space kept between the parts. Default is the spacing between items
///
///     center_align (str): "left" (right after the left part), "center" (in the middle of the room)
///         or "right" (against the right part). Default is "center"
///
/// Example::
///
///     def left():
///         button_clicked("Back")
///
///     def right():
///         label("3 items")        # then the button, against the right edge
///         button_clicked("Settings")
///
///     bar(left=left, center=lambda: label("title"), right=right)
#[pyfunction]
#[pyo3(signature = (left=None, center=None, right=None, **kwargs))]
unsafe fn bar(
    left: Option<Bound<'_, PyAny>>,
    center: Option<Bound<'_, PyAny>>,
    right: Option<Bound<'_, PyAny>>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let gap = kwarg::<f32>(kwargs, "gap")?.unwrap_or(ui.spacing().item_spacing.x).max(0.0);
    let height = kwarg::<f32>(kwargs, "height")?.unwrap_or(ui.spacing().interact_size.y).max(1.0);
    let align = kwarg::<String>(kwargs, "center_align")?.unwrap_or_else(|| "center".to_string());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::hover());
    let centered = egui::Align::Center;
    let ltr = egui::Layout::left_to_right(centered);
    let child = |ui: &mut egui::Ui, name: &str, rect: egui::Rect| {
        ui.new_child(egui::UiBuilder::new().id_salt(("bar", name)).max_rect(rect).layout(ltr))
    };
    // the width of a part: it is drawn once without being seen, its widgets do nothing
    let measured = |ui: &mut egui::Ui, name: &str, rect: egui::Rect, f: &Bound<'_, PyAny>| -> PyResult<f32> {
        let mut probe = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("bar_probe", name))
                .max_rect(rect)
                .layout(ltr)
                .sizing_pass()
                .invisible(),
        );
        run_nested_update_func(&mut probe, f.clone())?;
        Ok(probe.min_rect().width().min(rect.width()))
    };

    // the right part first: its width tells where the others stop
    let mut right_edge = rect.max.x;
    if let Some(right) = right {
        let width = measured(ui, "right", rect, &right)?;
        if width > 0.0 {
            let at = egui::Rect::from_min_max(egui::pos2(rect.max.x - width, rect.min.y), rect.max);
            let mut part = child(ui, "right", at);
            run_nested_update_func(&mut part, right)?;
            right_edge = (at.min.x - gap).max(rect.min.x);
        }
    }
    let mut left_edge = rect.min.x;
    if let Some(left) = left {
        let room = egui::Rect::from_min_max(rect.min, egui::pos2(right_edge, rect.max.y));
        let mut part = child(ui, "left", room);
        run_nested_update_func(&mut part, left)?;
        if part.min_rect().width() > 0.0 {
            left_edge = (part.min_rect().max.x + gap).min(right_edge);
        }
    }
    if let Some(center) = center {
        let room = egui::Rect::from_min_max(egui::pos2(left_edge, rect.min.y), egui::pos2(right_edge.max(left_edge), rect.max.y));
        match align.as_str() {
            "left" => {
                let mut part = child(ui, "center", room);
                run_nested_update_func(&mut part, center)?;
            }
            _ => {
                // "right" and "center": the width of the part tells where it starts
                let width = measured(ui, "center", room, &center)?;
                let x = if align == "right" {
                    room.max.x - width
                } else {
                    room.center().x - width / 2.0
                };
                let x = x.clamp(room.min.x, room.max.x.max(room.min.x));
                let at = egui::Rect::from_min_max(egui::pos2(x, room.min.y), egui::pos2(room.max.x.max(x), room.max.y));
                let mut part = child(ui, "center", at);
                run_nested_update_func(&mut part, center)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(bar, m)?)?;
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
