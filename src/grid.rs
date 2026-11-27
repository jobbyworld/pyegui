use std::collections::HashMap;

use eframe::egui;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList};

use crate::columns::{parse_type, DataType};
use crate::{current_ui, kwarg, run_nested_call, UI};
use pyegui_core::grid::{show, GridColumn, GridOptions, GridOutput, GridRow, GridState};

/// A grid of data: rows of cells in columns, flat or as a tree, that is not tied to files. The columns sort, filter,
/// move, resize, hide and pin like the ones of the file views, the rows are selected with the mouse and the keyboard,
/// a cell can be edited in place, a branch opens and closes. It keeps no data: give the rows at each frame, and apply
/// what it tells (`take_edited`, `take_toggled`...). A state object: create it once, outside the update function, and
/// draw it with `data_grid`.
///
/// Args:
///     columns (list[dict]): the columns, each a dict with `key` (what the state and the events name it by), `title`
///         (default the key), `type` ("text", "number", "date" or "choice", default "text"), `width` (default 120)
///         `editable` (a double click on a cell, or F2, edits it) and `custom` (the cells are drawn by the `cell`
///         function of `data_grid`, with widgets)
///     id (str): tells apart two grids of a ui. Default is "grid"
///
/// Example::
///
///     grid = DataGrid([dict(key="name", title="Name", editable=True), dict(key="age", title="Age", type="number")])
///     people = {"1": ["Alice", "32"], "2": ["Bob", "27"]}
///
///     def update_func(ctx):
///         rows = [dict(id=i, cells=cells) for i, cells in people.items()]
///         data_grid(grid, rows)
///         edit = grid.take_edited()  # (row id, column key, new text)
///         if edit:
///             row, column, text = edit
///             people[row][0 if column == "name" else 1] = text
#[pyclass(unsendable, module = "pyegui")]
pub struct DataGrid {
    columns: Vec<GridColumn>,
    state: GridState,
    id: String,
    clicked: Option<String>,
    activated: Vec<String>,
    edited: Option<(String, String, String)>,
    toggled: Option<(String, bool)>,
    drag_started: Option<Vec<String>>,
    hovered: Option<String>,
    shown: Vec<String>,
}

fn columns_from_py(columns: &Bound<'_, PyList>) -> PyResult<Vec<GridColumn>> {
    let mut out = vec![];
    for item in columns.iter() {
        let d = item.cast::<PyDict>().map_err(|_| PyValueError::new_err("a column is a dict with a key"))?;
        let key: String = d.get_item("key")?.ok_or_else(|| PyValueError::new_err("a column needs a key"))?.extract()?;
        let title: String = d.get_item("title")?.map(|v| v.extract()).transpose()?.unwrap_or_else(|| key.clone());
        let mut column = GridColumn::new(&key, &title);
        if let Some(t) = d.get_item("type")? {
            column.ty = parse_type(&t.extract::<String>()?)?;
        }
        if let Some(w) = d.get_item("width")? {
            column.width = w.extract()?;
        }
        if let Some(e) = d.get_item("editable")? {
            column.editable = e.extract()?;
        }
        if let Some(e) = d.get_item("custom")? {
            column.custom = e.extract()?;
        }
        if out.iter().any(|c: &GridColumn| c.key == key) {
            return Err(PyValueError::new_err(format!("two columns have the key '{key}'")));
        }
        out.push(column);
    }
    Ok(out)
}

fn rows_from_py(rows: &Bound<'_, PyAny>, columns: &[GridColumn]) -> PyResult<Vec<GridRow>> {
    let mut out = vec![];
    for item in rows.try_iter()? {
        let item = item?;
        let d = item.cast::<PyDict>().map_err(|_| PyValueError::new_err("a row is a dict with an id and its cells"))?;
        let id: String = match d.get_item("id")? {
            Some(v) => v.str()?.to_string(),
            None => return Err(PyValueError::new_err("a row needs an id")),
        };
        let cells = match d.get_item("cells")? {
            None => vec![String::new(); columns.len()],
            Some(c) => {
                if let Ok(by_key) = c.cast::<PyDict>() {
                    columns
                        .iter()
                        .map(|col| match by_key.get_item(&col.key) {
                            Ok(Some(v)) if !v.is_none() => v.str().map(|s| s.to_string()),
                            _ => Ok(String::new()),
                        })
                        .collect::<PyResult<Vec<String>>>()?
                } else {
                    let mut texts = vec![];
                    for v in c.try_iter()? {
                        let v = v?;
                        texts.push(if v.is_none() { String::new() } else { v.str()?.to_string() });
                    }
                    texts
                }
            }
        };
        let parent = d.get_item("parent")?.filter(|v| !v.is_none()).map(|v| v.str().map(|s| s.to_string())).transpose()?;
        let readonly = d.get_item("readonly")?.map(|v| v.extract()).transpose()?.unwrap_or(false);
        let icon = d.get_item("icon")?.filter(|v| !v.is_none()).map(|v| v.extract::<String>()).transpose()?;
        out.push(GridRow { id, cells, parent, readonly, icon, ..Default::default() });
    }
    Ok(out)
}

#[pymethods]
impl DataGrid {
    #[new]
    #[pyo3(signature = (columns, id="grid"))]
    fn new(columns: &Bound<'_, PyList>, id: &str) -> PyResult<Self> {
        let columns = columns_from_py(columns)?;
        let sort = columns.first().map(|c| c.key.clone()).unwrap_or_default();
        Ok(DataGrid { columns, state: GridState::new(&sort), id: id.into(), clicked: None, activated: vec![], edited: None, toggled: None, drag_started: None, hovered: None, shown: vec![] })
    }

    /// The ids of the rows selected, in the order of the clicks (read and write)
    #[getter]
    fn selected(&self) -> Vec<String> {
        self.state.selected.clone()
    }

    #[setter]
    fn set_selected(&mut self, ids: Vec<String>) {
        self.state.cursor = ids.last().cloned();
        self.state.anchor = ids.last().cloned();
        self.state.selected = ids;
    }

    /// The id of the row the keyboard is on, or None
    #[getter]
    fn cursor(&self) -> Option<String> {
        self.state.cursor.clone()
    }

    /// The ids of the rows shown during the last frame, in order: the rows the filters keep, the branches open
    #[getter]
    fn shown(&self) -> Vec<String> {
        self.shown.clone()
    }

    /// The id of the row the pointer was over during the last frame, or None
    #[getter]
    fn hovered(&self) -> Option<String> {
        self.hovered.clone()
    }

    /// The ids of the branches of the tree that are open (read and write)
    #[getter]
    fn expanded(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.state.expanded.iter().cloned().collect();
        ids.sort();
        ids
    }

    #[setter]
    fn set_expanded(&mut self, ids: Vec<String>) {
        self.state.expanded = ids.into_iter().collect();
    }

    /// Opens a branch of the tree
    fn expand(&mut self, id: String) {
        self.state.expanded.insert(id);
    }

    /// Closes a branch of the tree
    fn collapse(&mut self, id: &str) {
        self.state.expanded.remove(id);
    }

    /// Scrolls to a row at the next frame (it must be shown)
    fn scroll_to(&mut self, id: String) {
        self.state.scroll_to = Some(id);
    }

    /// Starts editing a cell, as a double click does. Nothing happens if the column is not editable
    fn start_edit(&mut self, row: String, column: &str) {
        if self.columns.iter().any(|c| c.key == column && c.editable) {
            self.state.editing = Some(pyegui_core::grid::Edit::new(row, column.into(), String::new()));
        }
    }

    /// The key of the column the grid is sorted by, and the direction (read and write)
    #[getter]
    fn sort_by(&self) -> String {
        self.state.columns.sort_key.clone()
    }

    #[setter]
    fn set_sort_by(&mut self, key: String) {
        self.state.columns.sort_key = key;
    }

    #[getter]
    fn ascending(&self) -> bool {
        self.state.columns.ascending
    }

    #[setter]
    fn set_ascending(&mut self, ascending: bool) {
        self.state.columns.ascending = ascending;
    }

    /// The keys of the columns in the order the user left them (read and write, to keep between runs)
    #[getter]
    fn column_order(&self) -> Vec<String> {
        self.state.columns.order.clone()
    }

    #[setter]
    fn set_column_order(&mut self, order: Vec<String>) {
        self.state.columns.order = order;
    }

    /// The widths the user gave to the columns, by key (read and write)
    #[getter]
    fn column_widths<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        for (k, w) in &self.state.columns.widths {
            d.set_item(k, *w)?;
        }
        Ok(d)
    }

    #[setter]
    fn set_column_widths(&mut self, widths: HashMap<String, f32>) {
        self.state.columns.set_widths(widths);
    }

    /// The keys of the columns the user hid with the eye of the settings (read and write)
    #[getter]
    fn hidden_columns(&self) -> Vec<String> {
        self.state.columns.hidden.clone()
    }

    #[setter]
    fn set_hidden_columns(&mut self, columns: Vec<String>) {
        self.state.columns.hidden = columns;
    }

    /// The id of the row clicked since the last call, or None
    fn take_clicked(&mut self) -> Option<String> {
        self.clicked.take()
    }

    /// The ids of the rows opened (double click, Enter) since the last call
    fn take_activated(&mut self) -> Vec<String> {
        std::mem::take(&mut self.activated)
    }

    /// (row id, column key, new text) of the cell edited since the last call, or None. Apply it to your data: the
    /// grid shows what it is given
    fn take_edited(&mut self) -> Option<(String, String, String)> {
        self.edited.take()
    }

    /// (row id, opened) of the branch opened or closed since the last call, or None
    fn take_toggled(&mut self) -> Option<(String, bool)> {
        self.toggled.take()
    }

    /// The ids of the rows a drag started from (the selection) since the last call, or None. Needs `drag=True`
    fn take_drag_started(&mut self) -> Option<Vec<String>> {
        self.drag_started.take()
    }
}

/// Draws a `DataGrid`: the rows as a list with columns, or as a tree. It takes the room left in the ui and scrolls;
/// only the visible rows are drawn.
///
/// Args:
///     grid (DataGrid): the state
///     rows (list[dict]): the rows, each a dict with `id` (unique), `cells` (the texts, in the order of the columns,
///         or a dict by column key), `parent` (the id of the parent row, for a tree), `readonly` and `icon` (an
///         image uri drawn before the text of the first column, e.g. "file://..." or "bytes://...")
///
/// Keyword args:
///     tree (bool): the rows are a tree (`parent`), with a chevron and an indentation in the first column. Default is False
///     multi_select (bool): Cmd and Shift extend the selection. Default is True
///     striped (bool): every other row has a different background. Default is True
///     sortable, resizable, pinnable, reorderable, filterable (bool): what a title lets the user do. Default is True
///     settings (bool): the icon of the settings at the right of the header, to order, hide and pin the columns
///     max_height (float): the most the grid takes in height before it scrolls
///     row_height (float): the height of a row. Default is the one of a button
///     indent (float): the indentation of a level of the tree. Default is 16
///     drag (bool): tell the start of a drag of the selection (`take_drag_started`). Default is False
///     row_lines (bool): a thin horizontal line under each row. Default is True
///     context_menu (callable): called with the list of ids concerned when a row is right-clicked, inside a menu
///     cell (callable): called with the row id and the column key for each cell of a column with `custom=True`, in
///         a ui of the size of the cell: draw a widget there (a chip, a switch, a button)
///
/// An empty area under the rows clears the selection on a click, and a drag there draws a lasso that selects the
/// rows it touches (Cmd or Shift keeps the selection).
///
/// Example::
///
///     data_grid(grid, rows, tree=True, settings=True, context_menu=lambda ids: button_clicked(f"{len(ids)} rows"))
#[pyfunction]
#[pyo3(signature = (grid, rows, **kwargs))]
unsafe fn data_grid(py: Python<'_>, grid: Py<DataGrid>, rows: &Bound<'_, PyAny>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let (columns, id) = {
        let g = grid.borrow(py);
        (g.columns.clone(), g.id.clone())
    };
    let rows = rows_from_py(rows, &columns)?;
    let mut options = GridOptions { id, ..Default::default() };
    options.tree = kwarg(kwargs, "tree")?.unwrap_or(false);
    options.multi_select = kwarg(kwargs, "multi_select")?.unwrap_or(true);
    options.striped = kwarg(kwargs, "striped")?.unwrap_or(true);
    options.sortable = kwarg(kwargs, "sortable")?.unwrap_or(true);
    options.header.resizable = kwarg(kwargs, "resizable")?.unwrap_or(true);
    options.header.pinnable = kwarg(kwargs, "pinnable")?.unwrap_or(true);
    options.header.reorderable = kwarg(kwargs, "reorderable")?.unwrap_or(true);
    options.header.filterable = kwarg(kwargs, "filterable")?.unwrap_or(true);
    options.header.settings = kwarg(kwargs, "settings")?.unwrap_or(false);
    options.max_height = kwarg(kwargs, "max_height")?;
    options.row_height = kwarg(kwargs, "row_height")?;
    options.indent = kwarg(kwargs, "indent")?.unwrap_or(16.0);
    options.drag = kwarg(kwargs, "drag")?.unwrap_or(false);
    options.row_lines = kwarg(kwargs, "row_lines")?.unwrap_or(true);
    let menu: Option<Bound<'_, PyAny>> = kwarg(kwargs, "context_menu")?;
    let cell_fn: Option<Bound<'_, PyAny>> = kwarg(kwargs, "cell")?;

    // the state is not borrowed while the grid is drawn: the menu may read the grid
    let mut state = std::mem::take(&mut grid.borrow_mut(py).state);
    let ui = current_ui(&UI)?;
    let error: std::cell::RefCell<Option<PyErr>> = std::cell::RefCell::new(None);
    let out: GridOutput = show(
        ui,
        &mut state,
        &columns,
        &rows,
        &options,
        &mut |ui: &mut egui::Ui, row: &GridRow, col: &GridColumn, rect: egui::Rect| {
            let (Some(f), true) = (&cell_fn, error.borrow().is_none()) else { return false };
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(6.0, 1.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
            child.shrink_clip_rect(rect);
            if let Err(e) = run_nested_call(&mut child, f, (row.id.clone(), col.key.clone())) {
                *error.borrow_mut() = Some(e);
            }
            true
        },
        &mut |ui: &mut egui::Ui, ids: &[String]| {
            if let (Some(f), true) = (&menu, error.borrow().is_none()) {
                if let Err(e) = run_nested_call(ui, f, (ids.to_vec(),)) {
                    *error.borrow_mut() = Some(e);
                }
            }
        },
    );
    let mut g = grid.borrow_mut(py);
    g.state = state;
    if out.clicked.is_some() {
        g.clicked = out.clicked;
    }
    if !out.activated.is_empty() {
        g.activated = out.activated;
    }
    if out.edited.is_some() {
        g.edited = out.edited;
    }
    if out.toggled.is_some() {
        g.toggled = out.toggled;
    }
    if out.drag_started.is_some() {
        g.drag_started = out.drag_started;
    }
    g.hovered = out.hovered;
    g.shown = out.shown;
    match error.into_inner() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<DataGrid>()?;
    m.add_function(wrap_pyfunction!(data_grid, m)?)?;
    let _ = DataType::Text;
    Ok(())
}
