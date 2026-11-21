//! `TableView`: the state of a `table` that sorts, filters, reorders, resizes and pins its columns.
//! The table is drawn here (not with `egui_extras`) because pinned columns need three zones: the
//! columns pinned at the left, the columns that scroll, and the columns pinned at the right.

use std::collections::HashMap;

use eframe::egui::{self, pos2, vec2, Align2, Rect, Sense, TextStyle};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::columns::{
    hscroll_bar, hscroll_wheel, parse_date, parse_number, zoned_header, Cell, ColumnsState, DataType,
    Value, Zones, WIDTH_RANGE,
};
use crate::{kwarg, Int};

const DEFAULT_WIDTH: f32 = 120.0;

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

/// The zones of the columns in the order shown, and the index of the column at each position. What
/// the widths leave of the available width goes to the last column the user did not size
fn zones_of(keys: &[String], defaults: &[f32], state: &ColumnsState, available: f32) -> (Zones, Vec<usize>) {
    let order: Vec<usize> = state.ordered(keys).iter().filter_map(|key| keys.iter().position(|k| k == key)).collect();
    let mut widths: Vec<f32> = order.iter().map(|&c| state.widths.get(&keys[c]).copied().unwrap_or(defaults[c])).collect();
    let total: f32 = widths.iter().sum();
    if total < available {
        let free = |zone_keys: &dyn Fn(&String) -> bool| order.iter().rposition(|&c| zone_keys(&keys[c]) && !state.widths.contains_key(&keys[c]));
        let scrolling = |k: &String| !state.pinned_left.contains(k) && !state.pinned_right.contains(k);
        if let Some(last) = free(&scrolling).or_else(|| free(&|_| true)) {
            widths[last] += available - total;
        }
    }
    let ordered_keys: Vec<String> = order.iter().map(|&c| keys[c].clone()).collect();
    (Zones::new(&ordered_keys, &widths, state, available, None), order)
}

fn value_of(text: &str, ty: DataType) -> Value {
    match ty {
        DataType::Text | DataType::Choice => Value::Text(text.to_string()),
        DataType::Number => parse_number(text).map_or(Value::None, Value::Num),
        DataType::Date => parse_date(text).map_or(Value::None, |(t, _)| Value::Num(t)),
    }
}

fn cell_text(rows: &[Vec<String>], row: usize, col: usize) -> &str {
    rows[row].get(col).map_or("", |s| s.as_str())
}

/// The rows kept by the filters, sorted
fn arrange(rows: &[Vec<String>], keys: &[String], types: &[DataType], state: &ColumnsState) -> Vec<usize> {
    let active: Vec<(usize, DataType, &crate::columns::Filter)> = keys
        .iter()
        .enumerate()
        .filter_map(|(col, key)| state.filters.get(key).filter(|f| !f.is_empty()).map(|f| (col, types[col], f)))
        .collect();
    let mut shown: Vec<usize> = (0..rows.len())
        .filter(|&row| active.iter().all(|(col, ty, f)| f.matches(*ty, &value_of(cell_text(rows, row, *col), *ty))))
        .collect();
    if let Some(col) = keys.iter().position(|k| *k == state.sort_key) {
        let ty = types[col];
        let mut keyed: Vec<(Value, usize)> =
            shown.iter().map(|&row| (value_of(cell_text(rows, row, col), ty), row)).collect();
        keyed.sort_by(|a, b| {
            let order = a.0.compare(&b.0);
            if state.ascending { order } else { order.reverse() }
        });
        shown = keyed.into_iter().map(|(_, row)| row).collect();
    }
    shown
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
    let opts = crate::columns::header_options(kwargs, false)?;
    let sortable = kwarg::<bool>(kwargs, "sortable")?.unwrap_or(true);
    let striped = kwarg::<bool>(kwargs, "striped")?.unwrap_or(true);
    let max_height = kwarg::<f32>(kwargs, "max_height")?;
    let scroll_to_row = kwarg::<usize>(kwargs, "scroll_to_row")?;
    let salt = kwarg::<String>(kwargs, "id")?.unwrap_or_else(|| "table".into());
    let id = ui.make_persistent_id(("table_view", salt));

    let available = ui.available_width();
    if !available.is_finite() {
        return Ok(());
    }
    let row_height = ui.spacing().interact_size.y;
    let header_height = row_height + 6.0;
    let mut v = view.borrow_mut();

    let whole_top = ui.cursor().min;
    let (header_rect, _) = ui.allocate_exact_size(vec2(available, header_height), Sense::hover());
    let area = Rect::from_min_size(whole_top, vec2(available, ui.available_height() + header_height));

    let (zones, order) = zones_of(&keys, &defaults, &v.columns, available);
    hscroll_wheel(ui, &mut v.columns, &zones, area);

    // the header, one zone after the other
    let before = (v.columns.order.clone(), v.columns.widths.clone(), v.columns.sort_key.clone(), v.columns.ascending);
    let cells: Vec<Cell> = order.iter().map(|&col| Cell {
        key: keys[col].clone(),
        title: keys[col].clone(),
        rect: header_rect,
        ty: types[col],
        sortable,
        filterable: true,
        movable: true,
        resizable: true,
    }).collect();
    zoned_header(ui, id.with("header"), header_rect, &zones, cells, &mut v.columns, opts, &mut |key| {
        let Some(col) = keys.iter().position(|k| k == key) else { return vec![] };
        let mut values: Vec<String> = (0..rows.len()).map(|r| cell_text(rows, r, col).to_string()).collect();
        values.sort();
        values.dedup();
        values
    });
    let after = (v.columns.order.clone(), v.columns.widths.clone(), v.columns.sort_key.clone(), v.columns.ascending);
    if before != after {
        ui.ctx().request_repaint(); // the rows follow at the next frame
    }
    ui.painter().hline(header_rect.x_range(), header_rect.bottom(), ui.visuals().widgets.noninteractive.bg_stroke);

    // the rows
    let (zones, order) = zones_of(&keys, &defaults, &v.columns, available);
    let scroll = v.columns.scroll;
    let shown = arrange(rows, &keys, &types, &v.columns);
    let mut height = ui.available_height();
    if let Some(max) = max_height {
        height = height.min(max);
    }
    let font = TextStyle::Body.resolve(ui.style());
    let text_color = ui.visuals().text_color();
    let accent = ui.visuals().selection.bg_fill;
    let faint = ui.visuals().faint_bg_color;
    let selectable = selected.is_some();
    let current = selected.as_ref().map_or(-1, |s| s.value);
    let mut clicked = None;
    let mut scroll_area = egui::ScrollArea::vertical()
        .id_salt(id.with("rows"))
        .max_height(height.max(row_height))
        .auto_shrink([false, max_height.is_some()]);
    if let Some(row) = scroll_to_row {
        scroll_area = scroll_area.vertical_scroll_offset(row as f32 * row_height);
    }
    let output = scroll_area.show_rows(ui, row_height, shown.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for index in range {
            let row = shown[index];
            let sense = if selectable { Sense::click() } else { Sense::hover() };
            let (rect, response) = ui.allocate_exact_size(vec2(available, row_height), sense);
            if selectable && response.clicked() {
                clicked = Some(row);
            }
            if current == row as i32 && selectable {
                ui.painter().rect_filled(rect, 0.0, accent);
            } else if striped && index % 2 == 1 {
                ui.painter().rect_filled(rect, 0.0, faint);
            } else if response.hovered() && selectable {
                ui.painter().rect_filled(rect, 0.0, text_color.gamma_multiply(0.06));
            }
            for p in &zones.placed {
                let col = order[p.index];
                let cell = zones.cell_rect(rect, p, scroll);
                let clip = zones.visible(rect, p, scroll);
                if clip.width() <= 0.0 {
                    continue;
                }
                let text = cell_text(rows, row, col);
                let painter = ui.painter().with_clip_rect(clip);
                if types[col] == DataType::Number {
                    painter.text(pos2(cell.right() - 8.0, cell.center().y), Align2::RIGHT_CENTER, text, font.clone(), text_color);
                } else {
                    painter.text(pos2(cell.left() + 8.0, cell.center().y), Align2::LEFT_CENTER, text, font.clone(), text_color);
                }
            }
        }
    });
    if let (Some(row), Some(selected)) = (clicked, selected.as_mut()) {
        selected.value = row as i32;
    }
    hscroll_bar(ui, id, &mut v.columns, &zones, output.inner_rect);
    v.shown = shown;
    Ok(())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TableView>()?;
    Ok(())
}
