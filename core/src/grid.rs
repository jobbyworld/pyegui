//! A data grid: rows of cells in columns, flat or as a tree, that is not tied to files. The columns sort, filter,
//! move, resize, hide and pin (the `ColumnsState` and the header of `columns`), the rows are selected with the
//! mouse and the keyboard, a cell can be edited in place, a branch of a tree opens and closes, and a context menu,
//! a double click and a drag are told to the caller. The views of files are the same thing with files as rows.
//!
//! The grid keeps no data: give the rows at every frame, and apply what the output tells (a cell edited, a branch
//! opened). What is kept is the state of the view (`GridState`): the columns as the user left them, the selection,
//! the branches opened and the cell being edited.

use std::collections::{HashMap, HashSet};

use egui::{pos2, vec2, Align2, Key, Rect, Sense, TextStyle};

use crate::columns::{hscroll_bar, hscroll_wheel, zoned_header, Cell, ColumnsState, DataType, Filter, HeaderOptions, Value, Zones};
use crate::files::{lock_arrows, selection_colors};
use crate::table::{value_of, DEFAULT_WIDTH};

/// A column of the grid
#[derive(Clone, Debug)]
pub struct GridColumn {
    /// what the state and the output name the column by (all different)
    pub key: String,
    pub title: String,
    pub ty: DataType,
    pub width: f32,
    /// a double click on a cell of the column (or F2 on the row) edits it in place
    pub editable: bool,
    pub icon: Option<crate::columns::ColumnIcon>,
    /// the cells are drawn by the caller (the `cell` function of `show`), not as text
    pub custom: bool,
}

impl GridColumn {
    pub fn new(key: &str, title: &str) -> Self {
        GridColumn { key: key.into(), title: title.into(), ty: DataType::Text, width: DEFAULT_WIDTH, editable: false, icon: None, custom: false }
    }
}

/// A row: its id (all different), its texts one per column in the order of the columns given, and the id of its
/// parent for a tree (None for a root)
#[derive(Clone, Debug, Default)]
pub struct GridRow {
    pub id: String,
    pub cells: Vec<String>,
    pub parent: Option<String>,
    /// the row cannot be edited
    pub readonly: bool,
    /// an image (an uri) drawn before the text of the first column
    pub icon: Option<String>,
    /// a branch whose children are not given yet (a folder not read): it has a chevron and opens all the same
    pub expandable: bool,
    /// colored dots drawn at the end of the first column (tags)
    pub badges: Vec<egui::Color32>,
    /// the row is drawn faded (a hidden file)
    pub dim: bool,
}

/// The cell being edited
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub row: String,
    pub column: String,
    pub text: String,
    /// the field has been given the focus
    focused: bool,
}

impl Edit {
    pub fn new(row: String, column: String, text: String) -> Self {
        Edit { row, column, text, focused: false }
    }
}

#[derive(Default)]
pub struct GridState {
    pub columns: ColumnsState,
    /// the ids selected, in the order of the clicks
    pub selected: Vec<String>,
    /// the row the keyboard is on, and the one a Shift range starts from
    pub cursor: Option<String>,
    pub anchor: Option<String>,
    /// the branches of the tree that are open
    pub expanded: HashSet<String>,
    pub editing: Option<Edit>,
    /// scrolls to this row at the next frame
    pub scroll_to: Option<String>,
    /// the grid has the keyboard focus (read it to tint the selection)
    pub focused: bool,
    /// the lasso being drawn: where it started (in the coordinates of the content) and the selection it keeps
    pub band: Option<(egui::Vec2, Vec<String>)>,
    /// the scroll offset of the last frame, and what the lasso asks to scroll by
    pub offset_y: f32,
    pub scroll_by: f32,
}

impl Default for ColumnsState {
    fn default() -> Self {
        ColumnsState::new("")
    }
}

#[derive(Clone)]
pub struct GridOptions {
    pub header: HeaderOptions,
    pub multi_select: bool,
    pub striped: bool,
    /// the rows are a tree (`parent`), with a chevron and an indentation in the first column
    pub tree: bool,
    pub sortable: bool,
    pub max_height: Option<f32>,
    pub row_height: Option<f32>,
    pub indent: f32,
    /// tells the grids of a ui apart
    pub id: String,
    /// a drag of the selection is told in the output
    pub drag: bool,
    /// the first column is always at the left, with the tree
    pub tree_column: Option<String>,
    /// the rows come sorted and filtered by the caller: the grid keeps their order and ignores the filters
    pub keep_order: bool,
    /// a thin horizontal line under each row
    pub row_lines: bool,
}

impl Default for GridOptions {
    fn default() -> Self {
        GridOptions {
            header: HeaderOptions { resizable: true, pinnable: true, reorderable: true, filterable: true, drag_out: false, grow_left: false, tinted: false, settings: false },
            multi_select: true,
            striped: true,
            tree: false,
            sortable: true,
            max_height: None,
            row_height: None,
            indent: 16.0,
            id: "grid".into(),
            drag: false,
            tree_column: None,
            keep_order: false,
            row_lines: true,
        }
    }
}

/// What the user did in a frame
#[derive(Default, Debug)]
pub struct GridOutput {
    /// the ids of the rows shown, in order (the rows kept by the filters, the branches open)
    pub shown: Vec<String>,
    /// the row clicked, and the one double clicked or opened with Enter
    pub clicked: Option<String>,
    pub activated: Vec<String>,
    pub selection_changed: bool,
    /// a cell was edited: (row, column key, new text)
    pub edited: Option<(String, String, String)>,
    /// a branch was opened (true) or closed (false)
    pub toggled: Option<(String, bool)>,
    /// the rows that were dragged from (the selection), the frame the drag started
    pub drag_started: Option<Vec<String>>,
    /// the id of the row the pointer is over
    pub hovered: Option<String>,
    /// the rectangles of the rows drawn this frame
    pub row_rects: Vec<(String, Rect)>,
}

/// One line of what is shown
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    /// the index in the rows given
    pub index: usize,
    pub depth: usize,
    pub has_children: bool,
    pub open: bool,
}

fn filters_match(row: &GridRow, columns: &[GridColumn], state: &ColumnsState) -> bool {
    columns.iter().enumerate().all(|(c, col)| match state.filters.get(&col.key).filter(|f: &&Filter| !f.is_empty()) {
        None => true,
        Some(f) => f.matches(col.ty, &value_of(row.cells.get(c).map_or("", |s| s.as_str()), col.ty)),
    })
}

fn sort_indexes(indexes: &mut [usize], rows: &[GridRow], columns: &[GridColumn], state: &ColumnsState) {
    if let Some(c) = columns.iter().position(|col| col.key == state.sort_key) {
        let ty = columns[c].ty;
        let key = |i: usize| value_of(rows[i].cells.get(c).map_or("", |s| s.as_str()), ty);
        let mut keyed: Vec<(Value, usize)> = indexes.iter().map(|&i| (key(i), i)).collect();
        keyed.sort_by(|a, b| {
            let order = a.0.compare(&b.0);
            if state.ascending { order } else { order.reverse() }
        });
        for (slot, (_, i)) in indexes.iter_mut().zip(keyed) {
            *slot = i;
        }
    }
}

/// The lines to show: the rows kept by the filters, sorted, and for a tree the branches that are open (a row that
/// is kept brings its ancestors, and with a filter active they are open). Pure, so it is tested.
pub fn arrange(rows: &[GridRow], columns: &[GridColumn], state: &ColumnsState, tree: bool, expanded: &HashSet<String>) -> Vec<Shown> {
    arrange_with(rows, columns, state, tree, expanded, false)
}

/// `keep_order`: the rows are already sorted and filtered, they keep their order
pub fn arrange_with(rows: &[GridRow], columns: &[GridColumn], state: &ColumnsState, tree: bool, expanded: &HashSet<String>, keep_order: bool) -> Vec<Shown> {
    let owned;
    let state = if keep_order {
        let mut plain = ColumnsState::new("");
        plain.sort_key = String::new();
        owned = plain;
        &owned
    } else {
        state
    };
    let filtered = columns.iter().any(|c| state.filters.get(&c.key).is_some_and(|f| !f.is_empty()));
    if !tree {
        let mut indexes: Vec<usize> = (0..rows.len()).filter(|&i| filters_match(&rows[i], columns, state)).collect();
        sort_indexes(&mut indexes, rows, columns, state);
        return indexes.into_iter().map(|index| Shown { index, depth: 0, has_children: false, open: false }).collect();
    }
    let position: HashMap<&str, usize> = rows.iter().enumerate().map(|(i, r)| (r.id.as_str(), i)).collect();
    let mut children: Vec<Vec<usize>> = vec![vec![]; rows.len()];
    let mut roots = vec![];
    for (i, row) in rows.iter().enumerate() {
        match row.parent.as_deref().and_then(|p| position.get(p)) {
            Some(&parent) if parent != i => children[parent].push(i),
            _ => roots.push(i),
        }
    }
    // a row is kept when it matches or one of its descendants does
    let mut keep = vec![false; rows.len()];
    fn mark(i: usize, rows: &[GridRow], columns: &[GridColumn], state: &ColumnsState, children: &[Vec<usize>], keep: &mut [bool], depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        let mut any = filters_match(&rows[i], columns, state);
        for &c in &children[i] {
            if mark(c, rows, columns, state, children, keep, depth + 1) {
                any = true;
            }
        }
        keep[i] = any;
        any
    }
    for &r in &roots {
        mark(r, rows, columns, state, &children, &mut keep, 0);
    }
    let mut out = vec![];
    #[allow(clippy::too_many_arguments)]
    fn walk(list: &mut Vec<usize>, depth: usize, rows: &[GridRow], columns: &[GridColumn], state: &ColumnsState, children: &[Vec<usize>], keep: &[bool], expanded: &HashSet<String>, filtered: bool, out: &mut Vec<Shown>) {
        if depth > 64 {
            return;
        }
        list.retain(|&i| keep[i]);
        sort_indexes(list, rows, columns, state);
        for &i in list.iter() {
            let kids: Vec<usize> = children[i].iter().copied().filter(|&c| keep[c]).collect();
            let open = (!kids.is_empty() || rows[i].expandable) && (filtered || expanded.contains(&rows[i].id));
            out.push(Shown { index: i, depth, has_children: !children[i].is_empty() || rows[i].expandable, open });
            if open {
                let mut kids = kids;
                walk(&mut kids, depth + 1, rows, columns, state, children, keep, expanded, filtered, out);
            }
        }
    }
    walk(&mut roots, 0, rows, columns, state, &children, &keep, expanded, filtered, &mut out);
    out
}

// -- selection, pure ----------------------------------------------------------------------------------------

impl GridState {
    pub fn new(sort_key: &str) -> Self {
        GridState { columns: ColumnsState::new(sort_key), ..Default::default() }
    }

    pub fn select_only(&mut self, id: &str) {
        self.selected = vec![id.to_string()];
        self.cursor = Some(id.to_string());
        self.anchor = Some(id.to_string());
    }

    /// A click on a row: alone it selects only it; with Cmd (Ctrl) it adds or removes it; with Shift it selects the
    /// range from the anchor. Returns true if the selection changed
    pub fn click(&mut self, shown: &[String], id: &str, modifiers: egui::Modifiers, multi: bool) -> bool {
        let before = self.selected.clone();
        if multi && modifiers.shift {
            self.select_range(shown, id);
        } else if multi && modifiers.command {
            if let Some(at) = self.selected.iter().position(|s| s == id) {
                self.selected.remove(at);
            } else {
                self.selected.push(id.to_string());
            }
            self.cursor = Some(id.to_string());
            self.anchor = Some(id.to_string());
        } else {
            self.select_only(id);
        }
        before != self.selected
    }

    fn select_range(&mut self, shown: &[String], to: &str) {
        let anchor = self.anchor.clone().or_else(|| self.cursor.clone()).unwrap_or_else(|| to.to_string());
        let (Some(a), Some(b)) = (shown.iter().position(|s| *s == anchor), shown.iter().position(|s| s == to)) else {
            self.select_only(to);
            return;
        };
        let (lo, hi) = (a.min(b), a.max(b));
        self.selected = shown[lo..=hi].to_vec();
        self.cursor = Some(to.to_string());
        self.anchor = Some(anchor);
    }

    /// Moves the cursor by `delta` rows (or to an end), extending the selection with Shift. True if it changed
    pub fn move_cursor(&mut self, shown: &[String], target: Target, extend: bool, multi: bool) -> bool {
        if shown.is_empty() {
            return false;
        }
        let at = self.cursor.as_ref().and_then(|c| shown.iter().position(|s| s == c));
        let last = shown.len() - 1;
        let next = match (target, at) {
            (Target::Home, _) => 0,
            (Target::End, _) => last,
            (Target::Delta(d), Some(i)) => (i as i64 + d).clamp(0, last as i64) as usize,
            (Target::Delta(d), None) => if d >= 0 { 0 } else { last },
        };
        let before = (self.selected.clone(), self.cursor.clone());
        let id = shown[next].clone();
        if multi && extend {
            self.select_range(shown, &id);
        } else {
            self.select_only(&id);
        }
        self.scroll_to = Some(id);
        before != (self.selected.clone(), self.cursor.clone())
    }

    /// Forgets the selected ids that are not in `ids` any more (a filter, a deletion)
    pub fn keep_existing(&mut self, ids: &HashSet<&str>) {
        self.selected.retain(|s| ids.contains(s.as_str()));
        if self.cursor.as_ref().is_some_and(|c| !ids.contains(c.as_str())) {
            self.cursor = None;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Target {
    Delta(i64),
    Home,
    End,
}

/// The ids of the rows (`row_height` high, one after the other from 0) that a band from `top` to `bottom` touches
pub fn rows_in_band(shown: &[String], row_height: f32, top: f32, bottom: f32) -> Vec<String> {
    let (top, bottom) = if top <= bottom { (top, bottom) } else { (bottom, top) };
    if shown.is_empty() || row_height <= 0.0 || bottom < 0.0 {
        return vec![];
    }
    let first = (top.max(0.0) / row_height).floor() as usize;
    let last = ((bottom / row_height).floor() as usize).min(shown.len() - 1);
    if first > last {
        return vec![];
    }
    shown[first..=last].to_vec()
}

// -- drawing ------------------------------------------------------------------------------------------------

/// Draws the grid. `cell` may draw a cell itself (a chip, a switch): called with the cell rectangle, it returns
/// true when it drew the cell. `menu` is the context menu of a right click: it is given the ids concerned.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    state: &mut GridState,
    columns: &[GridColumn],
    rows: &[GridRow],
    options: &GridOptions,
    cell: &mut dyn FnMut(&mut egui::Ui, &GridRow, &GridColumn, Rect) -> bool,
    menu: &mut dyn FnMut(&mut egui::Ui, &[String]),
) -> GridOutput {
    let mut out = GridOutput::default();
    let id = ui.make_persistent_id(("data_grid", options.id.as_str()));
    let available = ui.available_width();
    if !available.is_finite() || columns.is_empty() {
        return out;
    }
    let row_height = options.row_height.unwrap_or(ui.spacing().interact_size.y);
    let header_height = ui.spacing().interact_size.y + 6.0;

    let keys: Vec<String> = columns.iter().map(|c| c.key.clone()).collect();
    state.columns.catalog = columns.iter().map(|c| (c.key.clone(), c.title.clone())).collect();
    // the columns the user hid are not drawn
    let visible: Vec<usize> = (0..columns.len()).filter(|&c| !state.columns.hidden.contains(&columns[c].key)).collect();
    let visible_keys: Vec<String> = visible.iter().map(|&c| keys[c].clone()).collect();
    let order: Vec<usize> = state.columns.ordered(&visible_keys).iter().filter_map(|k| visible.iter().copied().find(|&c| &keys[c] == k)).collect();
    let first_key = options.tree_column.clone().or_else(|| order.first().map(|&c| keys[c].clone()));
    let zones_for = |state: &ColumnsState| -> Zones {
        let mut widths: Vec<f32> = order.iter().map(|&c| state.widths.get(&keys[c]).copied().unwrap_or(columns[c].width)).collect();
        let total: f32 = widths.iter().sum();
        if total < available {
            if let Some(last) = order.iter().rposition(|&c| !state.widths.contains_key(&keys[c])) {
                widths[last] += available - total;
            }
        }
        let ordered_keys: Vec<String> = order.iter().map(|&c| keys[c].clone()).collect();
        Zones::new(&ordered_keys, &widths, state, available, if options.tree { first_key.as_deref() } else { None })
    };

    let top = ui.cursor().min;
    let (header_rect, _) = ui.allocate_exact_size(vec2(available, header_height), Sense::hover());
    let area = Rect::from_min_size(top, vec2(available, ui.available_height() + header_height));
    let zones = zones_for(&state.columns);
    hscroll_wheel(ui, &mut state.columns, &zones, area);
    let header_cells: Vec<Cell> = order
        .iter()
        .map(|&c| Cell {
            key: keys[c].clone(),
            title: columns[c].title.clone(),
            rect: header_rect,
            ty: columns[c].ty,
            sortable: options.sortable,
            filterable: options.header.filterable,
            movable: true,
            resizable: true,
            icon: columns[c].icon.clone(),
        })
        .collect();
    let before = (state.columns.order.clone(), state.columns.widths.clone(), state.columns.sort_key.clone(), state.columns.ascending, state.columns.hidden.clone());
    zoned_header(ui, id.with("header"), header_rect, &zones, header_cells, &mut state.columns, options.header, &mut |key| {
        let Some(c) = keys.iter().position(|k| k == key) else { return vec![] };
        let mut values: Vec<String> = rows.iter().map(|r| r.cells.get(c).cloned().unwrap_or_default()).collect();
        values.sort();
        values.dedup();
        values
    });
    let after = (state.columns.order.clone(), state.columns.widths.clone(), state.columns.sort_key.clone(), state.columns.ascending, state.columns.hidden.clone());
    if before != after {
        ui.ctx().request_repaint();
    }
    ui.painter().hline(header_rect.x_range(), header_rect.bottom(), ui.visuals().widgets.noninteractive.bg_stroke);

    // the rows
    let zones = zones_for(&state.columns);
    let scroll = state.columns.scroll;
    let lines = arrange_with(rows, columns, &state.columns, options.tree, &state.expanded, options.keep_order);
    let shown_ids: Vec<String> = lines.iter().map(|l| rows[l.index].id.clone()).collect();
    out.shown = shown_ids.clone();
    let existing: HashSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    state.keep_existing(&existing);

    let body_height = options.max_height.map_or(ui.available_height(), |m| ui.available_height().min(m)).max(row_height);
    let body_top = ui.cursor().min;
    let body_rect = Rect::from_min_size(body_top, vec2(available, body_height));
    // the grid takes the focus on a click and the arrows while it has it
    let focus_id = id.with("focus");
    let focus = ui.interact(body_rect, focus_id, Sense::focusable_noninteractive());
    let focused = ui.memory(|m| m.has_focus(focus_id));
    state.focused = focused;
    if focused {
        lock_arrows(ui, focus_id);
    }
    let (selected_fill, selected_text) = selection_colors(ui, focused);
    let font = TextStyle::Body.resolve(ui.style());
    let text_color = ui.visuals().text_color();
    let faint = ui.visuals().faint_bg_color;

    // the empty space (and the rows' gaps) answers to a click that clears the selection and to the lasso
    let band_sense = ui.interact(body_rect, id.with("band"), if options.multi_select { Sense::click_and_drag() } else { Sense::click() });
    // for each line, at each depth: a later line of that depth comes before any line that is less deep
    let mut continues: Vec<Vec<bool>> = vec![vec![]; if options.tree { lines.len() } else { 0 }];
    if options.tree {
        let mut pending: Vec<bool> = vec![];
        for (n, line) in lines.iter().enumerate().rev() {
            if pending.len() <= line.depth + 1 {
                pending.resize(line.depth + 2, false);
            }
            continues[n] = pending[..=line.depth].to_vec();
            pending[line.depth] = true;
            for deeper in pending.iter_mut().skip(line.depth + 1) {
                *deeper = false;
            }
        }
    }
    let mut area = egui::ScrollArea::vertical().id_salt(id.with("rows")).max_height(body_height).auto_shrink([false, options.max_height.is_some()]);
    if state.scroll_by != 0.0 {
        area = area.vertical_scroll_offset((state.offset_y + std::mem::take(&mut state.scroll_by)).max(0.0));
    }
    if let Some(target) = state.scroll_to.take() {
        if let Some(i) = shown_ids.iter().position(|s| *s == target) {
            area = area.vertical_scroll_offset((i as f32 * row_height - body_height / 2.0).max(0.0));
        }
    }
    // an edit started by the caller (`start_edit`) takes the text of the cell
    if let Some(edit) = state.editing.as_mut().filter(|e| !e.focused && e.text.is_empty()) {
        if let (Some(row), Some(c)) = (rows.iter().find(|r| r.id == edit.row), columns.iter().position(|c| c.key == edit.column)) {
            edit.text = row.cells.get(c).cloned().unwrap_or_default();
        }
    }
    let selected_set: HashSet<String> = state.selected.iter().cloned().collect();
    let mut start_edit: Option<(String, String)> = None;
    let mut commit: Option<(String, String, String)> = None;
    let mut cancel = false;
    let mut clicked: Option<(String, egui::Modifiers)> = None;
    let mut toggled: Option<(String, bool)> = None;
    let output = area.show_rows(ui, row_height, lines.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for position in range {
            let line = &lines[position];
            let row = &rows[line.index];
            let (rect, response) = ui.allocate_exact_size(vec2(available, row_height), Sense::click_and_drag());
            let is_selected = selected_set.contains(&row.id);
            out.row_rects.push((row.id.clone(), rect));
            if response.hovered() {
                out.hovered = Some(row.id.clone());
            }
            if is_selected {
                ui.painter().rect_filled(rect, 0.0, selected_fill);
            } else if options.striped && position % 2 == 1 {
                ui.painter().rect_filled(rect, 0.0, faint);
            } else if response.hovered() {
                ui.painter().rect_filled(rect, 0.0, text_color.gamma_multiply(0.06));
            }
            if options.row_lines {
                let line = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.5));
                ui.painter().hline(rect.x_range(), rect.bottom().round() - 0.5, line);
            }
            let ink = if is_selected { selected_text } else { text_color };
            let ink = if row.dim { ink.gamma_multiply(0.5) } else { ink };
            let mut cell_hit: Option<String> = None;
            for p in &zones.placed {
                let c = order[p.index];
                let column = &columns[c];
                let cell_rect = zones.cell_rect(rect, p, scroll);
                let clip = zones.visible(rect, p, scroll);
                if clip.width() <= 0.0 {
                    continue;
                }
                if response.double_clicked() || response.clicked() {
                    if ui.input(|i| i.pointer.interact_pos()).is_some_and(|pos| clip.contains(pos)) {
                        cell_hit = Some(column.key.clone());
                    }
                }
                let mut left = cell_rect.left() + 8.0;
                let is_first = Some(&column.key) == first_key.as_ref();
                if options.tree && is_first {
                    // the guide lines of the tree: a vertical line under the chevron of each ancestor that goes on while it has
                    // later children, and an elbow to this row
                    let guide = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.9));
                    let painter = ui.painter().with_clip_rect(clip);
                    let x_of = |level: usize| (cell_rect.left() + 8.0 + level as f32 * options.indent + 6.0).round() - 0.5;
                    let middle = rect.center().y.round() - 0.5;
                    for level in 0..line.depth {
                        let x = x_of(level);
                        if level + 1 == line.depth {
                            let bottom = if continues[position].get(level + 1).copied().unwrap_or(false) { rect.bottom() } else { middle };
                            painter.vline(x, rect.top()..=bottom, guide);
                            let end = if line.has_children { x_of(level + 1) - 8.0 } else { x_of(level + 1) + 6.0 };
                            painter.hline(x..=end, middle, guide);
                        } else if continues[position].get(level + 1).copied().unwrap_or(false) {
                            painter.vline(x, rect.y_range(), guide);
                        }
                    }
                    left += line.depth as f32 * options.indent;
                    if line.has_children {
                        let chevron = Rect::from_center_size(pos2(left + 6.0, cell_rect.center().y), vec2(14.0, row_height));
                        let hit = ui.interact(chevron.intersect(clip), id.with(("chevron", &row.id)), Sense::click());
                        let ink = if hit.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
                        let c = chevron.center();
                        let points = if line.open { vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)] } else { vec![c + vec2(-2.0, -4.0), c + vec2(-2.0, 4.0), c + vec2(3.0, 0.0)] };
                        ui.painter().with_clip_rect(clip).add(egui::Shape::convex_polygon(points, ink, egui::Stroke::NONE));
                        if hit.clicked() {
                            toggled = Some((row.id.clone(), !line.open));
                        }
                    }
                    left += 16.0;
                }
                if is_first {
                    if let Some(uri) = &row.icon {
                        let icon = Rect::from_min_size(pos2(left, cell_rect.center().y - 8.0), vec2(16.0, 16.0));
                        if icon.intersects(clip) {
                            egui::Image::new(uri.as_str()).fit_to_exact_size(vec2(16.0, 16.0)).tint(if row.dim { egui::Color32::from_white_alpha(128) } else { egui::Color32::WHITE }).paint_at(ui, icon);
                        }
                        left += 20.0;
                    }
                }
                let editing_here = state.editing.as_ref().is_some_and(|e| e.row == row.id && e.column == column.key);
                let text = row.cells.get(c).map_or("", |s| s.as_str());
                if editing_here {
                    let edit = state.editing.as_mut().expect("editing");
                    let field = Rect::from_min_max(pos2(left - 4.0, cell_rect.top() + 1.0), pos2(cell_rect.right() - 2.0, cell_rect.bottom() - 1.0)).intersect(clip);
                    let mut field_ui = ui.new_child(egui::UiBuilder::new().max_rect(field));
                    let edit_id = id.with(("edit", &row.id, &column.key));
                    let r = field_ui.add(egui::TextEdit::singleline(&mut edit.text).id(edit_id).desired_width(field.width()).margin(vec2(4.0, 2.0)));
                    if !edit.focused {
                        r.request_focus();
                        edit.focused = true;
                    }
                    if r.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                        cancel = true;
                    } else if r.lost_focus() {
                        commit = Some((row.id.clone(), column.key.clone(), edit.text.clone()));
                    }
                    continue;
                }
                if column.custom && cell(ui, row, column, cell_rect.intersect(clip)) {
                    continue;
                }
                let painter = ui.painter().with_clip_rect(clip);
                if column.ty == DataType::Number {
                    painter.text(pos2(cell_rect.right() - 8.0, cell_rect.center().y), Align2::RIGHT_CENTER, text, font.clone(), ink);
                } else {
                    painter.text(pos2(left, cell_rect.center().y), Align2::LEFT_CENTER, text, font.clone(), ink);
                }
                if is_first && !row.badges.is_empty() {
                    let dots = crate::files::dots_width(&row.badges, 9.0);
                    crate::files::paint_dots(ui, &row.badges, clip.right() - dots - 8.0, cell_rect.center().y, 9.0);
                }
            }
            if response.clicked() && state.editing.is_none() {
                clicked = Some((row.id.clone(), ui.input(|i| i.modifiers)));
            }
            if response.double_clicked() {
                let editable = !row.readonly && cell_hit.as_ref().and_then(|k| columns.iter().find(|c| &c.key == k)).is_some_and(|c| c.editable);
                if editable {
                    start_edit = Some((row.id.clone(), cell_hit.clone().unwrap_or_default()));
                } else {
                    out.activated = if is_selected { state.selected.clone() } else { vec![row.id.clone()] };
                }
            }
            if response.secondary_clicked() && !is_selected {
                clicked = Some((row.id.clone(), egui::Modifiers::NONE));
            }
            if options.drag && response.drag_started() {
                let ids = if is_selected { state.selected.clone() } else { vec![row.id.clone()] };
                out.drag_started = Some(ids);
            }
            let concerned = if is_selected { state.selected.clone() } else { vec![row.id.clone()] };
            response.context_menu(|ui| menu(ui, &concerned));
        }
    });
    hscroll_bar(ui, id, &mut state.columns, &zones, output.inner_rect);
    state.offset_y = output.state.clamped_offset().y;

    // the separators of the columns, in line with the ones of the header
    {
        let line = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6));
        let rows_rect = output.inner_rect;
        let painter = ui.painter().with_clip_rect(rows_rect);
        let count = zones.placed.len();
        for (n, p) in zones.placed.iter().enumerate() {
            if n + 1 == count {
                continue;
            }
            let visible = zones.visible(rows_rect, p, scroll);
            if visible.width() > 0.0 {
                painter.vline(visible.right().round() - 0.5, rows_rect.y_range(), line);
            }
        }
    }

    // the lasso
    if options.multi_select {
        let origin = output.inner_rect.min - vec2(0.0, output.state.clamped_offset().y);
        let mods = ui.input(|i| i.modifiers);
        if band_sense.drag_started() {
            if let Some(start) = ui.input(|i| i.pointer.press_origin()) {
                let kept = if mods.command || mods.shift { state.selected.clone() } else { vec![] };
                state.band = Some((start - origin, kept));
            }
        }
        if band_sense.drag_stopped() || !band_sense.dragged() {
            state.band = None;
        } else if let (Some((start, kept)), Some(here)) = (state.band.clone(), ui.input(|i| i.pointer.interact_pos())) {
            let end = here - origin;
            let mut selected = kept;
            for id in rows_in_band(&shown_ids, row_height, start.y.min(end.y), start.y.max(end.y)) {
                if !selected.contains(&id) {
                    selected.push(id);
                }
            }
            if selected != state.selected {
                state.selected = selected;
                state.cursor = state.selected.last().cloned();
                out.selection_changed = true;
            }
            let accent = ui.visuals().selection.bg_fill;
            let band = Rect::from_two_pos(pos2(start.x, start.y) + origin.to_vec2(), here).intersect(output.inner_rect);
            ui.painter().rect(band, 2.0, accent.gamma_multiply(0.25), egui::Stroke::new(1.0, accent), egui::StrokeKind::Inside);
            let rows_area = output.inner_rect;
            let past = if here.y < rows_area.top() { here.y - rows_area.top() } else if here.y > rows_area.bottom() { here.y - rows_area.bottom() } else { 0.0 };
            if past != 0.0 {
                state.scroll_by = past.clamp(-30.0, 30.0);
                ui.ctx().request_repaint();
            }
        }
        if band_sense.clicked() && !mods.command && !mods.shift && state.editing.is_none() && !state.selected.is_empty() {
            focus.request_focus();
            state.selected.clear();
            state.cursor = None;
            out.selection_changed = true;
        }
    }

    // what the frame decided
    if let Some((row, modifiers)) = clicked {
        focus.request_focus();
        out.selection_changed |= state.click(&shown_ids, &row, modifiers, options.multi_select);
        out.clicked = Some(row);
    }
    if let Some((row, open)) = toggled {
        if open {
            state.expanded.insert(row.clone());
        } else {
            state.expanded.remove(&row);
        }
        out.toggled = Some((row, open));
    }
    if let Some((row, column)) = start_edit {
        let text = rows.iter().find(|r| r.id == row).and_then(|r| columns.iter().position(|c| c.key == column).and_then(|c| r.cells.get(c))).cloned().unwrap_or_default();
        state.editing = Some(Edit { row, column, text, focused: false });
    }
    if cancel {
        state.editing = None;
    } else if let Some((row, column, text)) = commit {
        state.editing = None;
        out.edited = Some((row, column, text));
    }
    if focused && state.editing.is_none() {
        out.selection_changed |= keyboard(ui, state, &shown_ids, rows, columns, &lines, options, &mut out, body_height / row_height);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn keyboard(ui: &egui::Ui, state: &mut GridState, shown: &[String], rows: &[GridRow], columns: &[GridColumn], lines: &[Shown], options: &GridOptions, out: &mut GridOutput, page: f32) -> bool {
    let keys = crate::files::read_keys(ui);
    let mut changed = false;
    let multi = options.multi_select;
    let page = page.max(1.0) as i64;
    if keys.down {
        changed |= state.move_cursor(shown, Target::Delta(1), keys.shift, multi);
    }
    if keys.up {
        changed |= state.move_cursor(shown, Target::Delta(-1), keys.shift, multi);
    }
    if keys.page_down {
        changed |= state.move_cursor(shown, Target::Delta(page), keys.shift, multi);
    }
    if keys.page_up {
        changed |= state.move_cursor(shown, Target::Delta(-page), keys.shift, multi);
    }
    if keys.home {
        changed |= state.move_cursor(shown, Target::Home, keys.shift, multi);
    }
    if keys.end {
        changed |= state.move_cursor(shown, Target::End, keys.shift, multi);
    }
    if keys.select_all && multi {
        state.selected = shown.to_vec();
        changed = true;
    }
    if options.tree {
        let cursor_line = state.cursor.as_ref().and_then(|c| shown.iter().position(|s| s == c)).map(|i| &lines[i]);
        if let Some(line) = cursor_line {
            let id = rows[line.index].id.clone();
            if keys.right && line.has_children && !line.open {
                state.expanded.insert(id.clone());
                out.toggled = Some((id, true));
            } else if keys.left && line.open {
                state.expanded.remove(&id);
                out.toggled = Some((id, false));
            } else if keys.left {
                // to the parent
                if let Some(parent) = rows[line.index].parent.clone() {
                    if shown.contains(&parent) {
                        state.select_only(&parent);
                        state.scroll_to = Some(parent);
                        changed = true;
                    }
                }
            }
        }
    }
    if keys.enter && keys.text.is_empty() {
        if keys.parent {
            // Cmd+Up is not Enter
        } else if !state.selected.is_empty() {
            out.activated = state.selected.clone();
        }
    }
    // F2 edits the first editable cell of the row the cursor is on
    if ui.input(|i| i.key_pressed(Key::F2)) {
        if let Some(cursor) = state.cursor.clone() {
            let row = rows.iter().find(|r| r.id == cursor);
            if let (Some(row), Some(column)) = (row, columns.iter().find(|c| c.editable)) {
                if !row.readonly {
                    let c = columns.iter().position(|x| x.key == column.key).unwrap_or(0);
                    state.editing = Some(Edit { row: row.id.clone(), column: column.key.clone(), text: row.cells.get(c).cloned().unwrap_or_default(), focused: false });
                }
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<GridRow> {
        let r = |id: &str, parent: Option<&str>, name: &str, n: &str| GridRow { id: id.into(), cells: vec![name.into(), n.into()], parent: parent.map(String::from), readonly: false, icon: None, expandable: false, badges: vec![], dim: false };
        vec![r("a", None, "Alpha", "3"), r("b", None, "Bravo", "10"), r("a1", Some("a"), "Child 1", "2"), r("a2", Some("a"), "Child 2", "1"), r("b1", Some("b"), "Kid", "5")]
    }

    fn cols() -> Vec<GridColumn> {
        let mut n = GridColumn::new("n", "N");
        n.ty = DataType::Number;
        vec![GridColumn::new("name", "Name"), n]
    }

    fn ids(shown: &[Shown], rows: &[GridRow]) -> Vec<String> {
        shown.iter().map(|s| rows[s.index].id.clone()).collect()
    }

    #[test]
    fn rows_given_in_order_keep_it_and_an_unread_branch_opens() {
        let mut rows = rows();
        rows.reverse();
        let mut state = ColumnsState::new("name");
        state.ascending = true;
        let sorted = arrange_with(&rows, &cols(), &state, false, &HashSet::new(), false);
        let kept = arrange_with(&rows, &cols(), &state, false, &HashSet::new(), true);
        assert_ne!(ids(&sorted, &rows), ids(&kept, &rows));
        assert_eq!(ids(&kept, &rows), rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>());
        let leaf = vec![GridRow { id: "dir".into(), cells: vec!["dir".into(), "1".into()], expandable: true, ..Default::default() }];
        let shown = arrange(&leaf, &cols(), &ColumnsState::new("name"), true, &HashSet::new());
        assert!(shown[0].has_children && !shown[0].open);
        let open: HashSet<String> = ["dir".to_string()].into();
        assert!(arrange(&leaf, &cols(), &ColumnsState::new("name"), true, &open)[0].open);
    }

    #[test]
    fn the_lasso_takes_the_rows_it_touches() {
        let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        assert_eq!(rows_in_band(&ids, 20.0, 25.0, 45.0), vec!["b", "c"]);
        assert_eq!(rows_in_band(&ids, 20.0, 70.0, 10.0), vec!["a", "b", "c", "d"]);
        assert_eq!(rows_in_band(&ids, 20.0, 500.0, 600.0), Vec::<String>::new());
        assert!(rows_in_band(&[], 20.0, 0.0, 10.0).is_empty());
    }

    #[test]
    fn flat_sorts_by_number_not_by_text() {
        let rows = rows();
        let mut state = ColumnsState::new("n");
        let shown = arrange(&rows, &cols(), &state, false, &HashSet::new());
        assert_eq!(ids(&shown, &rows), ["a2", "a1", "a", "b1", "b"]);
        state.ascending = false;
        assert_eq!(ids(&arrange(&rows, &cols(), &state, false, &HashSet::new()), &rows)[0], "b");
    }

    #[test]
    fn tree_shows_only_open_branches() {
        let rows = rows();
        let state = ColumnsState::new("name");
        let closed = arrange(&rows, &cols(), &state, true, &HashSet::new());
        assert_eq!(ids(&closed, &rows), ["a", "b"]);
        assert!(closed[0].has_children && !closed[0].open);
        let open: HashSet<String> = ["a".to_string()].into();
        let shown = arrange(&rows, &cols(), &state, true, &open);
        assert_eq!(ids(&shown, &rows), ["a", "a1", "a2", "b"]);
        assert_eq!(shown[1].depth, 1);
    }

    #[test]
    fn tree_sorts_the_siblings_and_filter_keeps_the_ancestors() {
        let rows = rows();
        let mut state = ColumnsState::new("n");
        let open: HashSet<String> = ["a".to_string()].into();
        assert_eq!(ids(&arrange(&rows, &cols(), &state, true, &open), &rows), ["a", "a2", "a1", "b"]);
        // a filter on the name: only the branch with "Kid", open by itself
        state.filters.entry("name".into()).or_default().text = "Kid".into();
        let shown = arrange(&rows, &cols(), &state, true, &HashSet::new());
        assert_eq!(ids(&shown, &rows), ["b", "b1"]);
        assert_eq!(shown[1].depth, 1);
    }

    #[test]
    fn a_cycle_of_parents_does_not_hang() {
        let mut rows = rows();
        rows[0].parent = Some("a1".into());
        let state = ColumnsState::new("name");
        let _ = arrange(&rows, &cols(), &state, true, &HashSet::new());
    }

    #[test]
    fn click_selects_adds_and_ranges() {
        let shown: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let mut s = GridState::new("name");
        assert!(s.click(&shown, "b", egui::Modifiers::NONE, true));
        assert_eq!(s.selected, ["b"]);
        s.click(&shown, "d", egui::Modifiers { shift: true, ..Default::default() }, true);
        assert_eq!(s.selected, ["b", "c", "d"]);
        s.click(&shown, "a", egui::Modifiers { command: true, ..Default::default() }, true);
        assert_eq!(s.selected.len(), 4);
        s.click(&shown, "c", egui::Modifiers { command: true, ..Default::default() }, true);
        assert!(!s.selected.contains(&"c".to_string()));
        // a single selection ignores Shift and Cmd
        s.click(&shown, "d", egui::Modifiers { shift: true, ..Default::default() }, false);
        assert_eq!(s.selected, ["d"]);
    }

    #[test]
    fn the_cursor_moves_and_stops_at_the_ends() {
        let shown: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let mut s = GridState::new("name");
        s.move_cursor(&shown, Target::Delta(1), false, true);
        assert_eq!(s.cursor.as_deref(), Some("a"));
        s.move_cursor(&shown, Target::Delta(5), false, true);
        assert_eq!(s.cursor.as_deref(), Some("c"));
        s.move_cursor(&shown, Target::Delta(-1), true, true);
        assert_eq!(s.selected, ["c", "b"].iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>());
        s.move_cursor(&shown, Target::Home, false, true);
        assert_eq!(s.selected, ["a"]);
    }

    #[test]
    fn selection_forgets_rows_that_went_away() {
        let mut s = GridState::new("name");
        s.selected = vec!["a".into(), "z".into()];
        s.cursor = Some("z".into());
        s.keep_existing(&["a"].into_iter().collect());
        assert_eq!(s.selected, ["a"]);
        assert_eq!(s.cursor, None);
    }
}
