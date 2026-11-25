//! `TableView`: the state of a `table` that sorts, filters, reorders, resizes and pins its columns.
//! The table is drawn here (not with `egui_extras`) because pinned columns need three zones: the
//! columns pinned at the left, the columns that scroll, and the columns pinned at the right.

use std::collections::HashMap;

use eframe::egui;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use pyegui_core::table::{TableOptions, DEFAULT_WIDTH};

use crate::columns::{ColumnsState, DataType, WIDTH_RANGE};
use crate::{kwarg, Int};

/// The state of a table: sort, filters, widths, order and pinned columns. Create it once, outside
/// the update function, and give it to `table` with `view=`.
///
/// The columns are named by their header, so the headers of a table should be different.
///
/// Args:
///     sort_by (str): header of the column to sort by. Default is "", the order of the rows
///
/// Properties:
///     sort_by (str), ascending (bool), column_widths (dict[str, float]), column_order (list[str]),
///     column_filters (dict), pinned_left (list[str]), pinned_right (list[str]) are read and
///     written, to save at the end of the run and give back at the next one.
///     visible_rows (list[int]) is read only: the indexes in `rows` of the rows shown, in order.
///
/// Example::
///
///     view = TableView(sort_by="Age")
///     view.pinned_left = ["Name"]
///
///     def update_func(ctx):
///         table(["Name", "Age", "City"], rows, view=view, types=["text", "number", "choice"])
#[pyclass]
pub struct TableView {
    columns: ColumnsState,
    shown: Vec<usize>,
}

#[pymethods]
impl TableView {
    #[new]
    #[pyo3(signature = (sort_by=""))]
    fn new(sort_by: &str) -> Self {
        TableView {
            columns: ColumnsState::new(sort_by),
            shown: vec![],
        }
    }

    #[getter]
    fn get_sort_by(&self) -> String {
        self.columns.sort_key.clone()
    }

    #[setter]
    fn set_sort_by(&mut self, sort_by: String) {
        self.columns.sort_key = sort_by;
    }

    #[getter]
    fn get_ascending(&self) -> bool {
        self.columns.ascending
    }

    #[setter]
    fn set_ascending(&mut self, ascending: bool) {
        self.columns.ascending = ascending;
    }

    #[getter]
    fn get_column_widths<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        crate::columns::widths_py(&self.columns, py)
    }

    #[setter]
    fn set_column_widths(&mut self, widths: HashMap<String, f32>) {
        self.columns.set_widths(widths);
    }

    #[getter]
    fn get_column_order(&self) -> Vec<String> {
        self.columns.order.clone()
    }

    #[setter]
    fn set_column_order(&mut self, order: Vec<String>) {
        self.columns.order = order;
    }

    #[getter]
    fn get_column_filters<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        crate::columns::filters_py(&self.columns, py)
    }

    #[setter]
    fn set_column_filters(&mut self, filters: &Bound<'_, PyDict>) -> PyResult<()> {
        crate::columns::set_filters(&mut self.columns, filters)
    }

    #[getter]
    fn get_pinned_left(&self) -> Vec<String> {
        self.columns.pinned_left.clone()
    }

    #[setter]
    fn set_pinned_left(&mut self, columns: Vec<String>) {
        self.columns.pinned_left = columns;
    }

    #[getter]
    fn get_pinned_right(&self) -> Vec<String> {
        self.columns.pinned_right.clone()
    }

    #[setter]
    fn set_pinned_right(&mut self, columns: Vec<String>) {
        self.columns.pinned_right = columns;
    }

    #[getter]
    fn get_visible_rows<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        PyList::new(py, self.shown.iter())
    }

    /// Removes the filters of all the columns
    fn clear_filters(&mut self) {
        self.columns.filters.clear();
    }

    /// Gives the columns their default width, order and no pin
    fn reset_columns(&mut self) {
        self.columns.widths.clear();
        self.columns.order.clear();
        self.columns.pinned_left.clear();
        self.columns.pinned_right.clear();
        self.columns.scroll = 0.0;
    }
}

/// Draws a table with the state of a `TableView`
pub(crate) fn draw(
    ui: &mut egui::Ui,
    view: &Bound<'_, TableView>,
    headers: &[String],
    rows: &[Vec<String>],
    num_columns: usize,
    selected: &mut Option<PyRefMut<'_, Int>>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let keys: Vec<String> = (0..num_columns).map(|c| headers.get(c).cloned().unwrap_or_else(|| format!("{}", c + 1))).collect();
    let mut types = vec![DataType::Text; num_columns];
    if let Some(names) = kwarg::<Vec<String>>(kwargs, "types")? {
        for (c, name) in names.iter().enumerate().take(num_columns) {
            types[c] = crate::columns::parse_type(name)?;
        }
    }
    let mut defaults = vec![DEFAULT_WIDTH; num_columns];
    if let Some(widths) = kwarg::<Vec<Option<f32>>>(kwargs, "column_widths")? {
        for (c, w) in widths.iter().enumerate().take(num_columns) {
            if let Some(w) = w {
                defaults[c] = w.clamp(WIDTH_RANGE.0, WIDTH_RANGE.1);
            }
        }
    }
    let options = TableOptions {
        header: crate::columns::header_options(kwargs, false)?,
        sortable: kwarg::<bool>(kwargs, "sortable")?.unwrap_or(true),
        striped: kwarg::<bool>(kwargs, "striped")?.unwrap_or(true),
        max_height: kwarg::<f32>(kwargs, "max_height")?,
        scroll_to_row: kwarg::<usize>(kwargs, "scroll_to_row")?,
        id: kwarg::<String>(kwargs, "id")?.unwrap_or_else(|| "table".into()),
    };
    let mut v = view.borrow_mut();
    let current = selected.as_ref().map(|s| s.value);
    let output = pyegui_core::table::show(ui, &mut v.columns, &keys, &types, &defaults, rows, current, &options);
    if let (Some(row), Some(selected)) = (output.clicked, selected.as_mut()) {
        selected.value = row as i32;
    }
    v.shown = output.shown;
    Ok(())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TableView>()?;
    Ok(())
}
