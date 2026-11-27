//! Settings window: the frame shared by the configuration windows, see `pyegui_core::settings`.
//! `SettingsWindow` is its state (the width of the list), `settings_window` draws it and returns what happened.

use std::ops::{Deref, DerefMut};

use eframe::egui::Id;
use pyegui_core::settings::{SettingsEvent, SettingsItem, SettingsOptions, SettingsState};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::style::{apply_frame_kwargs, has_frame_kwargs};
use crate::{kwarg, panel_parent_ui, run_nested_update_func, UI};

/// The state of a settings window: the width of its list, that the person can change with the splitter.
/// Create it once, outside the update function, and draw it with settings_window().
///
/// Attributes (read and write):
///     list_width (float): the width of the list at the left, from 150 points
///
/// Usage::
///
///     window_state = SettingsWindow()
#[pyclass]
pub(crate) struct SettingsWindow(SettingsState);

impl Deref for SettingsWindow {
    type Target = SettingsState;
    fn deref(&self) -> &SettingsState {
        &self.0
    }
}

impl DerefMut for SettingsWindow {
    fn deref_mut(&mut self) -> &mut SettingsState {
        &mut self.0
    }
}

#[pymethods]
impl SettingsWindow {
    #[new]
    fn new() -> Self {
        SettingsWindow(SettingsState::default())
    }

    #[getter]
    fn get_list_width(&self) -> f32 {
        self.list_width
    }

    #[setter]
    fn set_list_width(&mut self, width: f32) {
        self.list_width = width;
    }
}

fn read_item(item: &Bound<'_, PyAny>) -> PyResult<SettingsItem> {
    let dict = item.cast::<PyDict>().map_err(|_| PyValueError::new_err("an item is a dict with `id` and `label`"))?;
    let text = |key: &str| -> PyResult<String> { Ok(dict.get_item(key)?.map(|v| v.extract::<String>()).transpose()?.unwrap_or_default()) };
    Ok(SettingsItem { id: text("id")?, label: text("label")?, hint: text("hint")? })
}

/// A configuration window with the same frame everywhere: a title with its close cross, a list at the left
/// (rows that can be moved by drag and drop, a splitter to resize it, a button at its foot to add one), the
/// panel of the item chosen at the right, and a foot with the buttons of the window (the close button is
/// always at the right). It is a modal window: draw it only while it must be open.
///
/// It returns what happened, to be applied by the caller: `("select", id)`, `("move", from, to)` (the item at
/// index `from` goes to index `to`: reorder the items), `("add",)` (the panel must show a new item: pass
/// `adding=True` until it is created or given up) and `("close",)` (the cross, the close button or Escape:
/// stop drawing the window).
///
/// Args:
///     state (SettingsWindow): the state, created once
///
///     items (list[dict]): the rows of the list, each a dict with `id`, `label` and an optional `hint` (a small
///         text at the right of the row)
///
///     detail (callable): draws the panel at the right: the item chosen, or the form of a new one
///
/// Keyword args:
///     selected (str): the id of the row chosen
///
///     footer (callable): draws the left of the foot (a note, a secondary button)
///
///     title (str), subtitle (str): the head of the window
///
///     list_title (str): the small heading over the list
///
///     add_label (str): the text of the button at the foot of the list, "Nouveau" by default
///
///     close_label (str): the text of the close button, "Fermer" by default
///
///     empty_text (str): shown when the list has no row
///
///     width (float), height (float): the size of the window (720) and of the list and the panel (400)
///
///     reorderable (bool): the rows can be moved by drag and drop. True by default
///
///     adding (bool): the panel is the one of a new item, the add button is lit. False by default
///
///     id (str): tells apart two windows
///
///     fill, stroke, rounding, inner_margin, shadow: look of the window, see the frame function
///
/// Example::
///
///     win = SettingsWindow()
///     names = ["Statut", "Client"]
///     chosen = Str("Statut")
///     adding = Bool(False)
///
///     def detail():
///         label(chosen.value)
///
///     def update_func(ctx):
///         if open_window.value:
///             items = [dict(id=n, label=n, hint="texte") for n in names]
///             for event in settings_window(win, items, detail, selected=chosen.value, title="Propriétés",
///                                          add_label="Nouvelle propriété", adding=adding.value):
///                 if event[0] == "select":
///                     chosen.value = event[1]
///                 elif event[0] == "move":
///                     names.insert(event[2], names.pop(event[1]))
///                 elif event[0] == "close":
///                     open_window.value = False
#[pyfunction]
#[pyo3(signature = (state, items, detail, **kwargs))]
unsafe fn settings_window(
    py: Python<'_>,
    state: &Bound<'_, SettingsWindow>,
    items: &Bound<'_, PyAny>,
    detail: Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyList>> {
    let ctx = panel_parent_ui(&UI)?.ctx().clone();
    let mut rows = Vec::new();
    for item in items.try_iter()? {
        rows.push(read_item(&item?)?);
    }
    let id = match kwarg::<String>(kwargs, "id")? {
        Some(id) => Id::unique(("pyegui-settings-window", id)),
        None => Id::unique(("pyegui-settings-window", state.as_ptr() as usize)),
    };
    let mut o = SettingsOptions::new(id);
    macro_rules! text {
        ($field:ident) => {
            if let Some(t) = kwarg::<String>(kwargs, stringify!($field))? {
                o.$field = t;
            }
        };
    }
    text!(title);
    text!(subtitle);
    text!(list_title);
    text!(add_label);
    text!(close_label);
    text!(empty_text);
    if let Some(w) = kwarg::<f32>(kwargs, "width")? {
        o.width = w;
    }
    if let Some(h) = kwarg::<f32>(kwargs, "height")? {
        o.height = h;
    }
    o.reorderable = kwarg::<bool>(kwargs, "reorderable")?.unwrap_or(true);
    o.adding = kwarg::<bool>(kwargs, "adding")?.unwrap_or(false);
    if has_frame_kwargs(kwargs)? {
        o.frame = Some(apply_frame_kwargs(eframe::egui::Frame::popup(&ctx.global_style()), kwargs)?);
    }
    let selected = kwarg::<String>(kwargs, "selected")?;
    let footer: Option<Bound<'_, PyAny>> = match kwargs {
        Some(k) => k.get_item("footer")?.filter(|f| !f.is_none()),
        None => None,
    };
    let events = {
        let mut state = state.borrow_mut();
        pyegui_core::settings::settings_window(
            &ctx,
            &mut state.0,
            &o,
            &rows,
            selected.as_deref(),
            |ui| {
                let _ = run_nested_update_func(ui, detail);
            },
            |ui| {
                if let Some(footer) = footer {
                    let _ = run_nested_update_func(ui, footer);
                }
            },
        )
    };
    let out = PyList::empty(py);
    for event in events {
        match event {
            SettingsEvent::Select(id) => out.append(("select", id))?,
            SettingsEvent::Move { from, to } => out.append(("move", from, to))?,
            SettingsEvent::Add => out.append(("add",))?,
            SettingsEvent::Close => out.append(("close",))?,
        }
    }
    Ok(out.unbind())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SettingsWindow>()?;
    m.add_function(wrap_pyfunction!(settings_window, m)?)?;
    Ok(())
}
