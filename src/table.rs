use eframe::egui;
use egui_extras::{Column, TableBuilder};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};

use crate::{current_ui, kwarg, run_nested_call, Int, UI};

/// The columns of a table: auto sized and the last one taking the rest of the width, or as
/// given by the column_widths keyword arg
fn columns(num_columns: usize, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Vec<Column>> {
    let Some(widths) = kwarg::<Vec<Bound<'_, PyAny>>>(kwargs, "column_widths")? else {
        let mut columns = vec![Column::auto(); num_columns.saturating_sub(1)];
        columns.push(Column::remainder());
        return Ok(columns);
    };
    if widths.len() != num_columns {
        return Err(PyValueError::new_err(format!(
            "column_widths has {} values for {} columns",
            widths.len(),
            num_columns
        )));
    }
    widths
        .iter()
        .map(|width| {
            if width.is_none() {
                Ok(Column::auto())
            } else if let Ok(width) = width.extract::<f32>() {
                // a starting width, still resizable, never too narrow to read
                Ok(Column::initial(width).at_least(width.min(40.0)).clip(true))
            } else if width.extract::<String>().is_ok_and(|w| w == "remainder") {
                Ok(Column::remainder())
            } else {
                Err(PyTypeError::new_err(
                    "column_widths values must be a width, None for auto or \"remainder\"",
                ))
            }
        })
        .collect()
}

/// Creates a table builder with the options shared by table and table_ui
fn table_builder<'a>(
    ui: &'a mut egui::Ui,
    num_columns: usize,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<TableBuilder<'a>> {
    let columns = columns(num_columns, kwargs)?;
    let mut t = TableBuilder::new(ui)
        .striped(kwarg::<bool>(kwargs, "striped")?.unwrap_or(true))
        .resizable(kwarg::<bool>(kwargs, "resizable")?.unwrap_or(true));
    for column in columns {
        t = t.column(column);
    }

    if let Some(id) = kwarg::<String>(kwargs, "id")? {
        t = t.id_salt(id);
    }
    if let Some(height) = kwarg::<f32>(kwargs, "max_height")? {
        t = t.max_scroll_height(height);
    }
    Ok(t)
}

/// Shows a table of texts. Only visible rows are drawn, so it is fast even with many rows.
///
/// Args:
///     headers (list[str]): column names
///
///     rows (list[list[str]]): cells of each row
///
///     selected (Int): if given, rows can be selected by clicking on them. Value is -1 when nothing is selected
///
/// Keyword args:
///
///     id (str): id of the table, needed if there are several tables in the same ui
///
///     striped (bool): every other row has a different background color. Default is True
///
///     resizable (bool): columns can be resized. Default is True
///
///     max_height (float): max height of the table before scrolling
///
///     column_widths (list[float | None | str]): width of each column: a starting width, None to fit
///     the contents or "remainder" for the rest of the width. Default fits the contents, the last
///     column takes the rest
///
/// Example::
///
///     selected = Int(-1)
///     rows = [["Alice", "32"], ["Bob", "27"]]
///
///     def update_func(ctx):
///         table(["Name", "Age"], rows, selected)
///         if selected.value >= 0:
///             label(f"Selected: {rows[selected.value][0]}")
#[pyfunction]
#[pyo3(signature = (headers, rows, selected=None, **kwargs))]
unsafe fn table(
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    mut selected: Option<PyRefMut<'_, Int>>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let num_columns = rows
        .iter()
        .map(|row| row.len())
        .chain([headers.len()])
        .max()
        .unwrap_or(0);

    if num_columns == 0 {
        return Ok(());
    }

    let ui = current_ui(&UI)?;
    let row_height = ui.spacing().interact_size.y;

    let mut t = table_builder(ui, num_columns, kwargs)?;
    if selected.is_some() {
        t = t.sense(egui::Sense::click());
    }

    t.header(row_height, |mut header| {
        for name in &headers {
            header.col(|ui| {
                ui.strong(name);
            });
        }
    })
    .body(|body| {
        body.rows(row_height, rows.len(), |mut row| {
            let index = row.index();

            if let Some(selected) = &selected {
                row.set_selected(selected.value == index as i32);
            }

            for cell in &rows[index] {
                row.col(|ui| {
                    ui.label(cell);
                });
            }

            if let Some(selected) = &mut selected {
                if row.response().clicked() {
                    selected.value = index as i32;
                }
            }
        });
    });

    Ok(())
}

/// Shows a table whose cells are drawn by a function. Only visible rows are drawn.
///
/// Args:
///     headers (list[str]): column names
///
///     num_rows (int): number of rows
///
///     cell_func (Callable[[int, int], None]): draws the cell at the given row and column
///
/// Accepts the same keyword args as table.
///
/// Example::
///
///     files = ["a.txt", "b.txt"]
///
///     def cell(row, col):
///         if col == 0:
///             label(files[row])
///         elif button_clicked("Delete"):
///             files.pop(row)
///
///     def update_func(ctx):
///         table_ui(["File", ""], len(files), cell)
#[pyfunction]
#[pyo3(signature = (headers, num_rows, cell_func, **kwargs))]
unsafe fn table_ui(
    headers: Vec<String>,
    num_rows: usize,
    cell_func: Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let num_columns = headers.len();

    if num_columns == 0 {
        return Ok(());
    }

    let ui = current_ui(&UI)?;
    let row_height = ui.spacing().interact_size.y;
    let mut result = Ok(());

    table_builder(ui, num_columns, kwargs)?
        .header(row_height, |mut header| {
            for name in &headers {
                header.col(|ui| {
                    ui.strong(name);
                });
            }
        })
        .body(|body| {
            body.rows(row_height, num_rows, |mut row| {
                let index = row.index();

                for col in 0..num_columns {
                    row.col(|ui| {
                        if let Err(err) = run_nested_call(ui, &cell_func, (index, col)) {
                            result = Err(err);
                        }
                    });
                }
            });
        });

    result
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(table, m)?)?;
    m.add_function(wrap_pyfunction!(table_ui, m)?)?;
    Ok(())
}
