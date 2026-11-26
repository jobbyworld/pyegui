//! A table of text cells that sorts, filters, reorders, resizes and pins its columns. The state is a
//! `ColumnsState`; the table is drawn here (not with `egui_extras`) because pinned columns need three
//! zones: the columns pinned at the left, the columns that scroll, and the columns pinned at the right.
//! Used by `table(view=...)` of the Python extension and by the Rust applications.

use egui::{pos2, vec2, Align2, Rect, Sense, TextStyle};

use crate::columns::{
    hscroll_bar, hscroll_wheel, parse_date, parse_number, zoned_header, Cell, ColumnsState, DataType, HeaderOptions, Value,
    Zones,
};

/// The width of a column the user did not size and the caller gave none for
pub const DEFAULT_WIDTH: f32 = 120.0;

/// What a table is drawn with
pub struct TableOptions {
    pub header: HeaderOptions,
    pub sortable: bool,
    pub striped: bool,
    /// the most the table takes in height before it scrolls
    pub max_height: Option<f32>,
    /// scrolls just enough to show this row (give it only during the frame the table has to scroll)
    pub scroll_to_row: Option<usize>,
    /// tells apart the tables of a ui
    pub id: String,
}

impl Default for TableOptions {
    fn default() -> Self {
        TableOptions {
            header: HeaderOptions { resizable: true, pinnable: true, reorderable: true, filterable: true, drag_out: false, grow_left: false, tinted: false, settings: false },
            sortable: true,
            striped: true,
            max_height: None,
            scroll_to_row: None,
            id: "table".into(),
        }
    }
}

/// What a frame of the table tells
pub struct TableOutput {
    /// the indexes in `rows` of the rows shown, in order
    pub shown: Vec<usize>,
    /// the index in `rows` of the row clicked
    pub clicked: Option<usize>,
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
pub fn arrange(rows: &[Vec<String>], keys: &[String], types: &[DataType], state: &ColumnsState) -> Vec<usize> {
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
        let mut keyed: Vec<(Value, usize)> = shown.iter().map(|&row| (value_of(cell_text(rows, row, col), ty), row)).collect();
        keyed.sort_by(|a, b| {
            let order = a.0.compare(&b.0);
            if state.ascending { order } else { order.reverse() }
        });
        shown = keyed.into_iter().map(|(_, row)| row).collect();
    }
    shown
}

/// Draws a table. `keys` are the titles of the columns (all different), `types` their data types and
/// `defaults` their starting widths. `selected` is the index in `rows` of the row selected, `None` for a
/// table whose rows cannot be selected.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    state: &mut ColumnsState,
    keys: &[String],
    types: &[DataType],
    defaults: &[f32],
    rows: &[Vec<String>],
    selected: Option<i32>,
    options: &TableOptions,
) -> TableOutput {
    let none = TableOutput { shown: vec![], clicked: None };
    let id = ui.make_persistent_id(("table_view", options.id.as_str()));
    let available = ui.available_width();
    if !available.is_finite() {
        return none;
    }
    let row_height = ui.spacing().interact_size.y;
    let header_height = row_height + 6.0;

    let whole_top = ui.cursor().min;
    let (header_rect, _) = ui.allocate_exact_size(vec2(available, header_height), Sense::hover());
    let area = Rect::from_min_size(whole_top, vec2(available, ui.available_height() + header_height));

    let (zones, order) = zones_of(keys, defaults, state, available);
    hscroll_wheel(ui, state, &zones, area);

    // the header, one zone after the other
    let before = (state.order.clone(), state.widths.clone(), state.sort_key.clone(), state.ascending);
    let cells: Vec<Cell> = order
        .iter()
        .map(|&col| Cell {
            key: keys[col].clone(),
            title: keys[col].clone(),
            rect: header_rect,
            ty: types[col],
            sortable: options.sortable,
            filterable: true,
            movable: true,
            resizable: true,
            icon: None,
        })
        .collect();
    zoned_header(ui, id.with("header"), header_rect, &zones, cells, state, options.header, &mut |key| {
        let Some(col) = keys.iter().position(|k| k == key) else { return vec![] };
        let mut values: Vec<String> = (0..rows.len()).map(|r| cell_text(rows, r, col).to_string()).collect();
        values.sort();
        values.dedup();
        values
    });
    let after = (state.order.clone(), state.widths.clone(), state.sort_key.clone(), state.ascending);
    if before != after {
        ui.ctx().request_repaint(); // the rows follow at the next frame
    }
    ui.painter().hline(header_rect.x_range(), header_rect.bottom(), ui.visuals().widgets.noninteractive.bg_stroke);

    // the rows
    let (zones, order) = zones_of(keys, defaults, state, available);
    let scroll = state.scroll;
    let shown = arrange(rows, keys, types, state);
    let mut height = ui.available_height();
    if let Some(max) = options.max_height {
        height = height.min(max);
    }
    let font = TextStyle::Body.resolve(ui.style());
    let text_color = ui.visuals().text_color();
    let accent = ui.visuals().selection.bg_fill;
    let faint = ui.visuals().faint_bg_color;
    let selectable = selected.is_some();
    let current = selected.unwrap_or(-1);
    let mut clicked = None;
    let mut scroll_area = egui::ScrollArea::vertical()
        .id_salt(id.with("rows"))
        .max_height(height.max(row_height))
        .auto_shrink([false, options.max_height.is_some()]);
    if let Some(row) = options.scroll_to_row {
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
            } else if options.striped && index % 2 == 1 {
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
    hscroll_bar(ui, id, state, &zones, output.inner_rect);
    TableOutput { shown, clicked }
}
