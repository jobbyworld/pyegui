//! Property set: the property grid of an IDE (the "Properties" window of Visual Studio), see
//! `pyegui_core::property_set`. `PropertySet` is the state (the order, the groups folded, the filter, the row
//! selected), `property_set` draws the rows it is given and returns what the person changed.

use std::ops::{Deref, DerefMut};

use eframe::egui::Id;
use pyegui_core::property_set::{Change, Kind, PropOption, Property, PropertySetOptions, PropertySetState};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use crate::style::extract_color;
use crate::{current_ui, kwarg, UI};

/// The state of a property set: by group or by name, the groups folded, the filter typed and the row
/// selected. Create it once, outside the update function, and draw it with property_set().
///
/// Args:
///     grouped (bool): rows in groups (categories) that fold (True, default) or sorted by name
///
/// Attributes (read and write):
///     grouped (bool): the order of the rows
///
///     filter (str): the text of the filter field
///
///     selected (str | None): the id of the row selected
///
///     split (float): the width of the name column, from 0.2 to 0.8 of the width
///
///     collapsed (list[str]): the groups folded
///
/// Usage::
///
///     props = PropertySet()
///
///     def update_func(ctx):
///         for change in property_set(props, [...]):
///             print(change)
#[pyclass]
pub(crate) struct PropertySet(PropertySetState);

impl Deref for PropertySet {
    type Target = PropertySetState;
    fn deref(&self) -> &PropertySetState {
        &self.0
    }
}

impl DerefMut for PropertySet {
    fn deref_mut(&mut self) -> &mut PropertySetState {
        &mut self.0
    }
}

#[pymethods]
impl PropertySet {
    #[new]
    #[pyo3(signature = (grouped=true))]
    fn new(grouped: bool) -> Self {
        PropertySet(PropertySetState::new(grouped))
    }

    #[getter]
    fn get_grouped(&self) -> bool {
        self.grouped
    }

    #[setter]
    fn set_grouped(&mut self, grouped: bool) {
        self.grouped = grouped;
    }

    #[getter]
    fn get_filter(&self) -> String {
        self.filter.clone()
    }

    #[setter]
    fn set_filter(&mut self, filter: String) {
        self.filter = filter;
    }

    #[getter]
    fn get_selected(&self) -> Option<String> {
        self.selected.clone()
    }

    #[setter]
    fn set_selected(&mut self, selected: Option<String>) {
        self.selected = selected;
    }

    #[getter]
    fn get_split(&self) -> f32 {
        self.split
    }

    #[setter]
    fn set_split(&mut self, split: f32) {
        self.split = split.clamp(0.2, 0.8);
    }

    #[getter]
    fn get_collapsed(&self) -> Vec<String> {
        let mut groups: Vec<String> = self.collapsed.iter().cloned().collect();
        groups.sort();
        groups
    }

    #[setter]
    fn set_collapsed(&mut self, groups: Vec<String>) {
        self.collapsed = groups.into_iter().collect();
    }
}

fn item<'py, T: for<'a> FromPyObject<'a, 'py>>(d: &Bound<'py, PyDict>, key: &str) -> PyResult<Option<T>>
where
    for<'a> <T as FromPyObject<'a, 'py>>::Error: Into<PyErr>,
{
    match d.get_item(key)? {
        Some(v) if !v.is_none() => v.extract::<T>().map(Some).map_err(Into::into),
        _ => Ok(None),
    }
}

fn read_options(d: &Bound<'_, PyDict>) -> PyResult<Vec<PropOption>> {
    let Some(list) = d.get_item("options")?.filter(|v| !v.is_none()) else { return Ok(vec![]) };
    read_option_list(&list)
}

fn read_option_list(list: &Bound<'_, PyAny>) -> PyResult<Vec<PropOption>> {
    let mut out = Vec::new();
    for o in list.try_iter()? {
        let o = o?;
        if let Ok(label) = o.extract::<String>() {
            out.push(PropOption { id: label.clone(), label, color: None });
            continue;
        }
        let o = o.cast::<PyDict>().map_err(|_| PyValueError::new_err("an option is a str or a dict with id, label and color"))?;
        let label = item::<String>(o, "label")?.unwrap_or_default();
        let id = match o.get_item("id")?.filter(|v| !v.is_none()) {
            Some(v) => v.str()?.to_string(),
            None => label.clone(),
        };
        let color = match o.get_item("color")?.filter(|v| !v.is_none()) {
            Some(c) if c.extract::<String>().is_ok_and(|s| s.is_empty()) => None,
            Some(c) => Some(extract_color(&c)?),
            None => None,
        };
        out.push(PropOption { id, label, color });
    }
    Ok(out)
}

fn read_property(d: &Bound<'_, PyDict>) -> PyResult<Property> {
    let id = match d.get_item("id")?.filter(|v| !v.is_none()) {
        Some(v) => v.str()?.to_string(),
        None => item::<String>(d, "name")?.ok_or_else(|| PyValueError::new_err("a property needs an id or a name"))?,
    };
    let name = item::<String>(d, "name")?.unwrap_or_else(|| id.clone());
    let kind = match item::<String>(d, "type")?.unwrap_or_else(|| "text".into()).as_str() {
        "text" | "str" => Kind::Text,
        "longtext" | "multiline" => Kind::LongText,
        "number" | "int" | "float" => Kind::Number,
        "bool" | "checkbox" => Kind::Bool,
        "date" => Kind::Date,
        "choice" | "list" => Kind::Choice {
            options: read_options(d)?,
            multiple: item::<bool>(d, "multiple")?.unwrap_or(false),
            open: item::<bool>(d, "open")?.unwrap_or(false),
        },
        "readonly" | "label" => Kind::Readonly,
        other => return Err(PyValueError::new_err(format!("unknown property type {other:?}: text, longtext, number, bool, date, choice or readonly"))),
    };
    let mut p = Property::new(&id, &name, kind);
    p.group = item::<String>(d, "group")?.unwrap_or_default();
    p.description = item::<String>(d, "description")?.unwrap_or_default();
    p.note = item::<String>(d, "note")?.unwrap_or_default();
    p.hint = item::<String>(d, "hint")?.unwrap_or_default();
    p.unit = item::<String>(d, "unit")?.unwrap_or_default();
    p.readonly = item::<bool>(d, "readonly")?.unwrap_or(false);
    if let Some(v) = d.get_item("value")?.filter(|v| !v.is_none()) {
        if let Ok(b) = v.extract::<bool>() {
            // a bool before an int: True is 1
            p.value = if b { "1" } else { "0" }.to_owned();
        } else if let Ok(list) = v.cast::<PyList>() {
            p.selected = list.iter().map(|x| x.str().map(|s| s.to_string())).collect::<PyResult<_>>()?;
        } else if let Ok(t) = v.cast::<PyTuple>() {
            p.selected = t.iter().map(|x| x.str().map(|s| s.to_string())).collect::<PyResult<_>>()?;
        } else if matches!(p.kind, Kind::Choice { .. }) {
            p.selected = vec![v.str()?.to_string()];
        } else {
            p.value = v.str()?.to_string();
        }
    }
    Ok(p)
}

/// Draws a property set and returns what the person changed.
///
/// A property set is the property grid of an IDE (the "Properties" window of Visual Studio): a table of two
/// columns, the name and an editor of the value. The rows are kept in groups (categories) that fold when their
/// header is clicked, or sorted by name; a filter field looks in the names, groups, descriptions and values; the
/// splitter between the columns can be dragged; the description of the row selected is shown at the bottom.
///
/// The widget keeps no value. Give it the current properties at each frame, apply the changes it returns,
/// and give it the new values at the next frame. A change the caller does not apply (a text that is not a
/// number) is undone by itself when the field loses the focus.
///
/// Args:
///     state (PropertySet): the state (create it once, outside the update function)
///
///     properties (list[dict]): the rows, in order. A row is a dict with
///
///         - ``id`` (str): what a change names (default: ``name``)
///         - ``name`` (str): the text of the name column
///         - ``type`` (str): ``"text"`` (default), ``"longtext"``, ``"number"``, ``"bool"``, ``"date"``,
///           ``"choice"`` or ``"readonly"``
///         - ``value``: a str for a text, number or date (what is typed), a bool for a bool, the id (or a list
///           of ids) of the options chosen for a choice; None for no value
///         - ``group`` (str): its group (category)
///         - ``description`` (str): shown at the bottom when the row is selected, and when the mouse is over its name
///         - ``options`` (list): for a choice, ``str`` or ``{"id", "label", "color"}``
///         - ``multiple`` (bool), ``open`` (bool): a choice that takes several options, and one the person can add
///           options to
///         - ``note`` (str): what the row has without a value of its own (``"inherited from X"``), shown weak
///           where the value would be
///         - ``hint`` (str): shown in an empty field
///         - ``unit`` (str): after a number
///         - ``readonly`` (bool): shown, not editable
///
/// Keyword args:
///     key (str): what the properties are about (a path, a name): when it changes, the texts being typed and
///         the selection are dropped (default "")
///
///     id (str): tells two property sets with one state apart
///
///     toolbar (bool): the order buttons and the filter field (default True)
///
///     description (bool): the description of the row selected at the bottom (default True)
///
///     row_height (float): default 26
///
///     default_group (str): the group of the rows that have none (default "Misc")
///
///     grouped_label, alphabetical_label, filter_hint, clear_tip, add_hint, empty_text (str): the texts of the
///         widget, to translate it
///
/// Returns:
///     list[tuple]: the changes of this frame, each ``("set", id, text)`` (a text, or "1" / "0" for a bool),
///     ``("clear", id)``, ``("choose", id, [option ids])`` or ``("add_option", id, label)`` (a new option of an
///     open list, to add and choose)
///
/// Example::
///
///     values = {"name": "Report", "done": False}
///     props = PropertySet()
///
///     def update_func(ctx):
///         rows = [
///             dict(id="name", name="Name", group="General", value=values["name"], description="The title"),
///             dict(id="done", name="Done", type="bool", group="General", value=values["done"]),
///             dict(id="size", name="Size", type="number", group="Layout", unit="cm", value="21"),
///         ]
///         for change in property_set(props, rows):
///             if change[0] == "set" and change[1] == "name":
///                 values["name"] = change[2]
///             elif change[0] == "set" and change[1] == "done":
///                 values["done"] = change[2] == "1"
#[pyfunction]
#[pyo3(signature = (state, properties, **kwargs))]
unsafe fn property_set(py: Python<'_>, state: &Bound<'_, PropertySet>, properties: &Bound<'_, PyAny>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyList>> {
    let ui = current_ui(&UI)?;
    let mut props = Vec::new();
    for row in properties.try_iter()? {
        let row = row?;
        let row = row.cast::<PyDict>().map_err(|_| PyValueError::new_err("a property is a dict"))?;
        props.push(read_property(row)?);
    }
    let id = match kwarg::<String>(kwargs, "id")? {
        Some(id) => Id::unique(("pyegui-property-set", id)),
        None => Id::unique(("pyegui-property-set", state.as_ptr() as usize)),
    };
    let mut o = PropertySetOptions::new(id);
    o.key = kwarg::<String>(kwargs, "key")?.unwrap_or_default();
    o.toolbar = kwarg::<bool>(kwargs, "toolbar")?.unwrap_or(true);
    o.description = kwarg::<bool>(kwargs, "description")?.unwrap_or(true);
    o.row_height = kwarg::<f32>(kwargs, "row_height")?.unwrap_or(26.0);
    macro_rules! text {
        ($field:ident) => {
            if let Some(t) = kwarg::<String>(kwargs, stringify!($field))? {
                o.$field = t;
            }
        };
    }
    text!(default_group);
    text!(grouped_label);
    text!(alphabetical_label);
    text!(filter_hint);
    text!(clear_tip);
    text!(add_hint);
    text!(empty_text);
    let changes = {
        let mut state = state.borrow_mut();
        pyegui_core::property_set::property_set(ui, &mut state.0, &props, &o)
    };
    let out = PyList::empty(py);
    for change in changes {
        match change {
            Change::Set { id, value } => out.append(("set", id, value))?,
            Change::Clear { id } => out.append(("clear", id))?,
            Change::Choose { id, ids } => out.append(("choose", id, ids))?,
            Change::AddOption { id, label } => out.append(("add_option", id, label))?,
        }
    }
    Ok(out.unbind())
}

/// The editor of a list in a cell or anywhere else: the one of the rows of `property_set` (a combo box with the
/// labels chosen, which opens a list of the options, a check box for each one when `multiple`), alone.
///
/// Args:
///     options (list): the options, each a str or a dict with `id`, `label` and `color`
///     selected (list[str]): the ids of the options chosen
///
/// Keyword args:
///     multiple (bool): a check box per option, several can be chosen. False by default
///
///     open (bool): a field adds a new option (and chooses it). False by default
///
///     id (str): tells apart two editors that have the same options (give each cell its own)
///
///     width (float): width of the combo box. Default is the room that is left
///
///     add_hint (str): the hint of the field of a new option. Default is "New value"
///
///     muted (bool): the values are drawn faded (a summary of the folder, an inherited value). False by default
///
///     enabled (bool): False to show it without editing. True by default
///
///     flat (bool): no outline and no fill, the arrow shows while the mouse is over it (for a cell). False by default
///
/// Returns:
///     tuple | None: None while nothing changed, else ("choose", ids), ("clear",) or ("add_option", label)
///
/// Example::
///
///     change = choice_edit(options, chosen, multiple=True, id=f"status {path}")
///     if change and change[0] == "choose":
///         chosen = change[1]
#[pyfunction]
#[pyo3(signature = (options, selected, **kwargs))]
unsafe fn choice_edit(py: Python<'_>, options: &Bound<'_, PyAny>, selected: Vec<String>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Option<Py<PyTuple>>> {
    use pyegui_core::property_set::{choice_combo, ChoiceChange, ChoiceSpec};
    let ui = current_ui(&UI)?;
    let options = read_option_list(options)?;
    let salt = Id::unique(("pyegui-choice-edit", kwarg::<String>(kwargs, "id")?.unwrap_or_default()));
    let typed_id = salt.with("typed");
    let mut typed: String = ui.data(|d| d.get_temp(typed_id)).unwrap_or_default();
    let add_hint = kwarg::<String>(kwargs, "add_hint")?.unwrap_or_else(|| "New value".into());
    let spec = ChoiceSpec {
        salt,
        options: &options,
        selected: &selected,
        multiple: kwarg::<bool>(kwargs, "multiple")?.unwrap_or(false),
        open: kwarg::<bool>(kwargs, "open")?.unwrap_or(false),
        editable: kwarg::<bool>(kwargs, "enabled")?.unwrap_or(true),
        width: kwarg::<f32>(kwargs, "width")?.unwrap_or_else(|| ui.available_width()),
        add_hint: &add_hint,
        note: None,
        flat: kwarg::<bool>(kwargs, "flat")?.unwrap_or(false),
        muted: kwarg::<bool>(kwargs, "muted")?.unwrap_or(false),
    };
    let change = choice_combo(ui, &spec, &mut typed);
    ui.data_mut(|d| d.insert_temp(typed_id, typed));
    Ok(match change {
        Some(ChoiceChange::Choose(ids)) => Some(PyTuple::new(py, [("choose").into_pyobject(py)?.into_any(), ids.into_pyobject(py)?.into_any()])?.unbind()),
        Some(ChoiceChange::Clear) => Some(PyTuple::new(py, ["clear"])?.unbind()),
        Some(ChoiceChange::AddOption(label)) => Some(PyTuple::new(py, ["add_option", label.as_str()])?.unbind()),
        None => None,
    })
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PropertySet>()?;
    m.add_function(wrap_pyfunction!(property_set, m)?)?;
    m.add_function(wrap_pyfunction!(choice_edit, m)?)?;
    Ok(())
}
