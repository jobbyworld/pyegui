use eframe::egui;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyAny;

use crate::style::extract_color;
use crate::{current_ui, run_nested_call, run_nested_update_func, Float, UI};

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

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<RGBA>()?;
    m.add_function(wrap_pyfunction!(colored_label, m)?)?;
    m.add_function(wrap_pyfunction!(selectable_label_clicked, m)?)?;
    m.add_function(wrap_pyfunction!(columns, m)?)?;
    m.add_function(wrap_pyfunction!(color_edit_button_rgba, m)?)?;
    m.add_function(wrap_pyfunction!(drag_angle, m)?)?;
    m.add_function(wrap_pyfunction!(dnd_drag_source, m)?)?;
    m.add_function(wrap_pyfunction!(dnd_drop_zone, m)?)?;
    Ok(())
}
