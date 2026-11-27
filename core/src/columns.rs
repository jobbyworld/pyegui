//! The columns of the file views (FileListView and FileTreeView): the state the user changes with
//! the header (width, order, sort, filters), the type of data of a column (text, number, date,
//! choice) and the header itself: a click sorts, a drag on a title moves the column, a drag on its
//! border resizes it, the funnel opens the filter that fits the type of the column.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use egui::{
    self, pos2, vec2, Color32, CursorIcon, Id, Pos2, Rect, Sense, Stroke, TextStyle, TextWrapMode,
};
use crate::watcher::natural_cmp;

/// The smallest and the largest width of a column
pub const WIDTH_RANGE: (f32, f32) = (40.0, 1200.0);

/// What a column holds: it decides how it sorts and which filter it gets
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DataType {
    /// a text: sorted in natural order, filtered by "contains"
    Text,
    /// a number: sorted by value, filtered by a minimum and a maximum
    Number,
    /// a date (seconds since 1970): sorted by time, filtered by a period
    Date,
    /// one value among a few: filtered by the values kept
    Choice,
}

impl DataType {
    /// The type named by its Python-like name, None when it is not one
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "text" | "str" => DataType::Text,
            "number" | "int" | "float" => DataType::Number,
            "date" | "datetime" => DataType::Date,
            "choice" | "enum" | "bool" => DataType::Choice,
            _ => return None,
        })
    }
}

/// The value of an item in a column, to sort and to filter
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    None,
    Text(String),
    Num(f64),
}

impl Value {
    pub fn compare(&self, other: &Value) -> Ordering {
        match (self, other) {
            (Value::None, Value::None) => Ordering::Equal,
            (Value::None, _) => Ordering::Less,
            (_, Value::None) => Ordering::Greater,
            (Value::Num(a), Value::Num(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (Value::Text(a), Value::Text(b)) => natural_cmp(a, b),
            (Value::Num(_), Value::Text(_)) => Ordering::Less,
            (Value::Text(_), Value::Num(_)) => Ordering::Greater,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Value::Text(t) => Some(t),
            _ => None,
        }
    }
}

/// A number with an optional unit: "12", "1.5k", "2 MB" (units are powers of 1000, like the
/// sizes shown)
pub fn parse_number(text: &str) -> Option<f64> {
    let t = text.trim().to_lowercase().replace(',', ".");
    let split = t.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(t.len());
    let number: f64 = t[..split].parse().ok()?;
    let power = match t[split..].trim() {
        "" | "b" | "o" | "byte" | "bytes" => 0,
        "k" | "kb" | "ko" => 1,
        "m" | "mb" | "mo" => 2,
        "g" | "gb" | "go" => 3,
        "t" | "tb" | "to" => 4,
        _ => return None,
    };
    Some(number * 1000f64.powi(power))
}

/// A date or a date and time of the local time zone, as seconds since 1970, and whether only the
/// day was given ("2024-05-01", "01/05/2024", "2024-05-01 14:30"), or a number of seconds
pub fn parse_date(text: &str) -> Option<(f64, bool)> {
    use chrono::TimeZone;
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    for format in ["%Y-%m-%d", "%d/%m/%Y", "%d.%m.%Y"] {
        if let Ok(day) = chrono::NaiveDate::parse_from_str(t, format) {
            let time = chrono::Local.from_local_datetime(&day.and_hms_opt(0, 0, 0)?).earliest()?;
            return Some((time.timestamp() as f64, true));
        }
    }
    for format in ["%Y-%m-%d %H:%M", "%Y-%m-%d %H:%M:%S", "%d/%m/%Y %H:%M", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(time) = chrono::NaiveDateTime::parse_from_str(t, format) {
            let time = chrono::Local.from_local_datetime(&time).earliest()?;
            return Some((time.timestamp() as f64, false));
        }
    }
    t.parse::<f64>().ok().map(|seconds| (seconds, false))
}

/// What a filter keeps in a column. Which fields count depends on the type of the column: text for
/// a text, min and max for a number or a date, choices for a choice
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Filter {
    pub text: String,
    pub min: String,
    pub max: String,
    /// the values kept; None keeps all of them
    pub choices: Option<BTreeSet<String>>,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.min.trim().is_empty() && self.max.trim().is_empty() && self.choices.is_none()
    }

    pub fn matches(&self, ty: DataType, value: &Value) -> bool {
        match ty {
            DataType::Text => {
                let text = self.text.trim().to_lowercase();
                text.is_empty() || value.text().is_some_and(|v| v.to_lowercase().contains(&text))
            }
            DataType::Choice => match &self.choices {
                None => true,
                Some(set) => value.text().is_some_and(|v| set.contains(v)),
            },
            DataType::Number | DataType::Date => {
                let (min, max) = self.range(ty);
                if min.is_none() && max.is_none() {
                    return true;
                }
                match value {
                    Value::Num(n) => min.is_none_or(|m| *n >= m) && max.is_none_or(|m| *n <= m),
                    _ => false,
                }
            }
        }
    }

    /// The bounds of the range. A last day is kept up to its end
    fn range(&self, ty: DataType) -> (Option<f64>, Option<f64>) {
        match ty {
            DataType::Date => (
                parse_date(&self.min).map(|(t, _)| t),
                parse_date(&self.max).map(|(t, day)| if day { t + 86_399.0 } else { t }),
            ),
            _ => (parse_number(&self.min), parse_number(&self.max)),
        }
    }
}

/// The columns of a view as the user left them
pub struct ColumnsState {
    /// the keys of the columns, in the order wanted; the missing ones follow in their default order
    pub order: Vec<String>,
    /// the widths the user chose; a column without one has its default width
    pub widths: HashMap<String, f32>,
    pub filters: BTreeMap<String, Filter>,
    pub sort_key: String,
    pub ascending: bool,
    /// the column being moved
    dragging: Option<String>,
    /// the columns pinned at the left and at the right, by key
    pub pinned_left: Vec<String>,
    pub pinned_right: Vec<String>,
    /// how far the columns that are not pinned are scrolled
    pub scroll: f32,
    /// the menu of a title (a column key and where it was asked)
    menu: Option<(String, Pos2)>,
    /// the last text of `filter` given by the Python code. The code often gives it again at every
    /// frame (a search field): only a new text replaces the filter the user set in the header
    given_name_text: Option<String>,
    /// the column whose icon was clicked since the last `take_icon_click`
    pub icon_clicked: Option<String>,
    /// the columns the user hid with the eye of the settings (by key)
    pub hidden: Vec<String>,
    /// every column the view can show, hidden or not, as (key, title): given by the view at each frame
    pub catalog: Vec<(String, String)>,
    /// the popup of the settings is open
    settings_open: bool,
    /// the column being moved in the popup of the settings
    settings_drag: Option<String>,
}

impl ColumnsState {
    pub fn new(sort_key: &str) -> Self {
        ColumnsState {
            order: vec![],
            widths: HashMap::new(),
            filters: BTreeMap::new(),
            sort_key: sort_key.to_string(),
            ascending: true,
            dragging: None,
            pinned_left: vec![],
            pinned_right: vec![],
            scroll: 0.0,
            menu: None,
            given_name_text: None,
            icon_clicked: None,
            hidden: vec![],
            catalog: vec![],
            settings_open: false,
            settings_drag: None,
        }
    }

    /// The text filter of the name, also the `filter` of the views
    pub fn name_text(&self) -> &str {
        self.filters.get("name").map_or("", |f| f.text.as_str())
    }

    pub fn set_name_text(&mut self, text: String) {
        if self.given_name_text.as_ref() == Some(&text) {
            return;
        }
        self.given_name_text = Some(text.clone());
        if text.is_empty() {
            self.filters.remove("name");
        } else {
            self.filters.entry("name".into()).or_default().text = text;
        }
    }

    /// The keys in the order wanted: the default order, changed by the user's order
    pub fn ordered(&self, default: &[String]) -> Vec<String> {
        let mut keys: Vec<String> = self.order.iter().filter(|k| default.contains(k)).cloned().collect();
        keys.dedup();
        for key in default {
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
        keys
    }

    /// A click on a title: sorts by it, a second click reverses the order
    pub fn toggle_sort(&mut self, key: &str) {
        if self.sort_key == key {
            self.ascending = !self.ascending;
        } else {
            self.sort_key = key.to_string();
            self.ascending = true;
        }
    }


    pub fn set_widths(&mut self, widths: HashMap<String, f32>) {
        self.widths = widths.into_iter().map(|(k, w)| (k, w.clamp(WIDTH_RANGE.0, WIDTH_RANGE.1))).collect();
    }


}

/// A cell of the header, placed by the view
/// A small SVG icon at the left of the title of a column (drawn in the color of the title)
#[derive(Clone, Debug)]
pub struct ColumnIcon {
    /// tells it from the other icons (a file name, say)
    pub name: String,
    pub bytes: std::sync::Arc<[u8]>,
}

#[derive(Clone)]
pub struct Cell {
    pub key: String,
    pub title: String,
    pub rect: Rect,
    pub ty: DataType,
    pub sortable: bool,
    pub filterable: bool,
    /// can be moved by a drag on its title
    pub movable: bool,
    pub resizable: bool,
    pub icon: Option<ColumnIcon>,
}

#[derive(Clone, Copy)]
pub struct HeaderOptions {
    pub resizable: bool,
    pub pinnable: bool,
    pub reorderable: bool,
    pub filterable: bool,
    /// a drag of items out of the window starts a drag of the files to the Finder
    pub drag_out: bool,
    /// the columns are at the right end and grow to the left (the tree)
    pub grow_left: bool,
    /// the cells of the header are filled with the tint of the bars (the one of the inactive tabs)
    pub tinted: bool,
    /// the settings icon at the right of the header (needs the view to give its catalog of columns)
    pub settings: bool,
}

/// The size of the icon at the left of a title
pub const ICON_SIZE: f32 = 13.0;

/// The height of the header of the columns: the one of the tab bars of the docks
pub const HEADER_H: f32 = 40.0;

/// The gap above the tabs of a bar of tabs, which the titles of a tinted header keep too
pub const TAB_TOP_GAP: f32 = 6.0;

/// Where a tinted header leaves the rectangle of the last one drawn (read by the pane that holds it)
pub const HEADER_BAND: &str = "pyegui-header-band";
/// Told by the file views (a `bool` in the temp data of the context): rows go under the pinned header.
pub const HEADER_SCROLLED: &str = "pyegui-header-scrolled";

fn galley(ui: &egui::Ui, text: &str, width: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    egui::WidgetText::from(egui::RichText::new(text).small().strong().color(color)).into_galley(
        ui,
        Some(TextWrapMode::Truncate),
        width.max(1.0),
        TextStyle::Body,
    )
}

fn paint_sort_arrow(ui: &egui::Ui, at: Pos2, ascending: bool, color: Color32) {
    let s = 3.5;
    let points = if ascending {
        vec![at + vec2(-s, s * 0.6), at + vec2(s, s * 0.6), at + vec2(0.0, -s * 0.6)]
    } else {
        vec![at + vec2(-s, -s * 0.6), at + vec2(s, -s * 0.6), at + vec2(0.0, s * 0.6)]
    };
    ui.painter().add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

fn paint_funnel(ui: &egui::Ui, center: Pos2, color: Color32, filled: bool) {
    let body = vec![center + vec2(-5.0, -4.5), center + vec2(5.0, -4.5), center + vec2(0.8, 0.5), center + vec2(0.8, 4.5), center + vec2(-0.8, 3.5), center + vec2(-0.8, 0.5)];
    let fill = if filled { color } else { Color32::TRANSPARENT };
    ui.painter().add(egui::Shape::convex_polygon(body, fill, Stroke::new(1.2, color)));
}

/// The header of the columns. A click on a title sorts, a drag on a title moves the column, a drag
/// on the border resizes it (a double click gives it its default width back), the funnel opens the
/// filter. `choices` gives the values of a column of choices, called only while its filter is open
pub fn header(
    ui: &mut egui::Ui,
    id: Id,
    cells: &[Cell],
    state: &mut ColumnsState,
    opts: HeaderOptions,
    choices: &mut dyn FnMut(&str) -> Vec<String>,
) {
    let line = Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
    let accent = ui.visuals().selection.bg_fill;
    let pointer = ui.input(|i| i.pointer.interact_pos());

    if opts.tinted {
        // the band of the header is told to the pane that holds it, which leaves its sheet out of it
        let band = cells.iter().map(|c| c.rect).reduce(|a, b| a.union(b));
        if let Some(band) = band {
            ui.ctx().data_mut(|d| {
                let key = Id::unique(HEADER_BAND);
                let all = d.get_temp::<Rect>(key).map_or(band, |old| old.union(band));
                d.insert_temp(key, all);
            });
        }
    }
    for (i, cell) in cells.iter().enumerate() {
        let rect = cell.rect;
        let movable = opts.reorderable && cell.movable;
        let sense = if movable {
            Sense::click_and_drag()
        } else if cell.sortable {
            Sense::click()
        } else {
            Sense::hover()
        };
        let body = ui.interact(rect, id.with(("cell", &cell.key)), sense);
        if opts.tinted && ui.rect_contains_pointer(rect) {
            // like an inactive tab: no fill, a light one under the pointer
            let fill = if ui.visuals().dark_mode { Color32::from_white_alpha(14) } else { Color32::from_white_alpha(110) };
            ui.painter().rect_filled(Rect::from_min_max(rect.min + vec2(0.0, TAB_TOP_GAP), rect.max), egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, fill);
        }
        if body.clicked() && cell.sortable {
            state.toggle_sort(&cell.key);
        }
        if movable && body.drag_started() {
            state.dragging = Some(cell.key.clone());
        }

        let active_sort = cell.sortable && state.sort_key == cell.key;
        let filter_active = state.filters.get(&cell.key).is_some_and(|f| !f.is_empty());
        let popup_id = id.with(("popup", &cell.key));
        let popup_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
        let filterable = opts.filterable && cell.filterable;
        let hovered = ui.rect_contains_pointer(rect);

        // the title and the arrow of the sort
        let color = if opts.tinted {
            // the color of the title of an inactive tab, whatever the sort (the arrow tells it)
            let v = ui.visuals();
            v.weak_text_color().gamma_multiply(if v.dark_mode { 1.0 } else { 0.7 })
        } else if active_sort || (body.hovered() && cell.sortable) {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        let icon_room = if cell.icon.is_some() { ICON_SIZE + 4.0 } else { 0.0 };
        let reserved = 26.0 + if filterable { 18.0 } else { 0.0 } + icon_room;
        let text = galley(ui, &cell.title, rect.width() - reserved, color);
        // a tinted header is read like a bar of tabs: its titles sit where the titles of the tabs do,
        // under the gap above the tabs
        let middle = if opts.tinted { rect.center().y + TAB_TOP_GAP / 2.0 } else { rect.center().y };
        if let Some(icon) = &cell.icon {
            let spot = Rect::from_center_size(pos2(rect.left() + 8.0 + ICON_SIZE / 2.0, middle), vec2(ICON_SIZE, ICON_SIZE));
            // the extension tells the loaders it is an SVG
            egui::Image::from_bytes(format!("bytes://column-icon/{}.svg", icon.name), icon.bytes.to_vec())
                .tint(color)
                .fit_to_exact_size(vec2(ICON_SIZE, ICON_SIZE))
                .paint_at(ui, spot);
            // a click on the icon is told to the application, which shows what the column is about
            let hit = ui.interact(spot.expand(3.0), id.with(("icon", &cell.key)), Sense::click());
            if hit.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if hit.clicked() {
                state.icon_clicked = Some(cell.key.clone());
            }
        }
        let pos = pos2(rect.left() + 8.0 + icon_room, middle - text.size().y / 2.0);
        let text_right = pos.x + text.size().x;
        ui.painter().with_clip_rect(rect).galley(pos, text, color);
        if active_sort {
            paint_sort_arrow(ui, pos2(text_right + 9.0, middle), state.ascending, color);
        }
        if i > 0 {
            ui.painter().vline(rect.left(), rect.y_range().shrink(4.0), line);
        }

        // the funnel of the filter
        if filterable {
            let spot = Rect::from_center_size(pos2(rect.right() - 14.0, if opts.tinted { rect.center().y + TAB_TOP_GAP / 2.0 } else { rect.center().y }), vec2(18.0, 18.0));
            let funnel = ui.interact(spot, id.with(("funnel", &cell.key)), Sense::click());
            if hovered || filter_active || popup_open {
                if funnel.hovered() || popup_open {
                    ui.painter().rect_filled(spot, crate::style::fit_radius(ui.visuals(), 4.0), ui.visuals().widgets.hovered.weak_bg_fill);
                }
                let color = if filter_active { ui.visuals().hyperlink_color } else { ui.visuals().weak_text_color() };
                paint_funnel(ui, spot.center(), color, filter_active);
            }
            let popup = egui::Popup::menu(&funnel)
                .style(crate::style::menu_style())
                .id(popup_id)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
            popup.show(|ui| {
                let values = if cell.ty == DataType::Choice { choices(&cell.key) } else { vec![] };
                let first = ui.data_mut(|d| d.get_temp::<bool>(popup_id).is_none());
                ui.data_mut(|d| d.insert_temp(popup_id, true));
                let filter = state.filters.entry(cell.key.clone()).or_default();
                filter_ui(ui, cell.ty, filter, &values, first);
            });
            if !popup_open {
                ui.data_mut(|d| d.remove::<bool>(popup_id));
            }
        }

        // the border: resizes
        if opts.resizable && cell.resizable {
            let x = if opts.grow_left { rect.left() } else { rect.right() };
            let handle = Rect::from_x_y_ranges((x - 3.0)..=(x + 3.0), rect.y_range());
            let grip = ui.interact(handle, id.with(("grip", &cell.key)), Sense::click_and_drag());
            if grip.hovered() || grip.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                ui.painter().vline(x, rect.y_range().shrink(3.0), Stroke::new(2.0, accent));
            }
            if grip.dragged() {
                let dx = grip.drag_delta().x * if opts.grow_left { -1.0 } else { 1.0 };
                let width = (rect.width() + dx).clamp(WIDTH_RANGE.0, WIDTH_RANGE.1);
                state.widths.insert(cell.key.clone(), width);
            }
            if grip.double_clicked() {
                state.widths.remove(&cell.key);
            }
        }
    }
    state.filters.retain(|_, f| !f.is_empty());

    // a column being moved: a bar shows where it lands
    if let Some(key) = state.dragging.clone().filter(|k| cells.iter().any(|c| &c.key == k)) {
        let from = cells.iter().position(|c| c.key == key);
        let targets: Vec<usize> = (0..cells.len()).filter(|i| cells[*i].movable).collect();
        let released = !ui.input(|i| i.pointer.any_down());
        match (from, pointer, targets.first(), targets.last()) {
            (Some(from), Some(pointer), Some(&first), Some(&last)) => {
                let to = targets
                    .iter()
                    .copied()
                    .find(|i| pointer.x < cells[*i].rect.right())
                    .unwrap_or(last)
                    .clamp(first, last);
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                ui.painter().rect_filled(cells[from].rect, crate::style::fit_radius(ui.visuals(), 0.0), ui.visuals().text_color().gamma_multiply(0.08));
                if to != from {
                    let x = if to > from { cells[to].rect.right() } else { cells[to].rect.left() };
                    ui.painter().vline(x, cells[to].rect.y_range(), Stroke::new(2.0, accent));
                }
                if released {
                    if to != from {
                        let mut keys: Vec<String> = cells.iter().map(|c| c.key.clone()).collect();
                        let moved = keys.remove(from);
                        keys.insert(to, moved);
                        let rest: Vec<String> = state.order.iter().filter(|k| !keys.contains(k)).cloned().collect();
                        keys.extend(rest);
                        state.order = keys;
                    }
                    state.dragging = None;
                }
            }
            _ => state.dragging = None,
        }
        if released {
            state.dragging = None;
        }
    }
}

/// The filter of a column, in its popup
fn filter_ui(ui: &mut egui::Ui, ty: DataType, filter: &mut Filter, values: &[String], first: bool) {
    ui.set_min_width(200.0);
    match ty {
        DataType::Text => {
            let edit = ui.add(egui::TextEdit::singleline(&mut filter.text).hint_text("Contient…").desired_width(200.0));
            if first {
                edit.request_focus();
            }
        }
        DataType::Number => {
            ui.label("Entre");
            range_edits(ui, filter, "min, ex. 10 Ko", "max", first);
        }
        DataType::Date => {
            // the calendar and its field in French ("cette semaine", "depuis lundi"): the filter keeps the ends as
            // YYYY-MM-DD, an empty end is open
            let today = chrono::Local::now().date_naive();
            let day = |d: Option<chrono::NaiveDate>| d.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
            let mut range = crate::date_range::DateRange {
                start: crate::date_input::parse_date(&filter.min, today).filter(|_| !filter.min.trim().is_empty()),
                end: crate::date_input::parse_date(&filter.max, today).filter(|_| !filter.max.trim().is_empty()),
            };
            if crate::date_range::date_range_inline(ui, "column-filter", &mut range) {
                filter.min = day(range.start);
                filter.max = day(range.end);
            }
        }
        DataType::Choice => {
            ui.horizontal(|ui| {
                if ui.small_button("Tout").clicked() {
                    filter.choices = None;
                }
                if ui.small_button("Aucun").clicked() {
                    filter.choices = Some(BTreeSet::new());
                }
            });
            egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                for value in values {
                    let mut kept = filter.choices.as_ref().is_none_or(|set| set.contains(value));
                    if ui.checkbox(&mut kept, value).changed() {
                        let mut set: BTreeSet<String> =
                            filter.choices.clone().unwrap_or_else(|| values.iter().cloned().collect());
                        if kept {
                            set.insert(value.clone());
                        } else {
                            set.remove(value);
                        }
                        filter.choices = if values.iter().all(|v| set.contains(v)) { None } else { Some(set) };
                    }
                }
            });
        }
    }
    if !filter.is_empty() {
        ui.separator();
        if ui.button("Effacer").clicked() {
            *filter = Filter::default();
        }
    }
}

fn range_edits(ui: &mut egui::Ui, filter: &mut Filter, min_hint: &str, max_hint: &str, first: bool) {
    let edit = ui.add(egui::TextEdit::singleline(&mut filter.min).hint_text(min_hint).desired_width(200.0));
    if first {
        edit.request_focus();
    }
    ui.add(egui::TextEdit::singleline(&mut filter.max).hint_text(max_hint).desired_width(200.0));
}


/// A column placed in its zone: 0 pinned at the left, 1 scrolling, 2 pinned at the right
#[derive(Clone)]
pub struct Placed {
    /// the position of the column in the lists given to `Zones::new`
    pub index: usize,
    /// the left of the column in its zone
    pub rel: f32,
    pub width: f32,
    pub zone: u8,
}

/// The three zones of a view with pinned columns: what is pinned at the left, what scrolls
/// horizontally between them, what is pinned at the right
#[derive(Clone)]
pub struct Zones {
    pub placed: Vec<Placed>,
    pub left: f32,
    pub scrolling: f32,
    pub right: f32,
    /// the room of the zone that scrolls
    pub room: f32,
}

impl Zones {
    /// `keys` and `widths` are the columns in the order shown. `always_left` is a column that is
    /// always in the first zone (the name of a tree), and must come first
    pub fn new(keys: &[String], widths: &[f32], state: &ColumnsState, available: f32, always_left: Option<&str>) -> Self {
        let zone_of = |key: &String| -> u8 {
            if always_left == Some(key.as_str()) || state.pinned_left.contains(key) {
                0
            } else if state.pinned_right.contains(key) {
                2
            } else {
                1
            }
        };
        let mut placed = vec![];
        let mut used = [0.0f32; 3];
        for zone in 0..3u8 {
            for (index, key) in keys.iter().enumerate() {
                if zone_of(key) == zone {
                    placed.push(Placed { index, rel: used[zone as usize], width: widths[index], zone });
                    used[zone as usize] += widths[index];
                }
            }
        }
        let room = (available - used[0] - used[2]).max(0.0);
        Zones { placed, left: used[0], scrolling: used[1], right: used[2], room }
    }

    pub fn max_scroll(&self) -> f32 {
        (self.scrolling - self.room).max(0.0)
    }

    pub fn zone_rect(&self, table: Rect, zone: u8) -> Rect {
        match zone {
            0 => Rect::from_min_size(table.min, vec2(self.left, table.height())),
            1 => Rect::from_min_size(pos2(table.left() + self.left, table.top()), vec2(self.room, table.height())),
            _ => Rect::from_min_max(pos2((table.right() - self.right).max(table.left()), table.top()), table.max),
        }
    }

    /// Where a column is in a row (or in the header) that is `table`, with the columns that are
    /// not pinned scrolled by `scroll`
    pub fn cell_rect(&self, table: Rect, p: &Placed, scroll: f32) -> Rect {
        let zone = self.zone_rect(table, p.zone);
        let x = zone.left() + p.rel - if p.zone == 1 { scroll } else { 0.0 };
        Rect::from_min_size(pos2(x, table.top()), vec2(p.width, table.height()))
    }

    /// The part of a cell that can be seen: the cell cut by its zone
    pub fn visible(&self, table: Rect, p: &Placed, scroll: f32) -> Rect {
        self.cell_rect(table, p, scroll).intersect(self.zone_rect(table, p.zone))
    }
}

/// The header of a view with pinned columns: one `header` per zone (the titles of the columns that
/// scroll are cut by their zone), and the menu that pins a column, on a right click on a title.
/// `cells` are in the order given to `Zones::new`; their rect is set here
#[allow(clippy::too_many_arguments)]
pub fn zoned_header(
    ui: &mut egui::Ui,
    id: Id,
    rect: Rect,
    zones: &Zones,
    mut cells: Vec<Cell>,
    state: &mut ColumnsState,
    opts: HeaderOptions,
    choices: &mut dyn FnMut(&str) -> Vec<String>,
) {
    let scroll = state.scroll;
    for p in &zones.placed {
        cells[p.index].rect = zones.cell_rect(rect, p, scroll);
    }
    let mut hits: Vec<(String, Rect, bool)> = vec![];
    for zone in 0..3u8 {
        let group: Vec<Cell> = zones.placed.iter().filter(|p| p.zone == zone).map(|p| cells[p.index].clone()).collect();
        if group.is_empty() {
            continue;
        }
        let zone_rect = zones.zone_rect(rect, zone);
        let mut zone_ui = ui.new_child(egui::UiBuilder::new().max_rect(zone_rect));
        zone_ui.shrink_clip_rect(zone_rect);
        header(&mut zone_ui, id.with(("zone", zone)), &group, state, opts, choices);
        for c in &group {
            // a column that cannot move (the name of a tree, always at the left) has no menu
            if c.movable {
                hits.push((c.key.clone(), c.rect.intersect(zone_rect), true));
            }
        }
    }
    // the limits of the zones, once something scrolls under the pinned columns
    if zones.max_scroll() > 0.0 {
        let line = Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
        if zones.left > 0.0 {
            ui.painter().vline(rect.left() + zones.left, rect.y_range(), line);
        }
        if zones.right > 0.0 {
            ui.painter().vline(rect.right() - zones.right, rect.y_range(), line);
        }
    }
    pin_menu(ui, id, state, opts, &hits);
    settings_menu(ui, id, rect, state, opts);
}

/// The menu of a title: pin the column at the left or at the right, or unpin it
fn pin_menu(ui: &mut egui::Ui, id: Id, state: &mut ColumnsState, opts: HeaderOptions, hits: &[(String, Rect, bool)]) {
    if !opts.pinnable {
        return;
    }
    let right_click = ui.input(|i| i.pointer.secondary_clicked());
    if right_click {
        if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
            if let Some((key, _, _)) = hits.iter().find(|(_, r, _)| r.contains(p)) {
                state.menu = Some((key.clone(), p));
            }
        }
    }
    let Some((key, at)) = state.menu.clone() else { return };
    let mut close = false;
    let area = egui::Area::new(id.with("pin_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            crate::popup_style::menu_frame(ui).show(ui, |ui| {
                ui.set_min_width(140.0);
                let pinned = state.pinned_left.contains(&key) || state.pinned_right.contains(&key);
                let unpin = |state: &mut ColumnsState| {
                    state.pinned_left.retain(|k| *k != key);
                    state.pinned_right.retain(|k| *k != key);
                };
                if ui.button("Épingler à gauche").clicked() {
                    unpin(state);
                    state.pinned_left.push(key.clone());
                    close = true;
                }
                if ui.button("Épingler à droite").clicked() {
                    unpin(state);
                    state.pinned_right.push(key.clone());
                    close = true;
                }
                if ui.add_enabled(pinned, egui::Button::new("Détacher")).clicked() {
                    unpin(state);
                    close = true;
                }
            });
        });
    if close || (!right_click && area.response.clicked_elsewhere()) {
        state.menu = None;
    }
}

/// The columns that are not pinned scroll with the wheel (or Shift and the wheel) over `area`
pub fn hscroll_wheel(ui: &egui::Ui, state: &mut ColumnsState, zones: &Zones, area: Rect) {
    let wheel = ui.input(|i| i.smooth_scroll_delta.x);
    if wheel != 0.0 && ui.rect_contains_pointer(area) {
        state.scroll -= wheel;
    }
    state.scroll = state.scroll.clamp(0.0, zones.max_scroll());
}

/// A thin scroll bar at the bottom of `area`, under the columns that scroll
pub fn hscroll_bar(ui: &mut egui::Ui, id: Id, state: &mut ColumnsState, zones: &Zones, area: Rect) {
    let max = zones.max_scroll();
    if max <= 0.0 || zones.room < 40.0 {
        return;
    }
    let track = Rect::from_min_size(pos2(area.left() + zones.left, area.bottom() - 9.0), vec2(zones.room, 6.0));
    let thumb_width = (track.width() * track.width() / zones.scrolling).clamp(24.0, track.width());
    let travel = (track.width() - thumb_width).max(1.0);
    let thumb = Rect::from_min_size(pos2(track.left() + state.scroll / max * travel, track.top()), vec2(thumb_width, track.height()));
    let drag = ui.interact(thumb.expand(3.0), id.with("hbar"), Sense::drag());
    if drag.dragged() {
        state.scroll = (state.scroll + drag.drag_delta().x * max / travel).clamp(0.0, max);
    }
    let text = ui.visuals().text_color();
    let color = text.gamma_multiply(if drag.hovered() || drag.dragged() { 0.5 } else { 0.3 });
    let painter = ui.painter().with_clip_rect(area);
    painter.rect_filled(track, 3.0, text.gamma_multiply(0.08));
    painter.rect_filled(thumb, 3.0, color);
}

const ICON_SLIDERS: &[u8] = include_bytes!("../../examples/icons/sliders.svg");
const ICON_PIN: &[u8] = include_bytes!("../../examples/icons/pin.svg");
const ICON_EYE: &[u8] = include_bytes!("../../examples/icons/eye.svg");
const ICON_EYE_OFF: &[u8] = include_bytes!("../../examples/icons/eye-off.svg");

fn paint_svg(ui: &egui::Ui, uri: &'static str, bytes: &'static [u8], rect: Rect, color: Color32) {
    egui::Image::from_bytes(uri, bytes).tint(color).fit_to_exact_size(rect.size()).paint_at(ui, rect);
}

/// The settings of the columns: an icon at the right end of the header, seen while the mouse is over
/// the header, which opens a popup to order the columns by drag and drop (the grip), to hide them
/// (the eye) and to pin them (the pin: at the left, at the right, or not pinned)
fn settings_menu(ui: &mut egui::Ui, id: Id, header: Rect, state: &mut ColumnsState, opts: HeaderOptions) {
    if !opts.settings || state.catalog.is_empty() {
        return;
    }
    let was_open = state.settings_open;
    let hovered = ui.rect_contains_pointer(header);
    let slot = Rect::from_min_size(pos2(header.right() - 26.0, header.center().y - 11.0), vec2(22.0, 22.0));
    let gear = ui.interact(slot, id.with("settings-icon"), Sense::click());
    if hovered || was_open || gear.hovered() {
        let visuals = ui.visuals();
        let fill = if was_open || gear.hovered() { visuals.widgets.hovered.bg_fill } else { visuals.widgets.inactive.weak_bg_fill };
        ui.painter().rect_filled(slot, crate::style::fit_radius(ui.visuals(), 5.0), fill);
        let ink = visuals.text_color();
        paint_svg(ui, "bytes://pyegui-columns-sliders.svg", ICON_SLIDERS, Rect::from_center_size(slot.center(), vec2(14.0, 14.0)), ink);
    }
    gear.clone().on_hover_text("Affichage des colonnes");
    if gear.clicked() {
        state.settings_open = !was_open;
    }
    if !was_open {
        return;
    }
    let keys: Vec<String> = state.catalog.iter().map(|(k, _)| k.clone()).collect();
    let mut order = state.ordered(&keys);
    let title_of = |key: &str| state.catalog.iter().find(|(k, _)| k == key).map(|(_, t)| t.clone()).unwrap_or_else(|| key.to_owned());
    let titles: Vec<String> = order.iter().map(|k| title_of(k)).collect();
    let (row_h, width) = (26.0, 260.0);
    let mut rows: Vec<Rect> = vec![];
    let area = egui::Area::new(id.with("settings-popup"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(slot.right_bottom() + vec2(0.0, 4.0))
        .show(ui.ctx(), |ui| {
            crate::popup_style::menu_frame(ui).show(ui, |ui| {
                ui.set_width(width);
                ui.label(egui::RichText::new("Colonnes").small().strong().weak());
                ui.add_space(2.0);
                let accent = ui.visuals().selection.bg_fill;
                let ink = ui.visuals().text_color();
                let weak = ink.gamma_multiply(0.45);
                for (index, key) in order.iter().enumerate() {
                    let (row, _) = ui.allocate_exact_size(vec2(width, row_h), Sense::hover());
                    rows.push(row);
                    let hidden = state.hidden.contains(key);
                    let dragged = state.settings_drag.as_deref() == Some(key.as_str());
                    if dragged {
                        ui.painter().rect_filled(row, crate::style::fit_radius(ui.visuals(), 5.0), accent.gamma_multiply(0.35));
                    } else if ui.rect_contains_pointer(row) && state.settings_drag.is_none() {
                        ui.painter().rect_filled(row, crate::style::fit_radius(ui.visuals(), 5.0), ui.visuals().widgets.hovered.weak_bg_fill);
                    }
                    // the grip: six dots, the handle of the drag and drop
                    let grip = Rect::from_min_size(row.min, vec2(22.0, row_h));
                    let grip_response = ui.interact(grip, id.with(("settings-grip", key)), Sense::drag());
                    if grip_response.drag_started() {
                        state.settings_drag = Some(key.clone());
                    }
                    if grip_response.hovered() || dragged {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }
                    for dx in [-2.5f32, 2.5] {
                        for dy in [-4.0f32, 0.0, 4.0] {
                            ui.painter().circle_filled(grip.center() + vec2(dx, dy), 1.2, weak);
                        }
                    }
                    // the eye: the name always stays
                    let eye = Rect::from_center_size(pos2(row.right() - 18.0, row.center().y), vec2(22.0, 22.0));
                    let can_hide = key != "name";
                    let eye_response = ui.interact(eye, id.with(("settings-eye", key)), if can_hide { Sense::click() } else { Sense::hover() });
                    if eye_response.clicked() {
                        if hidden {
                            state.hidden.retain(|k| k != key);
                        } else {
                            state.hidden.push(key.clone());
                        }
                    }
                    let (uri, bytes) = if hidden { ("bytes://pyegui-columns-eye-off.svg", ICON_EYE_OFF) } else { ("bytes://pyegui-columns-eye.svg", ICON_EYE) };
                    let eye_color = if !can_hide { weak.gamma_multiply(0.6) } else if hidden { weak } else { ink };
                    paint_svg(ui, uri, bytes, Rect::from_center_size(eye.center(), vec2(15.0, 15.0)), eye_color);
                    eye_response.on_hover_text(if !can_hide { "Toujours visible" } else if hidden { "Afficher la colonne" } else { "Masquer la colonne" });
                    // the pin: not pinned, then at the left, then at the right
                    let pin = Rect::from_center_size(pos2(eye.left() - 16.0, row.center().y), vec2(22.0, 22.0));
                    let at_left = state.pinned_left.contains(key);
                    let at_right = state.pinned_right.contains(key);
                    let pin_response = ui.interact(pin, id.with(("settings-pin", key)), Sense::click());
                    if pin_response.clicked() {
                        state.pinned_left.retain(|k| k != key);
                        state.pinned_right.retain(|k| k != key);
                        if !at_left && !at_right {
                            state.pinned_left.push(key.clone());
                        } else if at_left {
                            state.pinned_right.push(key.clone());
                        }
                    }
                    let pin_color = if at_left || at_right { accent } else if pin_response.hovered() { ink } else { weak };
                    paint_svg(ui, "bytes://pyegui-columns-pin.svg", ICON_PIN, Rect::from_center_size(pin.center(), vec2(14.0, 14.0)), pin_color);
                    if at_left || at_right {
                        let mark = if at_left { "◂" } else { "▸" };
                        let at = pin.center() + vec2(if at_left { -10.0 } else { 10.0 }, 0.0);
                        ui.painter().text(at, egui::Align2::CENTER_CENTER, mark, egui::FontId::proportional(10.0), accent);
                    }
                    pin_response.on_hover_text(if at_left {
                        "Épinglée à gauche : cliquer pour épingler à droite"
                    } else if at_right {
                        "Épinglée à droite : cliquer pour détacher"
                    } else {
                        "Épingler à gauche"
                    });
                    // the title
                    let text_rect = Rect::from_min_max(pos2(grip.right() + 2.0, row.top()), pos2(pin.left() - 14.0, row.bottom()));
                    let color = if hidden { weak } else { ink };
                    let galley = ui.painter().layout(titles[index].clone(), egui::FontId::proportional(13.5), color, text_rect.width());
                    ui.painter().with_clip_rect(text_rect).galley(pos2(text_rect.left(), row.center().y - galley.size().y / 2.0), galley, color);
                }
            });
        });
    // the drag: the column follows the row under the mouse
    if let Some(key) = state.settings_drag.clone() {
        if ui.input(|i| i.pointer.any_down()) {
            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                let under = if p.y < rows.first().map_or(0.0, |r| r.top()) {
                    Some(0)
                } else if p.y > rows.last().map_or(0.0, |r| r.bottom()) {
                    Some(order.len().saturating_sub(1))
                } else {
                    rows.iter().position(|r| r.y_range().contains(p.y))
                };
                if let (Some(to), Some(from)) = (under, order.iter().position(|k| *k == key)) {
                    if to != from {
                        let moved = order.remove(from);
                        order.insert(to, moved);
                        state.order = order;
                    }
                }
            }
            ui.ctx().request_repaint();
        } else {
            state.settings_drag = None;
        }
    }
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if escape || (!gear.clicked() && state.settings_drag.is_none() && area.response.clicked_elsewhere()) {
        state.settings_open = false;
    }
}
