use eframe::egui;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyAny;

use crate::style::extract_color;
use crate::{current_ui, run_nested_call, run_nested_update_func, Bool, Float, UI};

/// Shows a text in the given color.
///
/// Args:
///     color (color): (r, g, b), (r, g, b, a) or "#rrggbb[aa]"
///
///     text (str): the text
///
/// Example::
///
///     colored_label((220, 50, 50), "Error")
///     colored_label("#22c55e", "Saved")
#[pyfunction]
unsafe fn colored_label(color: &Bound<'_, PyAny>, text: &str) -> PyResult<()> {
    let color = extract_color(color)?;
    current_ui(&UI)?.colored_label(color, text);
    Ok(())
}

/// Shows a text that looks selected when selected is True, like a tab or an item of a list.
/// Returns True when it is clicked.
///
/// Example::
///
///     selected = Bool(False)
///     # inside update_func
///     if selectable_label_clicked(selected.value, "Bold"):
///         selected.value = not selected.value
#[pyfunction]
unsafe fn selectable_label_clicked(selected: bool, text: &str) -> PyResult<bool> {
    Ok(current_ui(&UI)?.selectable_label(selected, text).clicked())
}

/// Splits the available width in columns of the same width.
/// update_func is called once for each column, with the index of the column.
///
/// Example::
///
///     def column(i):
///         heading(f"Column {i}")
///         label("Some text")
///
///     columns(3, column)
#[pyfunction]
unsafe fn columns(count: usize, update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    if count == 0 {
        return Err(PyValueError::new_err("columns needs at least one column"));
    }
    current_ui(&UI)?.columns(count, |columns| {
        columns
            .iter_mut()
            .enumerate()
            .try_for_each(|(i, ui)| run_nested_call(ui, &update_fun, (i,)))
    })
}

/// A color with transparency, each channel between 0 and 1
///
/// Usage::
///
///     color = RGBA(0.2, 0.5, 1.0, 0.5)
///     color_edit_button_rgba(color)
// named like RGB, the Python class of the colors without transparency
#[allow(clippy::upper_case_acronyms)]
#[pyclass]
pub(crate) struct RGBA {
    #[pyo3(get, set)]
    r: f32,
    #[pyo3(get, set)]
    g: f32,
    #[pyo3(get, set)]
    b: f32,
    #[pyo3(get, set)]
    a: f32,
}

#[pymethods]
impl RGBA {
    #[new]
    #[pyo3(signature = (r, g, b, a=1.0))]
    fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        RGBA { r, g, b, a }
    }

    /// The color as (r, g, b, a) with channels between 0 and 255, as taken by the style functions
    fn to_tuple(&self) -> (u8, u8, u8, u8) {
        let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        (channel(self.r), channel(self.g), channel(self.b), channel(self.a))
    }

    fn __repr__(&self) -> String {
        // rounded: the f32 channels show noise like 0.23000003
        let channel = |value: f32| {
            let text = format!("{:.3}", value);
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        };
        format!(
            "RGBA({}, {}, {}, {})",
            channel(self.r),
            channel(self.g),
            channel(self.b),
            channel(self.a)
        )
    }
}

/// Shows a button with the given color. If the user clicks the button, a full color picker
/// with an opacity slider is shown.
///
/// Example::
///
///     color = RGBA(0.2, 0.5, 1.0, 0.5)
///     # inside update_func
///     color_edit_button_rgba(color)
///     colored_label(color.to_tuple(), "This text uses the color")
#[pyfunction]
unsafe fn color_edit_button_rgba(rgba: &mut RGBA) -> PyResult<()> {
    let mut tmp = [rgba.r, rgba.g, rgba.b, rgba.a];
    current_ui(&UI)?.color_edit_button_rgba_unmultiplied(&mut tmp);
    [rgba.r, rgba.g, rgba.b, rgba.a] = tmp;
    Ok(())
}

/// Edits an angle by dragging. The value is in radians, it is shown in degrees.
///
/// Example::
///
///     angle = Float(0.0)
///     # inside update_func
///     drag_angle(angle)
///     label(f"{math.degrees(angle.value):.0f}°")
#[pyfunction]
unsafe fn drag_angle(value: &mut Float) -> PyResult<()> {
    current_ui(&UI)?.drag_angle(&mut value.value);
    Ok(())
}

/// The Python object carried by a drag and drop
struct DndPayload(Py<PyAny>);

/// Makes the widgets drawn by update_func draggable. When they are dropped on a dnd_drop_zone,
/// the zone returns payload.
///
/// Args:
///     id (str): unique id of the drag source
///
///     payload (Any): any Python object, returned by the drop zone
///
///     update_func (Callable[[], None]): draws the dragged widgets
///
/// Example::
///
///     for i, fruit in enumerate(fruits):
///         dnd_drag_source(f"fruit {i}", fruit, lambda: label(fruit))
#[pyfunction]
unsafe fn dnd_drag_source(
    id: &str,
    payload: Py<PyAny>,
    update_fun: Bound<'_, PyAny>,
) -> PyResult<()> {
    current_ui(&UI)?
        .dnd_drag_source(egui::Id::unique(("pyegui dnd", id)), DndPayload(payload), |ui| {
            run_nested_update_func(ui, update_fun)
        })
        .inner
}

/// Draws update_func in a frame that lights up when something is dragged over it.
/// Returns the payload of the dnd_drag_source dropped on it this frame, or None.
///
/// Example::
///
///     basket = []
///
///     def update_func(ctx):
///         dnd_drag_source("apple", "apple", lambda: label("🍎 apple"))
///         dropped = dnd_drop_zone(lambda: label(f"Basket: {basket}"))
///         if dropped is not None:
///             basket.append(dropped)
#[pyfunction]
unsafe fn dnd_drop_zone(py: Python<'_>, update_fun: Bound<'_, PyAny>) -> PyResult<Option<Py<PyAny>>> {
    let ui = current_ui(&UI)?;
    let frame = egui::Frame::group(ui.style());
    let (response, payload) =
        ui.dnd_drop_zone::<DndPayload, _>(frame, |ui| run_nested_update_func(ui, update_fun));
    response.inner?;
    Ok(payload.map(|payload| payload.0.clone_ref(py)))
}

/// Shows a radio button that looks checked when selected is True. Returns True when it is clicked.
///
/// Example::
///
///     dark = Bool(False)
///     # inside update_func
///     if radio_clicked(not dark.value, "Light"):
///         dark.value = False
///     if radio_clicked(dark.value, "Dark"):
///         dark.value = True
#[pyfunction]
unsafe fn radio_clicked(selected: bool, text: &str) -> PyResult<bool> {
    Ok(current_ui(&UI)?.radio(selected, text).clicked())
}

/// Width left for the next widgets, in points.
///
/// Example::
///
///     if available_width() < 400:
///         label("narrow layout")
#[pyfunction]
unsafe fn available_width() -> PyResult<f32> {
    Ok(current_ui(&UI)?.available_width())
}

/// Height left for the next widgets, in points.
#[pyfunction]
unsafe fn available_height() -> PyResult<f32> {
    Ok(current_ui(&UI)?.available_height())
}

/// Sets the width of the current ui: the widgets after it wrap and justify to this width.
///
/// Example::
///
///     def sidebar():
///         set_width(200)
///         label("200 points wide")
///     vertical(sidebar)
#[pyfunction]
unsafe fn set_width(width: f32) -> PyResult<()> {
    current_ui(&UI)?.set_width(width);
    Ok(())
}

/// Sets the minimum width of the current ui
#[pyfunction]
unsafe fn set_min_width(width: f32) -> PyResult<()> {
    current_ui(&UI)?.set_min_width(width);
    Ok(())
}

/// Sets the maximum width of the current ui
#[pyfunction]
unsafe fn set_max_width(width: f32) -> PyResult<()> {
    current_ui(&UI)?.set_max_width(width);
    Ok(())
}

/// Sets the height of the current ui
#[pyfunction]
unsafe fn set_height(height: f32) -> PyResult<()> {
    current_ui(&UI)?.set_height(height);
    Ok(())
}

/// Sets the minimum height of the current ui
#[pyfunction]
unsafe fn set_min_height(height: f32) -> PyResult<()> {
    current_ui(&UI)?.set_min_height(height);
    Ok(())
}

/// Sets the maximum height of the current ui
#[pyfunction]
unsafe fn set_max_height(height: f32) -> PyResult<()> {
    current_ui(&UI)?.set_max_height(height);
    Ok(())
}

/// Scrolls the scroll area the current ui is in, so the next widget is visible.
///
/// Args:
///     align (str): where the next widget goes: "top", "center" or "bottom". Default is the
///         least scrolling that shows it
///
/// Example::
///
///     def log_ui():
///         for line in lines:
///             label(line)
///         if new_line_added:
///             scroll_to_cursor("bottom")
///
///     scroll_area_vertical(log_ui)
#[pyfunction]
#[pyo3(signature = (align=None))]
unsafe fn scroll_to_cursor(align: Option<&str>) -> PyResult<()> {
    let align = match align {
        None => None,
        Some("top") => Some(egui::Align::TOP),
        Some("center") => Some(egui::Align::Center),
        Some("bottom") => Some(egui::Align::BOTTOM),
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "Unknown align '{}', expected top, center or bottom",
                other
            )))
        }
    };
    current_ui(&UI)?.scroll_to_cursor(align);
    Ok(())
}

/// True if the widgets of the current ui can be interacted with, see disable() and add_enabled()
#[pyfunction]
unsafe fn is_enabled() -> PyResult<bool> {
    Ok(current_ui(&UI)?.is_enabled())
}

/// True if the widgets of the current ui are shown, see set_invisible()
#[pyfunction]
unsafe fn is_visible() -> PyResult<bool> {
    Ok(current_ui(&UI)?.is_visible())
}

/// True if the mouse is over the current ui, e.g. to show buttons on hover.
///
/// Example::
///
///     def row():
///         label(name)
///         if ui_contains_pointer():
///             small_button_clicked("✕")
///     horizontal(row)
#[pyfunction]
unsafe fn ui_contains_pointer() -> PyResult<bool> {
    Ok(current_ui(&UI)?.ui_contains_pointer())
}

/// Number of physical pixels for each point of the ui. 2.0 on most retina screens
#[pyfunction]
unsafe fn pixels_per_point() -> PyResult<f32> {
    Ok(current_ui(&UI)?.pixels_per_point())
}

fn menu_image(source: &str, size: f32) -> egui::Image<'_> {
    egui::Image::new(source).fit_to_exact_size(egui::vec2(size, size))
}

/// A button with an image, showing a menu when clicked. See menu_button.
///
/// Args:
///     source (str): uri of the image, like for image()
///
///     update_func (Callable[[], None]): draws the menu
///
///     size (float): size of the image. Default is 16
///
/// Example::
///
///     menu_image_button("file://assets/settings.png", settings_menu)
#[pyfunction]
#[pyo3(signature = (source, update_fun, size=16.0))]
unsafe fn menu_image_button(source: &str, update_fun: Bound<'_, PyAny>, size: f32) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    if egui::containers::menu::is_in_menu(ui) {
        return ui
            .menu_image_button(menu_image(source, size), |ui| run_nested_update_func(ui, update_fun))
            .inner
            .unwrap_or(Ok(()));
    }
    egui::containers::menu::MenuButton::from_button(egui::Button::image(menu_image(source, size)))
        .config(crate::style::menu_config())
        .ui(ui, |ui| run_nested_update_func(ui, update_fun))
        .1
        .map(|inner| inner.inner)
        .unwrap_or(Ok(()))
}

/// A button with an image and a text, showing a menu when clicked. See menu_button.
///
/// Example::
///
///     menu_image_text_button("file://assets/settings.png", "Settings", settings_menu)
#[pyfunction]
#[pyo3(signature = (source, text, update_fun, size=16.0))]
unsafe fn menu_image_text_button(
    source: &str,
    text: &str,
    update_fun: Bound<'_, PyAny>,
    size: f32,
) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    if egui::containers::menu::is_in_menu(ui) {
        return ui
            .menu_image_text_button(menu_image(source, size), text, |ui| {
                run_nested_update_func(ui, update_fun)
            })
            .inner
            .unwrap_or(Ok(()));
    }
    egui::containers::menu::MenuButton::from_button(egui::Button::image_and_text(
        menu_image(source, size),
        text,
    ))
    .config(crate::style::menu_config())
    .ui(ui, |ui| run_nested_update_func(ui, update_fun))
    .1
    .map(|inner| inner.inner)
    .unwrap_or(Ok(()))
}

/// An on/off switch, the modern look of a checkbox. The track takes the accent color when on.
///
/// Args:
///     value (Bool): the state of the switch
///
///     text (str): a label on the right of the switch
///
/// Example::
///
///     notifications = Bool(True)
///     # inside update_func
///     switch(notifications, "Notifications")
#[pyfunction]
#[pyo3(signature = (value, text=None))]
unsafe fn switch(value: &mut Bool, text: Option<&str>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    ui.horizontal(|ui| {
        let height = ui.spacing().interact_size.y * 0.8;
        let (rect, mut response) =
            ui.allocate_exact_size(egui::vec2(height * 1.75, height), egui::Sense::click());
        if response.clicked() {
            value.value = !value.value;
            response.mark_changed();
        }
        let label = text.unwrap_or("");
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Switch, ui.is_enabled(), value.value, label)
        });

        if ui.is_rect_visible(rect) {
            let on = ui.ctx().animate_bool_responsive(response.id, value.value);
            let visuals = ui.style().interact(&response);
            let off_fill = ui.visuals().widgets.inactive.bg_fill;
            let on_fill = ui.visuals().selection.bg_fill;
            let track = egui::Color32::from_rgba_unmultiplied(
                egui::lerp(off_fill.r() as f32..=on_fill.r() as f32, on) as u8,
                egui::lerp(off_fill.g() as f32..=on_fill.g() as f32, on) as u8,
                egui::lerp(off_fill.b() as f32..=on_fill.b() as f32, on) as u8,
                egui::lerp(off_fill.a() as f32..=on_fill.a() as f32, on) as u8,
            );
            let radius = rect.height() / 2.0;
            ui.painter()
                .rect(rect, radius, track, visuals.bg_stroke, egui::StrokeKind::Inside);
            let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), on);
            let knob = egui::pos2(x, rect.center().y);
            ui.painter().circle(
                knob,
                radius - 2.5,
                egui::Color32::WHITE,
                egui::Stroke::new(0.5, egui::Color32::from_black_alpha(40)),
            );
        }

        if let Some(text) = text {
            if ui.label(text).clicked() {
                value.value = !value.value;
            }
        }
    });
    Ok(())
}

/// A small colored label, for a status or a tag.
///
/// Args:
///     text (str): the text
///
///     color (color): the color of the text, the background is a tint of it. Default is the accent
///
/// Example::
///
///     badge("Done", "#22c55e")
///     badge("Blocked", (239, 68, 68))
#[pyfunction]
#[pyo3(signature = (text, color=None))]
unsafe fn badge(text: &str, color: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let color = match color {
        Some(color) => extract_color(color)?,
        None => crate::style::theme_state()
            .accent
            .unwrap_or(ui.visuals().selection.bg_fill),
    };
    let fill = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 38);
    egui::Frame::new()
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(8, 1))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).color(color).small().strong());
        });
    Ok(())
}

/// What the user did with a widget during the frame, returned by label_response and
/// selectable_label_response
#[pyclass(frozen, get_all)]
pub(crate) struct Response {
    /// clicked with the primary button
    clicked: bool,
    /// double clicked with the primary button
    double_clicked: bool,
    /// clicked with the secondary button, usually the right one
    secondary_clicked: bool,
    /// the mouse is over the widget
    hovered: bool,
}

#[pymethods]
impl Response {
    fn __repr__(&self) -> String {
        format!(
            "Response(clicked={}, double_clicked={}, secondary_clicked={}, hovered={})",
            self.clicked, self.double_clicked, self.secondary_clicked, self.hovered
        )
    }
}

impl From<egui::Response> for Response {
    fn from(response: egui::Response) -> Self {
        Response {
            clicked: response.clicked(),
            double_clicked: response.double_clicked(),
            secondary_clicked: response.secondary_clicked(),
            hovered: response.hovered(),
        }
    }
}

fn sized_text(text: &str, size: Option<f32>) -> egui::RichText {
    let text = egui::RichText::new(text);
    match size {
        Some(size) => text.size(size),
        None => text,
    }
}

/// Shows a text that can be clicked and returns a Response: clicked, double_clicked,
/// secondary_clicked and hovered.
///
/// Args:
///     text (str): the text
///
///     size (float): font size of the text. Default is the size of the body text
///
/// Example::
///
///     response = label_response("📁", size=48)
///     if response.double_clicked:
///         open_folder()
#[pyfunction]
#[pyo3(signature = (text, size=None))]
unsafe fn label_response(text: &str, size: Option<f32>) -> PyResult<Response> {
    let ui = current_ui(&UI)?;
    let response = ui.add(egui::Label::new(sized_text(text, size)).sense(egui::Sense::click()));
    Ok(response.into())
}

/// Shows a text that looks selected when selected is True, and returns a Response: clicked,
/// double_clicked, secondary_clicked and hovered. A double click is also a click.
///
/// Args:
///     selected (bool): show the text as selected
///
///     text (str): the text
///
///     size (float): font size of the text. Default is the size of the body text
///
/// Example::
///
///     response = selectable_label_response(name == selected.value, name)
///     if response.clicked:
///         selected.value = name
///     if response.double_clicked:
///         open_file(name)
#[pyfunction]
#[pyo3(signature = (selected, text, size=None))]
unsafe fn selectable_label_response(selected: bool, text: &str, size: Option<f32>) -> PyResult<Response> {
    let ui = current_ui(&UI)?;
    Ok(ui.selectable_label(selected, sized_text(text, size)).into())
}

/// Parses "Cmd+Shift+ArrowUp" into the modifiers and the key
fn parse_shortcut(shortcut: &str) -> PyResult<(egui::Modifiers, egui::Key)> {
    let mut modifiers = egui::Modifiers::NONE;
    let mut key = None;
    for part in shortcut.split('+').map(str::trim) {
        match part.to_lowercase().as_str() {
            // the command key on macOS, ctrl elsewhere
            "cmd" | "command" => modifiers = modifiers | egui::Modifiers::COMMAND,
            "ctrl" | "control" => modifiers = modifiers | egui::Modifiers::CTRL,
            "shift" => modifiers = modifiers | egui::Modifiers::SHIFT,
            "alt" | "option" => modifiers = modifiers | egui::Modifiers::ALT,
            _ => {
                key = Some(egui::Key::from_name(part).ok_or_else(|| {
                    PyValueError::new_err(format!(
                        "Unknown key '{}' in '{}'. Keys are named like A, 1, Enter, Escape, Space, Tab, Backspace, Delete, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, F1",
                        part, shortcut
                    ))
                })?)
            }
        }
    }
    let key = key.ok_or_else(|| PyValueError::new_err(format!("No key in '{}'", shortcut)))?;
    Ok((modifiers, key))
}

/// True if the key was pressed during the frame with these modifiers. Cmd and Ctrl must match,
/// Shift and Alt pressed in addition are ignored, so "Enter" is also Shift+Enter.
///
/// Args:
///     shortcut (str): a key with optional modifiers: "Enter", "ArrowDown", "Cmd+ArrowUp",
///         "Shift+Tab". Cmd is the command key on macOS and ctrl elsewhere
///
///     when_typing (bool): also when a text field has the focus. Default is False, so the arrows
///         move the cursor of the text field instead of triggering the shortcut
///
/// Example::
///
///     if key_pressed("Enter"):
///         open_selection()
///     if key_pressed("Cmd+ArrowUp"):
///         go_to_parent()
#[pyfunction]
#[pyo3(signature = (shortcut, when_typing=false))]
unsafe fn key_pressed(shortcut: &str, when_typing: bool) -> PyResult<bool> {
    let (modifiers, key) = parse_shortcut(shortcut)?;
    let ctx = current_ui(&UI)?.ctx().clone();
    if !when_typing && ctx.egui_wants_keyboard_input() {
        return Ok(false);
    }
    Ok(ctx.input(|i| i.key_pressed(key) && i.modifiers.matches_logically(modifiers)))
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<RGBA>()?;
    m.add_function(wrap_pyfunction!(colored_label, m)?)?;
    m.add_function(wrap_pyfunction!(selectable_label_clicked, m)?)?;
    m.add_function(wrap_pyfunction!(columns, m)?)?;
    m.add_function(wrap_pyfunction!(color_edit_button_rgba, m)?)?;
    m.add_function(wrap_pyfunction!(drag_angle, m)?)?;
    m.add_function(wrap_pyfunction!(dnd_drag_source, m)?)?;
    m.add_function(wrap_pyfunction!(dnd_drop_zone, m)?)?;
    m.add_function(wrap_pyfunction!(radio_clicked, m)?)?;
    m.add_function(wrap_pyfunction!(available_width, m)?)?;
    m.add_function(wrap_pyfunction!(available_height, m)?)?;
    m.add_function(wrap_pyfunction!(set_width, m)?)?;
    m.add_function(wrap_pyfunction!(set_min_width, m)?)?;
    m.add_function(wrap_pyfunction!(set_max_width, m)?)?;
    m.add_function(wrap_pyfunction!(set_height, m)?)?;
    m.add_function(wrap_pyfunction!(set_min_height, m)?)?;
    m.add_function(wrap_pyfunction!(set_max_height, m)?)?;
    m.add_function(wrap_pyfunction!(scroll_to_cursor, m)?)?;
    m.add_function(wrap_pyfunction!(is_enabled, m)?)?;
    m.add_function(wrap_pyfunction!(is_visible, m)?)?;
    m.add_function(wrap_pyfunction!(ui_contains_pointer, m)?)?;
    m.add_function(wrap_pyfunction!(pixels_per_point, m)?)?;
    m.add_function(wrap_pyfunction!(menu_image_button, m)?)?;
    m.add_function(wrap_pyfunction!(menu_image_text_button, m)?)?;
    m.add_function(wrap_pyfunction!(switch, m)?)?;
    m.add_function(wrap_pyfunction!(badge, m)?)?;
    m.add_class::<Response>()?;
    m.add_function(wrap_pyfunction!(label_response, m)?)?;
    m.add_function(wrap_pyfunction!(selectable_label_response, m)?)?;
    m.add_function(wrap_pyfunction!(key_pressed, m)?)?;
    Ok(())
}
