//! File views: FileListView, FileIconView and FileTreeView show folders of the disk, with
//! selection, keyboard, sorting and native icons. Their folders are listed through a FileWatcher,
//! so a change made by any program shows at the next frame.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, EventFilter, FontId, Id, Key, Pos2, Rect, Response, Sense,
    Stroke, TextStyle, TextWrapMode,
};
use egui_ltreeview::{Action, NodeBuilder, TreeViewBuilder, TreeViewState};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use crate::columns::{self, hscroll_bar, hscroll_wheel, Cell, ColumnsState, DataType, Filter, HeaderOptions, Value, Zones};
use crate::icons::icon_uri;
use crate::watcher::{default_core, entry, expand, lock, natural_cmp, Core, Entry, FileWatcher};
use crate::style::extract_color;
use crate::{current_ui, kwarg, run_nested_call, UI};

/// The same path, where the disk does not tell capitals apart (Windows): the tags of the Finder
/// keep their paths in lower case
fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().replace('/', "\\").to_lowercase() == b.to_string_lossy().replace('/', "\\").to_lowercase()
    } else {
        a == b
    }
}

// what the three views share
fn core_of(watcher: Option<PyRef<'_, FileWatcher>>) -> Arc<Mutex<Core>> {
    match watcher {
        Some(w) => w.core.clone(),
        None => default_core(),
    }
}

fn to_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn human_size(size: u64) -> String {
    let mut size = size as f64;
    for unit in ["bytes", "KB", "MB", "GB", "TB"] {
        if size < 1000.0 || unit == "TB" {
            return if unit == "bytes" {
                format!("{size:.0} {unit}")
            } else {
                format!("{size:.1} {unit}")
            };
        }
        size /= 1000.0;
    }
    unreachable!()
}

fn kind(e: &Entry) -> String {
    if e.is_dir {
        return "Folder".into();
    }
    let ext = e
        .path
        .extension()
        .map(|x| x.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let known = match ext.as_str() {
        "" => "Document",
        "txt" => "Plain text",
        "md" | "markdown" => "Markdown",
        "py" => "Python script",
        "rs" => "Rust source",
        "js" | "ts" => "Script",
        "json" => "JSON",
        "toml" | "yaml" | "yml" | "ini" | "cfg" => "Configuration",
        "html" | "htm" => "HTML page",
        "css" => "Style sheet",
        "pdf" => "PDF document",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" => "Image",
        "svg" => "SVG image",
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" => "Audio",
        "mp4" | "mov" | "mkv" | "avi" | "webm" => "Video",
        "zip" | "gz" | "tgz" | "tar" | "7z" | "rar" | "xz" | "bz2" => "Archive",
        "doc" | "docx" | "odt" | "rtf" | "pages" => "Text document",
        "xls" | "xlsx" | "ods" | "csv" | "numbers" => "Spreadsheet",
        "ppt" | "pptx" | "odp" | "key" => "Presentation",
        "app" => "Application",
        "dmg" | "iso" => "Disk image",
        "exe" | "msi" => "Program",
        _ => return format!("{} file", ext.to_uppercase()),
    };
    known.into()
}

fn date(time: Option<SystemTime>, format: &str) -> String {
    match time {
        Some(time) => chrono::DateTime::<chrono::Local>::from(time).format(format).to_string(),
        None => "--".into(),
    }
}

/// How much of its color a hidden item keeps in the views
const HIDDEN_OPACITY: f32 = 0.5;

fn emoji(e: &Entry) -> &'static str {
    if e.is_dir {
        "📁"
    } else {
        "📄"
    }
}

/// Paints the native icon of the entry, or an emoji where the platform gives none
fn paint_icon(ui: &egui::Ui, e: &Entry, rect: Rect) {
    match icon_uri(ui.ctx(), &e.path.to_string_lossy(), rect.width()) {
        Some(uri) => egui::Image::new(uri).paint_at(ui, rect),
        None => {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                emoji(e),
                FontId::proportional(rect.height() * 0.8),
                ui.visuals().text_color(),
            );
        }
    }
}

/// The colors of a selected item: full when the view has the keyboard, faint otherwise
fn selection_colors(ui: &egui::Ui, focused: bool) -> (Color32, Color32) {
    let v = ui.visuals();
    if focused {
        (v.selection.bg_fill, v.selection.stroke.color)
    } else {
        (v.text_color().gamma_multiply(0.15), v.strong_text_color())
    }
}

fn truncated(ui: &egui::Ui, text: &str, width: f32, color: Option<Color32>) -> Arc<egui::Galley> {
    let mut text = egui::RichText::new(text);
    if let Some(color) = color {
        text = text.color(color);
    }
    egui::WidgetText::from(text).into_galley(ui, Some(TextWrapMode::Truncate), width.max(1.0), TextStyle::Body)
}

/// The keys of a view, read once per frame when it has the focus
#[derive(Default)]
struct Keys {
    down: bool,
    up: bool,
    left: bool,
    right: bool,
    home: bool,
    end: bool,
    page_up: bool,
    page_down: bool,
    enter: bool,
    select_all: bool,
    parent: bool,
    shift: bool,
    text: String,
}

fn read_keys(ui: &egui::Ui) -> Keys {
    ui.input(|i| {
        let m = i.modifiers;
        let mut keys = Keys {
            down: i.key_pressed(Key::ArrowDown) && !m.command,
            up: i.key_pressed(Key::ArrowUp) && !m.command,
            left: i.key_pressed(Key::ArrowLeft),
            right: i.key_pressed(Key::ArrowRight),
            home: i.key_pressed(Key::Home),
            end: i.key_pressed(Key::End),
            page_up: i.key_pressed(Key::PageUp),
            page_down: i.key_pressed(Key::PageDown),
            // Cmd+Down opens like Enter, as in the Finder
            enter: i.key_pressed(Key::Enter) || (m.command && i.key_pressed(Key::ArrowDown)),
            select_all: m.command && i.key_pressed(Key::A),
            parent: m.command && i.key_pressed(Key::ArrowUp),
            shift: m.shift,
            text: String::new(),
        };
        if !m.command && !m.ctrl {
            for event in &i.events {
                if let egui::Event::Text(t) = event {
                    keys.text.push_str(t);
                }
            }
        }
        keys
    })
}

/// The view takes the arrows while it has the focus, Tab still moves to the next widget
fn lock_arrows(ui: &egui::Ui, id: Id) {
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            id,
            EventFilter {
                tab: false,
                escape: false,
                horizontal_arrows: true,
                vertical_arrows: true,
            },
        )
    });
}

/// The context menus to open after the view is drawn, once the Python object is free again: the
/// function may read or change the view
type Menus = Vec<(Response, Vec<String>)>;

unsafe fn run_menus(menus: Menus, menu: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
    let Some(menu) = menu else { return Ok(()) };
    let mut result = Ok(());
    for (response, paths) in menus {
        response.context_menu(|ui| {
            if let Err(err) = run_nested_call(ui, menu, (paths.clone(),)) {
                result = Err(err);
            }
        });
    }
    result
}

/// What the Python code adds to the items of a view: colored dots (badges) and, for the list,
/// columns it draws itself
struct Hooks<'py> {
    badges: Option<Bound<'py, PyAny>>,
    extras: Vec<Extra<'py>>,
}

/// A column of a list drawn by a Python function
struct Extra<'py> {
    title: String,
    width: f32,
    cell: Bound<'py, PyAny>,
    /// what the column holds, for its sort and its filter
    ty: DataType,
    /// called with the path of a row, returns its value: without it the column does not sort or filter
    value: Option<Bound<'py, PyAny>>,
}

impl<'py> Hooks<'py> {
    fn read(kwargs: Option<&Bound<'py, PyDict>>) -> PyResult<Self> {
        let mut extras = vec![];
        if let Some(columns) = kwarg::<Vec<Bound<'py, PyTuple>>>(kwargs, "extra_columns")? {
            for column in columns {
                if column.len() != 3 && column.len() != 4 {
                    return Err(PyValueError::new_err(
                        "an extra column is (title, width, cell) or (title, width, cell, options)",
                    ));
                }
                let (mut ty, mut value) = (DataType::Text, None);
                if column.len() == 4 {
                    let options = column.get_item(3)?;
                    let options = options.cast::<PyDict>()?;
                    if let Some(t) = kwarg::<String>(Some(options), "type")? {
                        ty = DataType::parse(&t)?;
                    }
                    value = kwarg::<Bound<'py, PyAny>>(Some(options), "value")?;
                }
                extras.push(Extra {
                    title: column.get_item(0)?.extract()?,
                    width: column.get_item(1)?.extract()?,
                    cell: column.get_item(2)?,
                    ty,
                    value,
                });
            }
        }
        Ok(Hooks {
            badges: kwarg(kwargs, "badges")?,
            extras,
        })
    }

    /// The colors of the dots of an item, given by the badges function. An error is printed and
    /// gives no dot, so the view is still drawn
    fn badges(&self, path: &Path) -> Vec<Color32> {
        let Some(function) = &self.badges else { return vec![] };
        let result = function
            .call1((to_str(path),))
            .and_then(|colors| colors.extract::<Vec<Bound<'py, PyAny>>>().map_err(Into::into))
            .and_then(|colors| colors.iter().map(extract_color).collect::<PyResult<Vec<_>>>());
        result.unwrap_or_else(|err| {
            err.display(function.py());
            vec![]
        })
    }
}

/// The dots of the badges, from `left` to the right, centered on `y`
fn paint_dots(ui: &egui::Ui, colors: &[Color32], left: f32, y: f32, size: f32) {
    for (i, color) in colors.iter().enumerate() {
        let center = pos2(left + size / 2.0 + i as f32 * (size + 3.0), y);
        ui.painter().circle_filled(center, size / 2.0, *color);
    }
}

fn dots_width(colors: &[Color32], size: f32) -> f32 {
    if colors.is_empty() {
        0.0
    } else {
        colors.len() as f32 * (size + 3.0) - 3.0
    }
}

// list and icon views: one folder

#[derive(Clone, Copy, PartialEq)]
enum SortBy {
    Name,
    Modified,
    Size,
    Kind,
}

impl SortBy {
    fn parse(name: &str) -> PyResult<Self> {
        Ok(match name {
            "name" => SortBy::Name,
            "modified" => SortBy::Modified,
            "size" => SortBy::Size,
            "kind" => SortBy::Kind,
            _ => {
                return Err(PyValueError::new_err(
                    "sort_by must be \"name\", \"modified\", \"size\" or \"kind\"",
                ))
            }
        })
    }

    fn name(self) -> &'static str {
        match self {
            SortBy::Name => "name",
            SortBy::Modified => "modified",
            SortBy::Size => "size",
            SortBy::Kind => "kind",
        }
    }

    fn data_type(self) -> DataType {
        match self {
            SortBy::Name => DataType::Text,
            SortBy::Modified => DataType::Date,
            SortBy::Size => DataType::Number,
            SortBy::Kind => DataType::Choice,
        }
    }
}

/// A column of the list or of the tree: one of the columns of the view, or one drawn by Python
/// (its index in the extra columns)
#[derive(Clone, Copy, PartialEq)]
enum Column {
    Sort(SortBy),
    Extra(usize),
}

impl Column {
    /// The key of the column in the state of the view: its name, or the title of an extra column
    fn key(self, hooks: &Hooks<'_>) -> String {
        match self {
            Column::Sort(sort) => sort.name().into(),
            Column::Extra(i) => hooks.extras[i].title.clone(),
        }
    }

    fn data_type(self, hooks: &Hooks<'_>) -> DataType {
        match self {
            Column::Sort(sort) => sort.data_type(),
            Column::Extra(i) => hooks.extras[i].ty,
        }
    }

    /// Sorts and filters: a column of Python needs a value function
    fn has_value(self, hooks: &Hooks<'_>) -> bool {
        match self {
            Column::Sort(_) => true,
            Column::Extra(i) => hooks.extras[i].value.is_some(),
        }
    }

    fn of_key(key: &str, hooks: &Hooks<'_>) -> Option<Column> {
        if let Ok(sort) = SortBy::parse(key) {
            return Some(Column::Sort(sort));
        }
        hooks.extras.iter().position(|e| e.title == key).map(Column::Extra)
    }
}

/// The values of the columns of Python, kept for a second: the functions are not called at every
/// frame for every item
#[derive(Default)]
struct Values {
    map: HashMap<(usize, PathBuf), Value>,
    since: Option<Instant>,
}

impl Values {
    fn refresh(&mut self) {
        if self.since.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            self.map.clear();
            self.since = Some(Instant::now());
        }
    }
}

/// The value of an item in a column. An error of a Python function is printed and gives no value
fn value_of(e: &Entry, column: Column, hooks: &Hooks<'_>, values: &mut Values) -> Value {
    match column {
        Column::Sort(SortBy::Name) => Value::Text(e.name.clone()),
        Column::Sort(SortBy::Modified) => match e.modified.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()) {
            Some(d) => Value::Num(d.as_secs_f64()),
            None => Value::None,
        },
        Column::Sort(SortBy::Size) if e.is_dir => Value::None,
        Column::Sort(SortBy::Size) => Value::Num(e.size as f64),
        Column::Sort(SortBy::Kind) => Value::Text(kind(e)),
        Column::Extra(i) => {
            let Some(function) = &hooks.extras[i].value else { return Value::None };
            let key = (i, e.path.clone());
            if let Some(v) = values.map.get(&key) {
                return v.clone();
            }
            let result = function.call1((to_str(&e.path),)).map(|r| Value::from_py(&r, hooks.extras[i].ty));
            let v = result.unwrap_or_else(|err| {
                err.display(function.py());
                Value::None
            });
            values.map.insert(key, v.clone());
            v
        }
    }
}

/// What the user chose: which items stay, and in which order
struct Rules {
    filters: Vec<(Column, DataType, Filter)>,
    sort: Column,
    ascending: bool,
    dirs_first: bool,
}

impl Rules {
    fn new(state: &ColumnsState, hooks: &Hooks<'_>, dirs_first: bool) -> Self {
        let filters = state
            .filters
            .iter()
            .filter(|(_, f)| !f.is_empty())
            .filter_map(|(key, f)| {
                let column = Column::of_key(key, hooks).filter(|c| c.has_value(hooks))?;
                Some((column, column.data_type(hooks), f.clone()))
            })
            .collect();
        let sort = Column::of_key(&state.sort_key, hooks)
            .filter(|c| c.has_value(hooks))
            .unwrap_or(Column::Sort(SortBy::Name));
        Rules { filters, sort, ascending: state.ascending, dirs_first }
    }
}

/// The items kept by the filters and sorted. `filter_dirs` false keeps the folders whatever the
/// filters say (the tree: a folder holds items that may match)
fn arrange<T>(
    items: Vec<T>,
    entry: impl Fn(&T) -> &Entry,
    rules: &Rules,
    hooks: &Hooks<'_>,
    values: &mut Values,
    filter_dirs: bool,
    sorted: bool,
) -> Vec<T> {
    let items: Vec<T> = items
        .into_iter()
        .filter(|item| {
            let e = entry(item);
            (e.is_dir && !filter_dirs)
                || rules.filters.iter().all(|(column, ty, filter)| filter.matches(*ty, &value_of(e, *column, hooks, values)))
        })
        .collect();
    if !sorted {
        return items;
    }
    let keys: Vec<Value> = items.iter().map(|item| value_of(entry(item), rules.sort, hooks, values)).collect();
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        let by = keys[a].compare(&keys[b]).then_with(|| natural_cmp(&entry(&items[a]).name, &entry(&items[b]).name));
        if rules.ascending {
            by
        } else {
            by.reverse()
        }
    });
    if rules.dirs_first {
        order.sort_by_key(|&i| !entry(&items[i]).is_dir); // stable: the order above is kept inside each group
    }
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| slots[i].take()).collect()
}

/// The distinct values of a column of choices, for its filter
fn distinct_values<'a>(
    entries: impl Iterator<Item = &'a Entry>,
    key: &str,
    hooks: &Hooks<'_>,
    values: &mut Values,
) -> Vec<String> {
    let Some(column) = Column::of_key(key, hooks) else { return vec![] };
    let mut found: Vec<String> = entries
        .filter_map(|e| match value_of(e, column, hooks, values) {
            Value::Text(t) => Some(t),
            _ => None,
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    found.sort_by(|a, b| natural_cmp(a, b));
    found
}

/// The state of a list or icon view
pub(crate) struct FolderView {
    id: String,
    core: Arc<Mutex<Core>>,
    history: Vec<PathBuf>,
    position: usize,
    show_hidden: bool,
    multi_select: bool,
    navigate: bool,
    dirs_first: bool,
    /// width, order, sort and filters of the columns
    columns: ColumnsState,
    /// values of the columns of Python
    values: Values,
    /// the items of the folder at the last frame, before the filters (for the choices of a filter)
    listing: Arc<Vec<Entry>>,
    selected: Vec<PathBuf>,
    anchor: Option<PathBuf>,
    cursor: Option<PathBuf>,
    activated: Vec<PathBuf>,
    /// the items shown at the last frame, in their order
    shown: Vec<PathBuf>,
    scroll_to: Option<PathBuf>,
    focus: Option<bool>,
    typed: String,
    typed_at: Option<Instant>,
    /// where the rubber band started, from the top left of the content, and the selection kept
    band: Option<(Pos2, Vec<PathBuf>)>,
    folder_changed: bool,
    /// the size of the icons of the icon view
    icon_size: f32,
    /// the items shown instead of the folder, e.g. the result of a search
    paths: Option<Vec<PathBuf>>,
    /// takes the keyboard focus back at every frame, unless a text field is being edited
    keep_focus: bool,
}

impl FolderView {
    #[allow(clippy::too_many_arguments)]
    fn new(
        id: String,
        folder: PathBuf,
        watcher: Option<PyRef<'_, FileWatcher>>,
        show_hidden: bool,
        multi_select: bool,
        navigate: bool,
        dirs_first: bool,
        sort_by: &str,
    ) -> PyResult<Self> {
        Ok(FolderView {
            id,
            core: core_of(watcher),
            history: vec![expand(&folder)],
            position: 0,
            show_hidden,
            multi_select,
            navigate,
            dirs_first,
            columns: ColumnsState::new(sort_by),
            values: Values::default(),
            listing: Arc::new(vec![]),
            selected: vec![],
            anchor: None,
            cursor: None,
            activated: vec![],
            shown: vec![],
            scroll_to: None,
            focus: None,
            typed: String::new(),
            typed_at: None,
            band: None,
            folder_changed: false,
            icon_size: 48.0,
            paths: None,
            keep_focus: false,
        })
    }

    fn folder(&self) -> &Path {
        &self.history[self.position]
    }

    fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
        self.cursor = None;
    }

    fn go(&mut self, folder: &Path) {
        let folder = expand(folder);
        if folder == self.folder() {
            return;
        }
        self.history.truncate(self.position + 1);
        self.history.push(folder);
        self.position = self.history.len() - 1;
        self.entered();
    }

    fn entered(&mut self) {
        self.paths = None;
        self.clear_selection();
        self.folder_changed = true;
    }

    fn up(&mut self) {
        let child = self.folder().to_path_buf();
        if let Some(parent) = child.parent() {
            self.go(parent);
            self.select_only(child.clone());
            self.scroll_to = Some(child);
        }
    }

    fn back(&mut self) {
        if self.position > 0 {
            self.position -= 1;
            self.entered();
        }
    }

    fn forward(&mut self) {
        if self.position + 1 < self.history.len() {
            self.position += 1;
            self.entered();
        }
    }

    fn select_only(&mut self, path: PathBuf) {
        self.selected = vec![path.clone()];
        self.anchor = Some(path.clone());
        self.cursor = Some(path);
    }

    fn select_range(&mut self, to: usize) {
        let from = self
            .anchor
            .as_ref()
            .and_then(|a| self.shown.iter().position(|p| p == a))
            .unwrap_or(to);
        let (a, b) = (from.min(to), from.max(to));
        self.selected = self.shown[a..=b].to_vec();
        self.cursor = Some(self.shown[to].clone());
        if self.anchor.is_none() {
            self.anchor = self.cursor.clone();
        }
    }

    /// A click on an item: alone, Cmd (Ctrl) toggles it, Shift selects the range from the last click
    fn click(&mut self, path: &Path, modifiers: egui::Modifiers) {
        if self.multi_select && modifiers.command {
            if let Some(i) = self.selected.iter().position(|p| p == path) {
                self.selected.remove(i);
            } else {
                self.selected.push(path.to_path_buf());
            }
            self.anchor = Some(path.to_path_buf());
            self.cursor = Some(path.to_path_buf());
        } else if self.multi_select && modifiers.shift {
            if let Some(i) = self.shown.iter().position(|p| p == path) {
                self.select_range(i);
            }
        } else {
            self.select_only(path.to_path_buf());
        }
    }

    /// Enter or a double click: a folder is entered when the view navigates, the rest is activated
    fn activate(&mut self, paths: Vec<PathBuf>) {
        if self.navigate && paths.len() == 1 && paths[0].is_dir() {
            self.go(&paths[0].clone());
        } else {
            self.activated = paths;
        }
    }

    /// The items of the folder as the view shows them: filtered and sorted
    fn entries(&mut self, ctx: &egui::Context, hooks: &Hooks<'_>) -> Vec<Entry> {
        let all = match &self.paths {
            None => {
                let folder = self.folder().to_path_buf();
                lock(&self.core).list(Some(ctx), &folder)
            }
            // given paths: each one from the listing of its folder, so it stays up to date too
            Some(paths) => {
                let mut core = lock(&self.core);
                let mut found = vec![];
                for path in paths {
                    let Some(parent) = path.parent() else { continue };
                    if let Some(e) = core.list(Some(ctx), parent).iter().find(|e| same_path(&e.path, path)) {
                        found.push(e.clone());
                    }
                }
                Arc::new(found)
            }
        };
        // what was removed from the disk is no longer selected
        let present: HashSet<&Path> = all.iter().map(|e| e.path.as_path()).collect();
        self.selected.retain(|p| present.contains(p.as_path()));
        if self.cursor.as_ref().is_some_and(|c| !present.contains(c.as_path())) {
            self.cursor = self.selected.last().cloned();
        }
        self.listing = all.clone();
        let rules = Rules::new(&self.columns, hooks, self.dirs_first);
        self.values.refresh();
        let visible: Vec<Entry> = all.iter().filter(|e| self.show_hidden || !e.hidden).cloned().collect();
        let entries = arrange(visible, |e| e, &rules, hooks, &mut self.values, true, true);
        self.shown = entries.iter().map(|e| e.path.clone()).collect();
        entries
    }

    /// The keyboard, when the view has the focus. step_x and step_y move the cursor by an item and
    /// by a row, page by a screen
    fn keyboard(&mut self, keys: &Keys, step_x: usize, step_y: usize, page: usize) {
        let n = self.shown.len();
        if keys.parent && self.navigate {
            self.up();
            return;
        }
        if keys.select_all && self.multi_select {
            self.selected = self.shown.clone();
            return;
        }
        if keys.enter && !self.selected.is_empty() {
            let paths = self.selected.clone();
            self.activate(paths);
            return;
        }
        if n == 0 {
            return;
        }
        let current = self
            .cursor
            .as_ref()
            .and_then(|c| self.shown.iter().position(|p| p == c));
        let back = |by: usize| current.map(|c| c.saturating_sub(by)).unwrap_or(n - 1);
        let ahead = |by: usize| current.map(|c| (c + by).min(n - 1)).unwrap_or(0);
        let target = if keys.down && step_y > 0 {
            Some(ahead(step_y))
        } else if keys.up && step_y > 0 {
            Some(back(step_y))
        } else if keys.right && step_x > 0 {
            Some(ahead(step_x))
        } else if keys.left && step_x > 0 {
            Some(back(step_x))
        } else if keys.home {
            Some(0)
        } else if keys.end {
            Some(n - 1)
        } else if keys.page_down {
            Some(ahead(page))
        } else if keys.page_up {
            Some(back(page))
        } else {
            None
        };
        if let Some(i) = target {
            if keys.shift && self.multi_select {
                self.select_range(i);
            } else {
                self.select_only(self.shown[i].clone());
            }
            self.scroll_to = Some(self.shown[i].clone());
            return;
        }
        // typing the start of a name selects it
        if !keys.text.is_empty() {
            if self.typed_at.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
                self.typed.clear();
            }
            self.typed.push_str(&keys.text.to_lowercase());
            self.typed_at = Some(Instant::now());
            let found = self.shown.iter().find(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().to_lowercase().starts_with(&self.typed))
            });
            if let Some(path) = found.cloned() {
                self.select_only(path.clone());
                self.scroll_to = Some(path);
            }
        }
    }
}

/// How the items are placed: rows of a list or tiles of a grid
enum Shape {
    List {
        columns: Vec<(Column, f32)>,
        headers: [String; 4],
        date_format: String,
        row_height: f32,
    },
    Icons {
        icon: f32,
        tile: Vec2Size,
        per_row: usize,
    },
}

#[derive(Clone, Copy)]
struct Vec2Size {
    w: f32,
    h: f32,
}

impl Shape {
    /// The rectangle of the item i, from the top left of the content
    fn item_rect(&self, i: usize, width: f32) -> Rect {
        match self {
            Shape::List { row_height, .. } => {
                Rect::from_min_size(pos2(0.0, i as f32 * row_height), vec2(width, *row_height))
            }
            Shape::Icons { tile, per_row, .. } => {
                let (col, row) = (i % per_row, i / per_row);
                Rect::from_min_size(pos2(col as f32 * tile.w, row as f32 * tile.h), vec2(tile.w, tile.h))
            }
        }
    }

    fn content_height(&self, n: usize) -> f32 {
        match self {
            Shape::List { row_height, .. } => n as f32 * row_height,
            Shape::Icons { tile, per_row, .. } => n.div_ceil(*per_row) as f32 * tile.h,
        }
    }

    /// The items that can be seen in this part of the content
    fn visible(&self, viewport: Rect, n: usize) -> std::ops::Range<usize> {
        let (row_h, per_row) = match self {
            Shape::List { row_height, .. } => (*row_height, 1),
            Shape::Icons { tile, per_row, .. } => (tile.h, *per_row),
        };
        let first = (viewport.top() / row_h).floor().max(0.0) as usize * per_row;
        let last = ((viewport.bottom() / row_h).ceil().max(0.0) as usize + 1) * per_row;
        first.min(n)..last.min(n)
    }
}

/// The width of a column that the user did not change
fn default_width(column: Column, hooks: &Hooks<'_>) -> f32 {
    match column {
        Column::Sort(SortBy::Name) => 0.0,
        Column::Sort(SortBy::Modified) => 150.0,
        Column::Sort(SortBy::Size) => 80.0,
        Column::Sort(SortBy::Kind) => 130.0,
        Column::Extra(i) => hooks.extras[i].width,
    }
}

/// The columns of a list, in the order the user chose. The name takes what the others leave (at
/// least 160), unless the user gave it a width
fn resolve_columns(state: &ColumnsState, wanted: &[Column], hooks: &Hooks<'_>, width: f32) -> Vec<(Column, f32)> {
    let default: Vec<String> = wanted.iter().map(|c| c.key(hooks)).collect();
    let columns: Vec<Column> = state
        .ordered(&default)
        .iter()
        .filter_map(|key| wanted.iter().copied().find(|c| c.key(hooks) == *key))
        .collect();
    let name = Column::Sort(SortBy::Name);
    let width_of = |c: Column| state.widths.get(&c.key(hooks)).copied().unwrap_or_else(|| default_width(c, hooks));
    let name_fixed = state.widths.get("name").copied();
    let others = |columns: &[Column]| columns.iter().filter(|c| **c != name).map(|c| width_of(*c)).sum::<f32>();
    // a view too narrow for its columns scrolls horizontally: the name keeps a minimum width
    let rest = (width - others(&columns)).max(160.0);
    columns
        .into_iter()
        .map(|c| (c, if c == name { name_fixed.unwrap_or(rest) } else { width_of(c) }))
        .collect()
}

/// The header of a list: a click on a column sorts by it (a second click reverses the order), a
/// drag on a title moves the column, a drag on its border resizes it, the funnel filters it
fn list_header(ui: &mut egui::Ui, view: &mut FolderView, shape: &Shape, hooks: &Hooks<'_>, id: Id, opts: HeaderOptions) {
    let Shape::List { columns, headers, row_height, .. } = shape else { return };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), *row_height), Sense::hover());
    let zones = list_zones(&view.columns, columns, hooks, rect.width());
    hscroll_wheel(ui, &mut view.columns, &zones, Rect::from_min_max(rect.min, ui.max_rect().max));
    let cells: Vec<Cell> = columns
        .iter()
        .map(|(column, _)| {
            let title = match column {
                Column::Sort(sort) => headers[*sort as usize].clone(),
                Column::Extra(k) => hooks.extras[*k].title.clone(),
            };
            Cell {
                key: column.key(hooks),
                title,
                rect,
                ty: column.data_type(hooks),
                sortable: column.has_value(hooks),
                filterable: column.has_value(hooks),
                movable: true,
                resizable: true,
            }
        })
        .collect();
    let (listing, show_hidden) = (view.listing.clone(), view.show_hidden);
    let mut values = std::mem::take(&mut view.values);
    columns::zoned_header(ui, id, rect, &zones, cells, &mut view.columns, opts, &mut |key| {
        distinct_values(listing.iter().filter(|e| show_hidden || !e.hidden), key, hooks, &mut values)
    });
    view.values = values;
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
}

/// The zones (pinned at the left, scrolling, pinned at the right) of the columns of a list
fn list_zones(state: &ColumnsState, columns: &[(Column, f32)], hooks: &Hooks<'_>, width: f32) -> Zones {
    let keys: Vec<String> = columns.iter().map(|(c, _)| c.key(hooks)).collect();
    let widths: Vec<f32> = columns.iter().map(|(_, w)| *w).collect();
    Zones::new(&keys, &widths, state, width, None)
}

#[allow(clippy::too_many_arguments)]
fn paint_list_row(
    row_ui: &mut egui::Ui,
    e: &Entry,
    rect: Rect,
    shape: &Shape,
    hooks: &Hooks<'_>,
    badges: &[Color32],
    text_color: Option<Color32>,
    zones: &Zones,
    scroll: f32,
) {
    let Shape::List { columns, date_format, .. } = shape else { return };
    for placed in &zones.placed {
        let (column, width) = &columns[placed.index];
        let cell = zones.cell_rect(rect, placed, scroll);
        let visible = zones.visible(rect, placed, scroll);
        if visible.width() <= 0.0 {
            continue;
        }
        // a column pinned or scrolling is cut by its zone
        let mut zone_ui = row_ui.new_child(egui::UiBuilder::new().max_rect(rect));
        zone_ui.shrink_clip_rect(visible);
        let ui = &mut zone_ui;
        let column = match column {
            Column::Sort(sort) => *sort,
            Column::Extra(k) => {
                // drawn by Python in a ui of the size of the cell
                let mut cell_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(cell.shrink2(vec2(8.0, 0.0)))
                        .layout(egui::Layout::left_to_right(Align::Center)),
                );
                cell_ui.shrink_clip_rect(cell);
                let _ = unsafe { run_nested_call(&mut cell_ui, &hooks.extras[*k].cell, (to_str(&e.path),)) };
                continue;
            }
        };
        match column {
            SortBy::Name => {
                let side = 16.0;
                let icon = Rect::from_center_size(pos2(cell.left() + 8.0 + side / 2.0, cell.center().y), vec2(side, side));
                paint_icon(ui, e, icon);
                let left = icon.right() + 6.0;
                // the badges at the right end of the name
                let dots = dots_width(badges, 9.0);
                if dots > 0.0 {
                    paint_dots(ui, badges, cell.right() - 8.0 - dots, cell.center().y, 9.0);
                }
                let galley = truncated(ui, &e.name, cell.right() - left - 6.0 - dots - if dots > 0.0 { 8.0 } else { 0.0 }, text_color);
                let color = text_color.unwrap_or(ui.visuals().text_color());
                ui.painter().galley(pos2(left, cell.center().y - galley.size().y / 2.0), galley, color);
            }
            other => {
                let text = match other {
                    SortBy::Modified => date(e.modified, date_format),
                    SortBy::Size if e.is_dir => "--".into(),
                    SortBy::Size => human_size(e.size),
                    _ => kind(e),
                };
                let color = text_color.unwrap_or(ui.visuals().weak_text_color());
                let galley = truncated(ui, &text, width - 16.0, Some(color));
                let left = if other == SortBy::Size {
                    cell.right() - 8.0 - galley.size().x // numbers are right aligned
                } else {
                    cell.left() + 8.0
                };
                ui.painter().galley(pos2(left, cell.center().y - galley.size().y / 2.0), galley, color);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_tile(ui: &egui::Ui, e: &Entry, rect: Rect, icon: f32, badges: &[Color32], selected: bool, hovered: bool, focused: bool) {
    let icon_rect = Rect::from_center_size(pos2(rect.center().x, rect.top() + 6.0 + icon / 2.0), vec2(icon, icon));
    if selected {
        ui.painter().rect_filled(icon_rect.expand(4.0), 6.0, ui.visuals().text_color().gamma_multiply(0.12));
    } else if hovered {
        ui.painter().rect_filled(rect.shrink(2.0), 6.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
    }
    paint_icon(ui, e, icon_rect);
    // the badges on the top left corner of the icon
    paint_dots(ui, badges, icon_rect.left() + 2.0, icon_rect.top() + 2.0 + 4.5, 9.0);

    let (fill, selected_text) = selection_colors(ui, focused);
    let color = if selected { selected_text } else { ui.visuals().text_color() };
    let font = TextStyle::Body.resolve(ui.style());
    let mut job = egui::text::LayoutJob::simple(e.name.clone(), font, color, rect.width() - 10.0);
    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = true;
    job.halign = Align::Center;
    let galley = ui.painter().layout_job(job);
    let pos = pos2(rect.center().x, icon_rect.bottom() + 6.0);
    if selected {
        let text_rect = galley.rect.translate(pos.to_vec2()).expand2(vec2(4.0, 1.0));
        ui.painter().rect_filled(text_rect, 4.0, fill);
    }
    ui.painter().galley(pos, galley, color);
}

/// Draws a list or icon view. Returns the context menus to open once the view is released
fn draw_folder(ui: &mut egui::Ui, view: &mut FolderView, shape: Shape, hooks: &Hooks<'_>, has_menu: bool, opts: HeaderOptions) -> Menus {
    let viewport = ui.available_width();
    draw_folder_in(ui, view, shape, hooks, has_menu, opts, viewport)
}

fn draw_folder_in(
    ui: &mut egui::Ui,
    view: &mut FolderView,
    mut shape: Shape,
    hooks: &Hooks<'_>,
    has_menu: bool,
    opts: HeaderOptions,
    viewport: f32,
) -> Menus {
    let id = ui.make_persistent_id(&view.id);
    let ctx = ui.ctx().clone();
    view.activated.clear();
    view.folder_changed = false;
    let mut menus = Menus::new();

    let width = viewport;
    if let Shape::Icons { icon, tile, per_row } = &mut shape {
        let line = ui.text_style_height(&TextStyle::Body);
        let min_w = (*icon + 44.0).max(84.0);
        *per_row = ((width / min_w).floor() as usize).max(1);
        *tile = Vec2Size {
            w: width / *per_row as f32,
            h: 6.0 + *icon + 6.0 + 2.0 * line + 10.0,
        };
    }
    let wanted: Vec<Column> = match &shape {
        Shape::List { columns, .. } => columns.iter().map(|(c, _)| *c).collect(),
        Shape::Icons { .. } => vec![],
    };
    if let Shape::List { columns, .. } = &mut shape {
        *columns = resolve_columns(&view.columns, &wanted, hooks, width);
    }

    match view.focus.take() {
        Some(true) => ui.memory_mut(|m| m.request_focus(id)),
        Some(false) => ui.memory_mut(|m| m.surrender_focus(id)),
        None if view.keep_focus && !ui.ctx().text_edit_focused() => ui.memory_mut(|m| m.request_focus(id)),
        None => {}
    }
    lock_arrows(ui, id);
    let focused = ui.memory(|m| m.has_focus(id));

    if matches!(shape, Shape::List { .. }) {
        list_header(ui, view, &shape, hooks, id, opts);
        // the header may have changed the widths and the order of the columns
        if let Shape::List { columns, .. } = &mut shape {
            *columns = resolve_columns(&view.columns, &wanted, hooks, width);
        }
    }
    // the header may have changed the sort or the filters
    let entries = view.entries(&ctx, hooks);
    let zones = match &shape {
        Shape::List { columns, .. } => list_zones(&view.columns, columns, hooks, width),
        Shape::Icons { .. } => Zones::new(&[], &[], &view.columns, width, None),
    };
    view.columns.scroll = view.columns.scroll.clamp(0.0, zones.max_scroll());
    let scroll = view.columns.scroll;

    if focused {
        let keys = read_keys(ui);
        let (step_x, step_y, page) = match &shape {
            Shape::List { row_height, .. } => (0, 1, (ui.available_height() / row_height) as usize),
            Shape::Icons { per_row, tile, .. } => (1, *per_row, (ui.available_height() / tile.h) as usize * per_row),
        };
        let folder = view.folder().to_path_buf();
        view.keyboard(&keys, step_x, step_y, page.max(1));
        if view.folder() != folder {
            return menus; // entered another folder: drawn at the next frame
        }
    }

    let n = entries.len();
    let scroll_to = view.scroll_to.take();
    let output = egui::ScrollArea::vertical()
        .id_salt(id.with(view.folder()))
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let origin = ui.max_rect().min;
            let width = ui.max_rect().width();
            let height = shape.content_height(n).max(viewport.height());
            ui.allocate_rect(Rect::from_min_size(origin, vec2(width, height)), Sense::hover());

            // the background: a click clears the selection, a drag draws a rubber band
            let seen = viewport.translate(origin.to_vec2());
            let background = ui.interact(seen, id, Sense::click_and_drag());
            if background.clicked() || background.drag_started() || background.secondary_clicked() {
                ui.memory_mut(|m| m.request_focus(id));
            }
            let mods = ui.input(|i| i.modifiers);
            if (background.clicked() && !mods.command && !mods.shift) || background.secondary_clicked() {
                view.clear_selection();
            }
            if has_menu {
                menus.push((background.clone(), vec![]));
            }

            let (fill, selected_text) = selection_colors(ui, focused);
            for i in shape.visible(viewport, n) {
                let e = &entries[i];
                let rect = shape.item_rect(i, width).translate(origin.to_vec2());
                let response = ui.interact(rect.shrink(1.0), id.with(("item", &e.path)), Sense::click());
                if response.clicked() || response.secondary_clicked() {
                    ui.memory_mut(|m| m.request_focus(id));
                }
                if response.clicked() {
                    view.click(&e.path, mods);
                }
                if response.secondary_clicked() && !view.selected.contains(&e.path) {
                    view.select_only(e.path.clone());
                }
                if response.double_clicked() {
                    view.select_only(e.path.clone());
                    view.activate(vec![e.path.clone()]);
                }
                let selected = view.selected.contains(&e.path);
                let badges = hooks.badges(&e.path);
                if has_menu {
                    let paths = if selected { view.selected.iter().map(|p| to_str(p)).collect() } else { vec![to_str(&e.path)] };
                    menus.push((response.clone(), paths));
                }
                // a hidden item (a name with a dot, a hidden flag) is drawn faded: its icon, its name
                // and the columns of Python, not the background of its row
                let opacity = ui.opacity();
                match &shape {
                    Shape::List { .. } => {
                        if selected {
                            ui.painter().rect_filled(rect, 4.0, fill);
                        } else if response.hovered() {
                            ui.painter().rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
                        } else if i % 2 == 1 {
                            ui.painter().rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
                        }
                        if e.hidden {
                            ui.multiply_opacity(HIDDEN_OPACITY);
                        }
                        paint_list_row(ui, e, rect, &shape, hooks, &badges, selected.then_some(selected_text), &zones, scroll);
                    }
                    Shape::Icons { icon, .. } => {
                        if e.hidden {
                            ui.multiply_opacity(HIDDEN_OPACITY);
                        }
                        paint_tile(ui, e, rect, *icon, &badges, selected, response.hovered(), focused);
                    }
                }
                ui.set_opacity(opacity);
            }
            if view.folder_changed {
                return; // a double click entered a folder
            }

            if let Some(target) = &scroll_to {
                if let Some(i) = entries.iter().position(|e| &e.path == target) {
                    ui.scroll_to_rect(shape.item_rect(i, width).translate(origin.to_vec2()), None);
                }
            }

            rubber_band(ui, view, &background, &shape, &entries, origin, width, seen, mods);
        });
    hscroll_bar(ui, id, &mut view.columns, &zones, output.inner_rect);
    menus
}

/// Selects the items a rectangle drawn on the background touches. Cmd or Shift keeps the selection
#[allow(clippy::too_many_arguments)]
fn rubber_band(
    ui: &mut egui::Ui,
    view: &mut FolderView,
    background: &Response,
    shape: &Shape,
    entries: &[Entry],
    origin: Pos2,
    width: f32,
    seen: Rect,
    mods: egui::Modifiers,
) {
    if !view.multi_select {
        return;
    }
    let pointer = ui.input(|i| i.pointer.interact_pos());
    if background.drag_started() {
        if let Some(start) = ui.input(|i| i.pointer.press_origin()) {
            let kept = if mods.command || mods.shift { view.selected.clone() } else { vec![] };
            view.band = Some((start - origin.to_vec2(), kept));
        }
    }
    if background.drag_stopped() || !background.dragged() {
        view.band = None;
        return;
    }
    let (Some((start, kept)), Some(here)) = (view.band.clone(), pointer) else { return };
    let band = Rect::from_two_pos(start, here - origin.to_vec2());
    let mut selected = kept;
    for (i, e) in entries.iter().enumerate() {
        if shape.item_rect(i, width).intersects(band) && !selected.contains(&e.path) {
            selected.push(e.path.clone());
        }
    }
    view.selected = selected;
    view.cursor = view.selected.last().cloned();
    let accent = ui.visuals().selection.bg_fill;
    let screen = band.translate(origin.to_vec2()).intersect(seen);
    ui.painter().rect(screen, 2.0, accent.gamma_multiply(0.25), Stroke::new(1.0, accent), egui::StrokeKind::Inside);
    // the view scrolls when the band goes past its top or bottom
    let past = if here.y < seen.top() {
        seen.top() - here.y
    } else if here.y > seen.bottom() {
        seen.bottom() - here.y
    } else {
        0.0
    };
    if past != 0.0 {
        ui.scroll_with_delta(vec2(0.0, past.clamp(-30.0, 30.0)));
        ui.ctx().request_repaint();
    }
}

/// The methods the list and the icon views share
macro_rules! folder_view_methods {
    ($class:ident $(, $($extra:tt)*)?) => {
        #[pymethods]
        impl $class {
            $($($extra)*)?

            #[new]
            #[pyo3(signature = (id, folder=PathBuf::from("."), watcher=None, show_hidden=false, multi_select=true, navigate=true, dirs_first=true, sort_by="name"))]
            #[allow(clippy::too_many_arguments)]
            fn new(
                id: String,
                folder: PathBuf,
                watcher: Option<PyRef<'_, FileWatcher>>,
                show_hidden: bool,
                multi_select: bool,
                navigate: bool,
                dirs_first: bool,
                sort_by: &str,
            ) -> PyResult<Self> {
                Ok($class {
                    view: FolderView::new(id, folder, watcher, show_hidden, multi_select, navigate, dirs_first, sort_by)?,
                })
            }

            /// The folder shown. Setting it enters another folder, like go
            #[getter]
            fn get_folder(&self) -> String {
                to_str(self.view.folder())
            }

            #[setter]
            fn set_folder(&mut self, folder: PathBuf) {
                self.view.go(&folder);
            }

            /// Enters a folder, kept in the history of back and forward
            fn go(&mut self, folder: PathBuf) {
                self.view.go(&folder);
            }

            /// Enters the parent folder and selects the folder we come from. Cmd+Up does the same
            fn up(&mut self) {
                self.view.up();
            }

            /// Goes back to the previous folder of the history
            fn back(&mut self) {
                self.view.back();
            }

            /// Goes forward in the history, after back
            fn forward(&mut self) {
                self.view.forward();
            }

            /// True if back has a folder to go to
            #[getter]
            fn can_go_back(&self) -> bool {
                self.view.position > 0
            }

            /// True if forward has a folder to go to
            #[getter]
            fn can_go_forward(&self) -> bool {
                self.view.position + 1 < self.view.history.len()
            }

            /// True during the frame the view entered another folder by itself: double click, Enter
            /// or Cmd+Up on a folder
            #[getter]
            fn folder_changed(&self) -> bool {
                self.view.folder_changed
            }

            /// Paths of the selected items, in the order they were selected
            #[getter]
            fn get_selected(&self) -> Vec<String> {
                self.view.selected.iter().map(|p| to_str(p)).collect()
            }

            #[setter]
            fn set_selected(&mut self, selected: Vec<PathBuf>) {
                self.view.selected = selected.iter().map(|p| expand(p)).collect();
                self.view.cursor = self.view.selected.last().cloned();
                self.view.anchor = self.view.cursor.clone();
            }

            /// Selects one item and scrolls to it
            fn reveal(&mut self, path: PathBuf) {
                let path = expand(&path);
                self.view.select_only(path.clone());
                self.view.scroll_to = Some(path);
            }

            /// Paths activated during the last frame: double click or Enter on files (on folders too
            /// when navigate is False)
            #[getter]
            fn activated(&self) -> Vec<String> {
                self.view.activated.iter().map(|p| to_str(p)).collect()
            }

            /// Paths of the items shown at the last frame, in their order
            #[getter]
            fn items(&self) -> Vec<String> {
                self.view.shown.iter().map(|p| to_str(p)).collect()
            }

            /// Only the items whose name contains this text are shown (without case). Empty shows all.
            /// It is the filter of the column "name"
            #[getter]
            fn get_filter(&self) -> String {
                self.view.columns.name_text().to_string()
            }

            #[setter]
            fn set_filter(&mut self, filter: String) {
                self.view.columns.set_name_text(filter);
            }

            /// Shows the hidden items: a name starting with a dot, or hidden by the system
            #[getter]
            fn get_show_hidden(&self) -> bool {
                self.view.show_hidden
            }

            #[setter]
            fn set_show_hidden(&mut self, show: bool) {
                self.view.show_hidden = show;
            }

            /// The order of the items: "name", "modified", "size", "kind", or the title of a column of your
            /// own that has a value function. A click on a title of the header changes it
            #[getter]
            fn get_sort_by(&self) -> String {
                self.view.columns.sort_key.clone()
            }

            #[setter]
            fn set_sort_by(&mut self, sort_by: String) {
                self.view.columns.sort_key = sort_by;
            }

            /// False reverses the order
            #[getter]
            fn get_ascending(&self) -> bool {
                self.view.columns.ascending
            }

            #[setter]
            fn set_ascending(&mut self, ascending: bool) {
                self.view.columns.ascending = ascending;
            }

            /// The widths the user gave to the columns by dragging their border, by column key ("name",
            /// "modified", "size", "kind" or the title of a column of your own). Save and give them back at
            /// the next run. A column without width has its default one
            #[getter]
            fn get_column_widths<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
                self.view.columns.widths_py(py)
            }

            #[setter]
            fn set_column_widths(&mut self, widths: HashMap<String, f32>) {
                self.view.columns.set_widths(widths);
            }

            /// The columns pinned at the left (by column key): they stay in place while the others scroll
            /// horizontally. The user pins a column with a right click on its title
            #[getter]
            fn get_pinned_left(&self) -> Vec<String> {
                self.view.columns.pinned_left.clone()
            }

            #[setter]
            fn set_pinned_left(&mut self, columns: Vec<String>) {
                self.view.columns.pinned_left = columns;
            }

            /// The columns pinned at the right (by column key)
            #[getter]
            fn get_pinned_right(&self) -> Vec<String> {
                self.view.columns.pinned_right.clone()
            }

            #[setter]
            fn set_pinned_right(&mut self, columns: Vec<String>) {
                self.view.columns.pinned_right = columns;
            }

            /// The keys of the columns in the order the user gave them by dragging their title. Empty while
            /// the order is the default one. Save and give it back at the next run
            #[getter]
            fn get_column_order(&self) -> Vec<String> {
                self.view.columns.order.clone()
            }

            #[setter]
            fn set_column_order(&mut self, order: Vec<String>) {
                self.view.columns.order = order;
            }

            /// The filters of the columns, by column key: a text for a column of text (the items whose value
            /// contains it, without case), a (min, max) tuple for a number or a date (None for no bound; a
            /// size is "10 KB", a date "2024-05-01"), a list of values for a column of choices. The funnel of
            /// the header sets them too. Assign a dict to replace them all
            #[getter]
            fn get_column_filters<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
                self.view.columns.filters_py(py)
            }

            #[setter]
            fn set_column_filters(&mut self, filters: &Bound<'_, PyDict>) -> PyResult<()> {
                self.view.columns.set_filters(filters)
            }

            /// Removes the filters of all the columns, the name included
            fn clear_filters(&mut self) {
                self.view.columns.filters.clear();
            }

            /// Asks the functions `value` of the columns of your own again at the next frame, e.g. after the
            /// tags changed. They are otherwise kept for a second
            fn refresh_columns(&mut self) {
                self.view.values.map.clear();
            }

            /// Gives the keyboard focus to the view (True, the default) or takes it away (False) at
            /// the next frame. A click on the view gives it the focus too
            #[pyo3(signature = (focused=true))]
            fn focus(&mut self, focused: bool) {
                self.view.focus = Some(focused);
            }

            /// If True, the view takes back the keyboard focus at every frame when another widget
            /// holds it, except a text field being edited. For an app where the view is the part of
            /// the window the keyboard acts on
            #[getter]
            fn get_keep_focus(&self) -> bool {
                self.view.keep_focus
            }

            #[setter]
            fn set_keep_focus(&mut self, keep: bool) {
                self.view.keep_focus = keep;
            }

            /// Scrolls just enough to show an item, at the next frame
            fn scroll_to(&mut self, path: PathBuf) {
                self.view.scroll_to = Some(expand(&path));
            }

            /// The items shown instead of the items of the folder, e.g. the result of a search or
            /// the files of a tag, from any folders. They stay up to date like a folder. None (the
            /// default) shows the folder; entering a folder sets it back to None
            #[getter]
            fn get_paths(&self) -> Option<Vec<String>> {
                self.view.paths.as_ref().map(|paths| paths.iter().map(|p| to_str(p)).collect())
            }

            #[setter]
            fn set_paths(&mut self, paths: Option<Vec<PathBuf>>) {
                let paths: Option<Vec<PathBuf>> = paths.map(|paths| paths.iter().map(|p| expand(p)).collect());
                if paths != self.view.paths {
                    self.view.paths = paths;
                }
            }
        }
    };
}

/// The state of a list of the files of a folder, with the columns Name, Date Modified, Size and
/// Kind. Create it once, outside the update function, and draw it with file_list_view.
///
/// The folder is watched while it is shown (see FileWatcher): what any program changes in it
/// shows at the next frame. A click selects, Cmd (Ctrl) and Shift extend the selection, a drag on
/// the background draws a rubber band, a double click or Enter opens. The arrows, Home, End, Page
/// Up and Page Down move, Cmd+A selects all, Cmd+Up enters the parent folder, and typing the start
/// of a name selects it.
///
/// The header of the columns is interactive: a click on a title sorts by the column (a second click
/// reverses the order), a drag on a title moves the column, a drag on the border of a title
/// resizes it (a double click gives it its default width back), and the funnel at the right of a
/// title filters it, with a filter that fits the type of the column: a text for the name, a minimum
/// and a maximum for the size, a period for the date, the values to keep for the kind. Everything
/// is also a property of the view (sort_by, ascending, column_widths, column_order,
/// column_filters), to save at the end of the run and give back at the next one.
///
/// Args:
///     id (str): unique id of the view
///
///     folder (str): the folder shown, ~ is the home folder. Default is the current folder
///
///     watcher (FileWatcher): the watcher of the folders. Default is one shared by the views created
///         without one
///
///     show_hidden (bool): shows the hidden items. Default is False
///
///     multi_select (bool): several items can be selected. Default is True
///
///     navigate (bool): a double click or Enter on a folder enters it. Default is True. With False
///         the folder is in activated, like a file
///
///     dirs_first (bool): folders before files. Default is True
///
///     sort_by (str): "name", "modified", "size", "kind" or the title of a column of your own that
///         has a value function. Default is "name"
///
/// Usage::
///
///     files = FileListView("files", "~/Documents")
///
///     def update_func(ctx):
///         if button_clicked("⬆"):
///             files.up()
///         label(files.folder)
///         file_list_view(files)
///         for path in files.activated:
///             ctx.open_url(Path(path).as_uri())
#[pyclass]
pub(crate) struct FileListView {
    view: FolderView,
}

folder_view_methods!(FileListView);

/// The state of a grid of the files of a folder, with large icons and the name under them. Create
/// it once, outside the update function, and draw it with file_icon_view.
///
/// It works like FileListView: the same arguments, selection, rubber band, keyboard (the four
/// arrows move in the grid) and the folder is watched while it is shown.
///
/// Args:
///     id (str): unique id of the view
///
///     folder (str): the folder shown, ~ is the home folder. Default is the current folder
///
///     watcher (FileWatcher): the watcher of the folders. Default is one shared by the views created
///         without one
///
///     show_hidden (bool): shows the hidden items. Default is False
///
///     multi_select (bool): several items can be selected. Default is True
///
///     navigate (bool): a double click or Enter on a folder enters it. Default is True
///
///     dirs_first (bool): folders before files. Default is True
///
///     sort_by (str): "name", "modified", "size", "kind" or the title of a column of your own that
///         has a value function. Default is "name"
///
/// The size of the grid is icon_size, which can change at any time, e.g. from a slider.
///
/// Usage::
///
///     icons = FileIconView("icons", "~/Pictures")
///     size = Float(64)
///
///     def update_func(ctx):
///         slider_float(size, 16, 256, "")
///         icons.icon_size = size.value  # the grid follows the slider
///         file_icon_view(icons)
///         if icons.selected:
///             label(f"{len(icons.selected)} selected")
#[pyclass]
pub(crate) struct FileIconView {
    view: FolderView,
}

/// The smallest and the largest icons of an icon view
const ICON_SIZES: (f32, f32) = (16.0, 256.0);

folder_view_methods!(FileIconView,
    /// The size of the icons, between 16 and 256: the tiles of the grid grow with it, so there
    /// are fewer of them on a row. Default is 48. Give it a slider to let the user choose, like the
    /// Finder. The icon_size keyword arg of file_icon_view wins over it
    #[getter]
    fn get_icon_size(&self) -> f32 {
        self.view.icon_size
    }

    #[setter]
    fn set_icon_size(&mut self, size: f32) {
        self.view.icon_size = size.clamp(ICON_SIZES.0, ICON_SIZES.1);
    }
);

fn parse_columns(kwargs: Option<&Bound<'_, PyDict>>, extras: usize) -> PyResult<Vec<(Column, f32)>> {
    let names = kwarg::<Vec<String>>(kwargs, "columns")?
        .unwrap_or_else(|| vec!["name".into(), "modified".into(), "size".into(), "kind".into()]);
    // the name, the columns of Python, then the others
    let mut columns = vec![(Column::Sort(SortBy::Name), 0.0)];
    columns.extend((0..extras).map(|i| (Column::Extra(i), 0.0)));
    for name in names {
        let column = SortBy::parse(&name)?;
        if column != SortBy::Name {
            columns.push((Column::Sort(column), 0.0));
        }
    }
    Ok(columns)
}

/// Draws a FileListView: the files of its folder as a list with columns. It takes the space left
/// in the ui and scrolls; only the visible rows are drawn, so large folders stay fast.
///
/// Args:
///     view (FileListView): the state of the view
///
/// Keyword args:
///     columns (list[str]): the columns after the name, among "modified", "size" and "kind".
///         Default is all three. A view narrower than its columns scrolls horizontally
///
///     headers (list[str]): the titles of the columns name, modified, size and kind. Default is
///         ["Name", "Date Modified", "Size", "Kind"]
///
///     date_format (str): format of the dates, like strftime. Default is "%d %b %Y %H:%M"
///
///     row_height (float): height of a row. Default is 24
///
///     extra_columns (list[tuple]): columns of your own, after the name: a title, a width and a
///         function called with the path of the row inside the cell, to draw it with the usual
///         widgets. They sort and filter when a fourth item, a dict, gives the type of their data
///         and a function that returns the value of a row: {"type": "text" | "number" | "date" |
///         "choice", "value": function(path) -> value}. A date is seconds since 1970 or a
///         datetime. The values are kept for a second (see refresh_columns). The function runs
///         while the view is drawn: it must not read or change the view itself
///
///     resizable (bool): the border of a column can be dragged. Default is True
///
///     reorderable (bool): the title of a column can be dragged to another place. Default is True
///
///     filterable (bool): the funnel of the header opens the filter of the column. Default is True
///
///     badges (callable): called with the path of each row shown, returns a list of colors drawn
///         as dots at the right end of the name, e.g. the tags of a file
///
///     context_menu (callable): called with the list of paths concerned on a right click, inside a
///         menu: the selection if the item clicked is in it, else the item clicked, and [] on the
///         background. Build the menu in it
///
/// Example::
///
///     files = FileListView("files", "~")
///
///     def menu(paths):
///         if paths and button_clicked("Show in Finder"):
///             ...
///         if button_clicked("Show hidden files"):
///             files.show_hidden = not files.show_hidden
///
///     def update_func(ctx):
///         file_list_view(files, columns=["modified", "size"], context_menu=menu)
///
/// A column of your own that sorts and filters::
///
///     def stars(path):
///         rating = ratings.get(path, 0)
///         label("★" * rating)
///
///     file_list_view(
///         files,
///         extra_columns=[("Rating", 80, stars, {"type": "number", "value": lambda p: ratings.get(p, 0)})],
///     )
///     files.column_filters = {"size": ("1 MB", None), "kind": ["PDF document"]}
#[pyfunction]
#[pyo3(signature = (view, **kwargs))]
unsafe fn file_list_view(view: &Bound<'_, FileListView>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let headers = match kwarg::<Vec<String>>(kwargs, "headers")? {
        Some(h) if h.len() == 4 => [h[0].clone(), h[1].clone(), h[2].clone(), h[3].clone()],
        Some(_) => return Err(PyValueError::new_err("headers needs 4 titles: name, modified, size and kind")),
        None => ["Name".into(), "Date Modified".into(), "Size".into(), "Kind".into()],
    };
    let hooks = Hooks::read(kwargs)?;
    let shape = Shape::List {
        columns: parse_columns(kwargs, hooks.extras.len())?,
        headers,
        date_format: kwarg::<String>(kwargs, "date_format")?.unwrap_or_else(|| "%d %b %Y %H:%M".into()),
        row_height: kwarg::<f32>(kwargs, "row_height")?.unwrap_or(24.0),
    };
    let menu = kwarg::<Bound<'_, PyAny>>(kwargs, "context_menu")?;
    let ui = current_ui(&UI)?;
    let menus = draw_folder(ui, &mut view.borrow_mut().view, shape, &hooks, menu.is_some(), HeaderOptions::read(kwargs, false)?);
    run_menus(menus, menu.as_ref())
}

/// Draws a FileIconView: the files of its folder as a grid of icons. It takes the space left in
/// the ui and scrolls; only the visible tiles are drawn.
///
/// Args:
///     view (FileIconView): the state of the view
///
/// Keyword args:
///     icon_size (float): size of the icons, instead of view.icon_size. Default is view.icon_size
///
///     badges (callable): called with the path of each tile shown, returns a list of colors drawn
///         as dots on the top left corner of the icon, e.g. the tags of a file
///
///     context_menu (callable): called with the list of paths concerned on a right click, inside a
///         menu, like for file_list_view
///
/// Example::
///
///     icons = FileIconView("icons", "~/Desktop")
///
///     def update_func(ctx):
///         file_icon_view(icons)
#[pyfunction]
#[pyo3(signature = (view, **kwargs))]
unsafe fn file_icon_view(view: &Bound<'_, FileIconView>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let size = view.borrow().view.icon_size;
    let hooks = Hooks::read(kwargs)?;
    let shape = Shape::Icons {
        icon: kwarg::<f32>(kwargs, "icon_size")?.unwrap_or(size).clamp(ICON_SIZES.0, ICON_SIZES.1),
        tile: Vec2Size { w: 1.0, h: 1.0 },
        per_row: 1,
    };
    let menu = kwarg::<Bound<'_, PyAny>>(kwargs, "context_menu")?;
    let ui = current_ui(&UI)?;
    let menus = draw_folder(ui, &mut view.borrow_mut().view, shape, &hooks, menu.is_some(), HeaderOptions::read(kwargs, false)?);
    run_menus(menus, menu.as_ref())
}

// tree view: a folder and the folders opened in it

/// The state of a tree of the folders and files under a root folder. Create it once, outside
/// the update function, and draw it with file_tree_view.
///
/// A folder is read when it is opened, and watched while it is open (see FileWatcher): what any
/// program changes in it shows at the next frame. The keyboard works like in TreeView: the
/// arrows move, Left and Right close and open a folder, Enter or a double click on a file
/// activates it.
///
/// Args:
///     id (str): unique id of the view
///
///     root (str): the folder at the top, ~ is the home folder. Default is the current folder
///
///     watcher (FileWatcher): the watcher of the folders. Default is one shared by the views created
///         without one
///
///     show_hidden (bool): shows the hidden items. Default is False
///
///     multi_select (bool): several items can be selected with Cmd (Ctrl) and Shift. Default is True
///
///     show_root (bool): the root folder is the first node and the rest is in it. Default is False:
///         the items of the root are at the top
///
///     dirs_first (bool): folders before files. Default is True
///
///     activatable_dirs (bool): a double click or Enter on a folder activates it, like a file (see
///         activated). Its arrow still opens and closes it. Default is False: a double click opens
///         and closes the folder
///
/// Usage::
///
///     tree = FileTreeView("tree", "~/projects/app")
///
///     def update_func(ctx):
///         file_tree_view(tree)
///         for path in tree.activated:
///             print("open", path)
#[pyclass]
pub(crate) struct FileTreeView {
    id: String,
    core: Arc<Mutex<Core>>,
    root: PathBuf,
    show_hidden: bool,
    multi_select: bool,
    show_root: bool,
    dirs_first: bool,
    /// width, order, sort and filters of the columns
    columns: ColumnsState,
    /// values of the columns of Python
    values: Values,
    /// the items shown at the last frame, for the choices of a filter (only kept when a column has some)
    listing: Arc<Vec<Entry>>,
    /// taken out while the tree is drawn, so the context menu can read the view
    state: TreeViewState<String>,
    /// what Python reads, also while the tree is drawn
    selected: Vec<String>,
    open: HashSet<String>,
    /// what Python asked for, given to the state at the next frame
    pending_selected: Option<Vec<String>>,
    pending_open: Vec<(String, bool)>,
    known_dirs: HashSet<String>,
    activated: Vec<String>,
    focus: Option<bool>,
    keep_focus: bool,
    scroll_to: Option<String>,
    activatable_dirs: bool,
    /// the items at the top instead of the items of the root
    paths: Option<Vec<PathBuf>>,
}

#[pymethods]
impl FileTreeView {
    #[new]
    #[pyo3(signature = (id, root=PathBuf::from("."), watcher=None, show_hidden=false, multi_select=true, show_root=false, dirs_first=true, activatable_dirs=false))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        id: String,
        root: PathBuf,
        watcher: Option<PyRef<'_, FileWatcher>>,
        show_hidden: bool,
        multi_select: bool,
        show_root: bool,
        dirs_first: bool,
        activatable_dirs: bool,
    ) -> Self {
        FileTreeView {
            id,
            core: core_of(watcher),
            root: expand(&root),
            show_hidden,
            multi_select,
            show_root,
            dirs_first,
            columns: ColumnsState::new("name"),
            values: Values::default(),
            listing: Arc::new(vec![]),
            state: TreeViewState::default(),
            selected: vec![],
            open: HashSet::new(),
            pending_selected: None,
            pending_open: vec![],
            known_dirs: HashSet::new(),
            activated: vec![],
            focus: None,
            keep_focus: false,
            scroll_to: None,
            activatable_dirs,
            paths: None,
        }
    }

    /// The folder at the top of the tree
    #[getter]
    fn get_root(&self) -> String {
        to_str(&self.root)
    }

    #[setter]
    fn set_root(&mut self, root: PathBuf) {
        self.root = expand(&root);
    }

    /// Paths of the selected items
    #[getter]
    fn get_selected(&self) -> Vec<String> {
        self.selected.clone()
    }

    #[setter]
    fn set_selected(&mut self, selected: Vec<PathBuf>) {
        self.selected = selected.iter().map(|p| to_str(&expand(p))).collect();
        self.pending_selected = Some(self.selected.clone());
    }

    /// Paths activated during the last frame by a double click or Enter: files, and folders when
    /// activatable_dirs is True
    #[getter]
    fn activated(&self) -> Vec<String> {
        self.activated.clone()
    }

    /// Opens or closes a folder of the tree
    fn set_open(&mut self, path: PathBuf, open: bool) {
        let path = to_str(&expand(&path));
        if open {
            self.open.insert(path.clone());
        } else {
            self.open.remove(&path);
        }
        self.pending_open.push((path, open));
    }

    /// True if the folder is open in the tree
    fn is_open(&self, path: PathBuf) -> bool {
        self.open.contains(&to_str(&expand(&path)))
    }

    /// The paths of the open folders, to save and give back at the next run
    #[getter]
    fn get_open_folders(&self) -> Vec<String> {
        let mut open: Vec<String> = self.open.iter().cloned().collect();
        open.sort();
        open
    }

    #[setter]
    fn set_open_folders(&mut self, folders: Vec<PathBuf>) {
        for path in self.open.clone() {
            self.set_open(PathBuf::from(path), false);
        }
        for path in folders {
            self.set_open(path, true);
        }
    }

    /// Opens the folders down to a path, selects it and scrolls to it
    fn reveal(&mut self, path: PathBuf) {
        let path = expand(&path);
        let mut folder = path.parent();
        while let Some(f) = folder {
            if !f.starts_with(&self.root) || (f == self.root && !self.show_root) {
                break;
            }
            self.set_open(f.to_path_buf(), true);
            folder = f.parent();
        }
        self.set_selected(vec![path.clone()]);
        self.scroll_to = Some(to_str(&path));
    }

    /// Only the files whose name contains this text are shown (without case), the folders always.
    /// It is the filter of the column "name"
    #[getter]
    fn get_filter(&self) -> String {
        self.columns.name_text().to_string()
    }

    #[setter]
    fn set_filter(&mut self, filter: String) {
        self.columns.set_name_text(filter);
    }

    /// Shows the hidden items: a name starting with a dot, or hidden by the system
    #[getter]
    fn get_show_hidden(&self) -> bool {
        self.show_hidden
    }

    #[setter]
    fn set_show_hidden(&mut self, show: bool) {
        self.show_hidden = show;
    }

    /// The order of the items: "name", "modified", "size", "kind", or the title of a column of your
    /// own that has a value function. A click on a title of the header changes it
    #[getter]
    fn get_sort_by(&self) -> String {
        self.columns.sort_key.clone()
    }

    #[setter]
    fn set_sort_by(&mut self, sort_by: String) {
        self.columns.sort_key = sort_by;
    }

    /// False reverses the order
    #[getter]
    fn get_ascending(&self) -> bool {
        self.columns.ascending
    }

    #[setter]
    fn set_ascending(&mut self, ascending: bool) {
        self.columns.ascending = ascending;
    }

    /// The widths the user gave to the columns by dragging their border, by column key ("name",
    /// "modified", "size", "kind" or the title of a column of your own). Save and give them back at
    /// the next run. A column without width has its default one
    #[getter]
    fn get_column_widths<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        self.columns.widths_py(py)
    }

    #[setter]
    fn set_column_widths(&mut self, widths: HashMap<String, f32>) {
        self.columns.set_widths(widths);
    }

    /// The columns pinned at the left (by column key): they stay in place while the others scroll
    /// horizontally. The user pins a column with a right click on its title
    #[getter]
    fn get_pinned_left(&self) -> Vec<String> {
        self.columns.pinned_left.clone()
    }

    #[setter]
    fn set_pinned_left(&mut self, columns: Vec<String>) {
        self.columns.pinned_left = columns;
    }

    /// The columns pinned at the right (by column key)
    #[getter]
    fn get_pinned_right(&self) -> Vec<String> {
        self.columns.pinned_right.clone()
    }

    #[setter]
    fn set_pinned_right(&mut self, columns: Vec<String>) {
        self.columns.pinned_right = columns;
    }

    /// The keys of the columns in the order the user gave them by dragging their title. Empty while
    /// the order is the default one. Save and give it back at the next run
    #[getter]
    fn get_column_order(&self) -> Vec<String> {
        self.columns.order.clone()
    }

    #[setter]
    fn set_column_order(&mut self, order: Vec<String>) {
        self.columns.order = order;
    }

    /// The filters of the columns, by column key: a text for a column of text (the items whose value
    /// contains it, without case), a (min, max) tuple for a number or a date (None for no bound; a
    /// size is "10 KB", a date "2024-05-01"), a list of values for a column of choices. The funnel of
    /// the header sets them too. Assign a dict to replace them all
    #[getter]
    fn get_column_filters<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        self.columns.filters_py(py)
    }

    #[setter]
    fn set_column_filters(&mut self, filters: &Bound<'_, PyDict>) -> PyResult<()> {
        self.columns.set_filters(filters)
    }

    /// Removes the filters of all the columns, the name included
    fn clear_filters(&mut self) {
        self.columns.filters.clear();
    }

    /// Asks the functions `value` of the columns of your own again at the next frame, e.g. after the
    /// tags changed. They are otherwise kept for a second
    fn refresh_columns(&mut self) {
        self.values.map.clear();
    }

    /// Gives the keyboard focus to the tree (True, the default) or takes it away (False) at the
    /// next frame
    #[pyo3(signature = (focused=true))]
    fn focus(&mut self, focused: bool) {
        self.focus = Some(focused);
    }

    /// If True, the tree takes back the keyboard focus at every frame when another widget holds
    /// it, except a text field being edited
    #[getter]
    fn get_keep_focus(&self) -> bool {
        self.keep_focus
    }

    #[setter]
    fn set_keep_focus(&mut self, keep: bool) {
        self.keep_focus = keep;
    }

    /// Scrolls just enough to show an item, at the next frame. It has to be shown: its folders open
    fn scroll_to(&mut self, path: PathBuf) {
        self.scroll_to = Some(to_str(&expand(&path)));
    }

    /// The items at the top of the tree instead of the items of the root, e.g. the result of a
    /// search or the files of a tag, from any folders; their folders still open. None (the
    /// default) shows the root
    #[getter]
    fn get_paths(&self) -> Option<Vec<String>> {
        self.paths.as_ref().map(|paths| paths.iter().map(|p| to_str(p)).collect())
    }

    #[setter]
    fn set_paths(&mut self, paths: Option<Vec<PathBuf>>) {
        self.paths = paths.map(|paths| paths.iter().map(|p| expand(p)).collect());
    }
}

/// A node of the tree, built before it is drawn
struct Node {
    entry: Entry,
    id: String,
    badges: Vec<Color32>,
    /// the items of an open folder
    children: Option<Vec<Node>>,
}

struct TreeOptions {
    show_hidden: bool,
}

/// The items of a folder as nodes, in the order of the disk: arrange puts them in order once the
/// listing is free again
fn tree_children(
    core: &mut Core,
    ctx: &egui::Context,
    dir: &Path,
    options: &TreeOptions,
    state: &TreeViewState<String>,
    known_dirs: &mut HashSet<String>,
) -> Vec<Node> {
    let all = core.list(Some(ctx), dir);
    let entries: Vec<&Entry> = all.iter().filter(|e| options.show_hidden || !e.hidden).collect();
    entries
        .into_iter()
        .map(|e| tree_node(core, ctx, e.clone(), options, state, known_dirs, false))
        .collect()
}

/// The nodes sorted and filtered by the columns, the folders inside too. `top` is the level of the
/// first nodes: sorted unless the items were given by the Python code
fn arrange_nodes(nodes: Vec<Node>, rules: &Rules, hooks: &Hooks<'_>, values: &mut Values, sort_top: bool) -> Vec<Node> {
    let mut nodes = arrange(nodes, |n| &n.entry, rules, hooks, values, false, sort_top);
    for node in &mut nodes {
        if let Some(children) = node.children.take() {
            node.children = Some(arrange_nodes(children, rules, hooks, values, true));
        }
    }
    nodes
}

fn collect_entries(nodes: &[Node], into: &mut Vec<Entry>) {
    for node in nodes {
        into.push(node.entry.clone());
        if let Some(children) = &node.children {
            collect_entries(children, into);
        }
    }
}

/// The columns at the right end of the rows of the tree, in the order of the user: the columns of
/// Python, then the ones of the view that were asked for
fn tree_columns(state: &ColumnsState, hooks: &Hooks<'_>, builtin: &[SortBy]) -> Vec<(Column, f32)> {
    let wanted: Vec<Column> =
        (0..hooks.extras.len()).map(Column::Extra).chain(builtin.iter().map(|s| Column::Sort(*s))).collect();
    let default: Vec<String> = wanted.iter().map(|c| c.key(hooks)).collect();
    state
        .ordered(&default)
        .iter()
        .filter_map(|key| wanted.iter().copied().find(|c| c.key(hooks) == *key))
        .map(|c| (c, state.widths.get(&c.key(hooks)).copied().unwrap_or_else(|| default_width(c, hooks))))
        .collect()
}

/// The titles of the columns of the view: name, modified, size and kind
fn parse_headers(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<[String; 4]> {
    Ok(match kwarg::<Vec<String>>(kwargs, "headers")? {
        Some(h) if h.len() == 4 => [h[0].clone(), h[1].clone(), h[2].clone(), h[3].clone()],
        Some(_) => return Err(PyValueError::new_err("headers needs 4 titles: name, modified, size and kind")),
        None => ["Name".into(), "Date Modified".into(), "Size".into(), "Kind".into()],
    })
}

fn tree_node(
    core: &mut Core,
    ctx: &egui::Context,
    entry: Entry,
    options: &TreeOptions,
    state: &TreeViewState<String>,
    known_dirs: &mut HashSet<String>,
    open_by_default: bool,
) -> Node {
    let id = to_str(&entry.path);
    let children = if entry.is_dir {
        known_dirs.insert(id.clone());
        state
            .is_open(&id)
            .unwrap_or(open_by_default)
            .then(|| tree_children(core, ctx, &entry.path, options, state, known_dirs))
    } else {
        None
    };
    Node { entry, id, badges: vec![], children }
}

/// The badges of the nodes, asked to Python once the view is free again
fn fill_badges(nodes: &mut [Node], hooks: &Hooks<'_>) {
    for node in nodes {
        node.badges = hooks.badges(&node.entry.path);
        if let Some(children) = &mut node.children {
            fill_badges(children, hooks);
        }
    }
}

/// Where the columns are in the tree: the name (always pinned at the left) and the columns, in
/// the three zones of the pinned columns
#[derive(Clone)]
struct TreeLayout {
    width: f32,
    name: f32,
    zones: Zones,
    scroll: f32,
    /// the width of the tree is known: the header can be drawn
    finite: bool,
}

impl TreeLayout {
    /// The width of the name: the one the user chose, else what the columns leave. Without a width
    /// (a tree in a ui as wide as it wants) the name has none and the columns end the rows
    fn new(width: f32, columns: &[(Column, f32)], hooks: &Hooks<'_>, state: &ColumnsState) -> Self {
        let total = columns.iter().map(|(_, w)| w).sum::<f32>();
        let finite = width.is_finite();
        let (width, name) = if finite {
            (width, state.widths.get("name").copied().unwrap_or((width - total).max(120.0)))
        } else {
            (total, 0.0)
        };
        let mut keys = vec!["name".to_string()];
        keys.extend(columns.iter().map(|(c, _)| c.key(hooks)));
        let mut widths = vec![name];
        widths.extend(columns.iter().map(|(_, w)| *w));
        let zones = Zones::new(&keys, &widths, state, width, Some("name"));
        let scroll = state.scroll.clamp(0.0, zones.max_scroll());
        TreeLayout { width, name, zones, scroll, finite }
    }
}

/// The right part of a row of the tree: the columns, from the right of the name, and the badges
/// at the end of the name. The cells are placed from the row, the same way as the titles of the
/// header, so they stay under them whatever the room
fn with_tree_accessory<'a>(
    b: NodeBuilder<'a, String>,
    badges: Vec<Color32>,
    entry: &'a Entry,
    hooks: &'a Hooks<'_>,
    columns: &'a [(Column, f32)],
    layout: Option<&'a TreeLayout>,
    date_format: &'a str,
) -> NodeBuilder<'a, String> {
    let Some(layout) = layout else { return b };
    if badges.is_empty() && columns.is_empty() {
        return b;
    }
    b.accessory(move |ui| {
        let area = ui.max_rect();
        let row = Rect::from_min_max(pos2(area.right() - layout.width, area.top()), area.max);
        // the left edge of the first column
        let first = row.left() + layout.name;
        for placed in layout.zones.placed.iter().filter(|p| p.index > 0) {
            let (column, width) = &columns[placed.index - 1];
            let cell = layout.zones.cell_rect(row, placed, layout.scroll);
            let visible = layout.zones.visible(row, placed, layout.scroll);
            if visible.width() <= 0.0 {
                continue;
            }
            match column {
                Column::Extra(k) => {
                    let extra = &hooks.extras[*k];
                    let mut cell_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(cell.shrink2(vec2(4.0, 0.0)))
                            .layout(egui::Layout::left_to_right(Align::Center)),
                    );
                    cell_ui.shrink_clip_rect(visible);
                    if let Err(err) = unsafe { run_nested_call(&mut cell_ui, &extra.cell, (to_str(&entry.path),)) } {
                        err.display(extra.cell.py());
                    }
                }
                Column::Sort(sort) => {
                    let text = match sort {
                        SortBy::Modified => date(entry.modified, date_format),
                        SortBy::Size if entry.is_dir => "--".into(),
                        SortBy::Size => human_size(entry.size),
                        _ => kind(entry),
                    };
                    let color = ui.visuals().weak_text_color();
                    let galley = truncated(ui, &text, width - 8.0, Some(color));
                    let left = if *sort == SortBy::Size { cell.right() - 4.0 - galley.size().x } else { cell.left() + 4.0 };
                    ui.painter().with_clip_rect(visible).galley(pos2(left, cell.center().y - galley.size().y / 2.0), galley, color);
                }
            }
        }
        if !badges.is_empty() {
            let dots = dots_width(&badges, 9.0);
            paint_dots(ui, &badges, first - dots - 8.0, area.center().y, 9.0);
        }
    })
}

/// The header above the tree: the name at the left, the columns over the cells of the rows. It is
/// pinned: `slot` is the room kept for it at the top of the content, and it is drawn over the rows
/// at the top of what can be seen when the tree is scrolled
fn tree_header(
    ui: &mut egui::Ui,
    id: Id,
    view: &Bound<'_, FileTreeView>,
    columns: &[(Column, f32)],
    headers: &[String; 4],
    hooks: &Hooks<'_>,
    opts: HeaderOptions,
    slot: Rect,
    layout: &TreeLayout,
) {
    let shift = (ui.clip_rect().top() - slot.top()).max(0.0);
    let rect = slot.translate(vec2(0.0, shift));
    if shift > 0.5 {
        ui.painter().rect_filled(rect, 0.0, ui.visuals().panel_fill);
    }
    let mut cells: Vec<Cell> = vec![Cell {
        key: "name".into(),
        title: headers[0].clone(),
        rect,
        ty: DataType::Text,
        sortable: true,
        filterable: true,
        movable: false,
        resizable: true,
    }];
    for (column, _) in columns {
        let title = match column {
            Column::Sort(sort) => headers[*sort as usize].clone(),
            Column::Extra(k) => hooks.extras[*k].title.clone(),
        };
        cells.push(Cell {
            key: column.key(hooks),
            title,
            rect,
            ty: column.data_type(hooks),
            sortable: column.has_value(hooks),
            filterable: column.has_value(hooks),
            movable: true,
            resizable: true,
        });
    }
    let listing = view.borrow().listing.clone();
    let mut values = std::mem::take(&mut view.borrow_mut().values);
    let mut v = view.borrow_mut();
    let before = (v.columns.sort_key.clone(), v.columns.ascending, v.columns.filters.clone(), v.columns.widths.clone(), v.columns.order.clone());
    columns::zoned_header(ui, id.with("columns"), rect, &layout.zones, cells, &mut v.columns, opts, &mut |key| {
        distinct_values(listing.iter(), key, hooks, &mut values)
    });
    let after = (v.columns.sort_key.clone(), v.columns.ascending, v.columns.filters.clone(), v.columns.widths.clone(), v.columns.order.clone());
    if before != after {
        ui.ctx().request_repaint(); // the rows follow at the next frame
    }
    v.values = values;
    ui.painter().hline(rect.x_range(), rect.bottom(), egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color));
}

fn with_tree_icon(b: NodeBuilder<'_, String>, uri: Option<String>, side: f32, hidden: bool) -> NodeBuilder<'_, String> {
    match uri {
        Some(uri) => b.icon(move |ui| {
            if hidden {
                ui.multiply_opacity(HIDDEN_OPACITY);
            }
            ui.add(egui::Image::new(uri.as_str()).fit_to_exact_size(vec2(side, side)));
            ui.add_space(4.0);
        }),
        None => b,
    }
}

fn add_tree_nodes<'a>(
    builder: &mut TreeViewBuilder<'_, String>,
    nodes: &'a [Node],
    ctx: &egui::Context,
    open_by_default: bool,
    activatable_dirs: bool,
    hooks: &'a Hooks<'_>,
    columns: &'a [(Column, f32)],
    layout: Option<&'a TreeLayout>,
    date_format: &'a str,
) {
    for node in nodes {
        let side = 16.0;
        let uri = icon_uri(ctx, &node.id, side);
        let label = match uri {
            Some(_) => node.entry.name.clone(),
            None => format!("{}  {}", emoji(&node.entry), node.entry.name),
        };
        // a hidden item: its name faded and in italics, like its icon
        let label = if node.entry.hidden {
            egui::RichText::new(label).weak().italics()
        } else {
            egui::RichText::new(label)
        };
        let with_icon = |b| {
            with_tree_accessory(with_tree_icon(b, uri.clone(), side, node.entry.hidden), node.badges.clone(), &node.entry, hooks, columns, layout, date_format)
        };
        if node.entry.is_dir {
            builder.node(with_icon(
                NodeBuilder::dir(node.id.clone())
                    .default_open(open_by_default)
                    .activatable(activatable_dirs)
                    .label(label),
            ));
            if let Some(children) = &node.children {
                add_tree_nodes(builder, children, ctx, false, activatable_dirs, hooks, columns, layout, date_format);
            }
            builder.close_dir();
        } else {
            builder.node(with_icon(NodeBuilder::leaf(node.id.clone()).label(label)));
        }
    }
}

/// The row of a node among the rows shown
fn tree_row(nodes: &[Node], id: &str, row: &mut usize) -> Option<usize> {
    for node in nodes {
        if node.id == id {
            return Some(*row);
        }
        *row += 1;
        if let Some(children) = &node.children {
            if let Some(found) = tree_row(children, id, row) {
                return Some(found);
            }
        }
    }
    None
}

/// Draws a FileTreeView: the folders and files under its root, as a tree. The folders are read
/// when they are opened and watched while they are open.
///
/// Args:
///     view (FileTreeView): the state of the view
///
/// Keyword args:
///     context_menu (callable): called with the list of paths concerned when an item is
///         right-clicked, inside a menu: the selection if the item clicked is in it, else the item
///         clicked. Build the menu in it
///
///     badges (callable): called with the path of each item, returns a list of colors drawn as
///         dots at the right end of its row, e.g. the tags of a file
///
///     columns (list[str]): columns of the view at the right end of the rows, among "modified",
///         "size" and "kind". Default is none, the tree is then only names
///
///     headers (list[str]): the titles of the columns name, modified, size and kind. Default is
///         ["Name", "Date Modified", "Size", "Kind"]
///
///     date_format (str): format of the dates, like strftime. Default is "%d %b %Y %H:%M"
///
///     extra_columns (list[tuple]): columns of your own at the right end of the rows: a title
///         (shown above the tree), a width, and a function called with the path of each row to draw
///         its cell, e.g. ("Tags", 160, lambda path: tags_edit(...)). A fourth item, a dict, makes
///         the column sort and filter: {"type": "text" | "number" | "date" | "choice", "value":
///         function(path) -> value}. A date is seconds since 1970 or a datetime
///
///     header (bool): draws the header of the columns above the tree, pinned at the top while the
///         tree scrolls. Default is True when there are columns
///
///     resizable (bool): the border of a column can be dragged. Default is True
///
///     reorderable (bool): the title of a column can be dragged to another place. Default is True
///
///     pinnable (bool): a right click on a title offers to pin the column at the left (after the
///         name) or at the right: the other columns then scroll horizontally. Default is True
///
///     filterable (bool): the funnel of the header opens the filter of the column, which fits its
///         type: a text, a minimum and a maximum, a period, or the values to keep. The folders
///         stay, only the files are filtered. Default is True
///
/// A click on a title sorts the items of each folder by the column, a second click reverses the
/// order. See sort_by, ascending, column_widths, column_order and column_filters of the view to
/// read, save and set them.
///
/// Example::
///
///     tree = FileTreeView("tree", "~/projects")
///
///     def update_func(ctx):
///         left_panel("side", lambda: scroll_area_both(lambda: file_tree_view(tree)))
///         label(f"selected: {tree.selected}")
#[pyfunction]
#[pyo3(signature = (view, **kwargs))]
unsafe fn file_tree_view(py: Python<'_>, view: &Bound<'_, FileTreeView>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let menu = kwarg::<Bound<'_, PyAny>>(kwargs, "context_menu")?;
    let hooks = Hooks::read(kwargs)?;
    let opts = HeaderOptions::read(kwargs, false)?;
    let headers = parse_headers(kwargs)?;
    let date_format = kwarg::<String>(kwargs, "date_format")?.unwrap_or_else(|| "%d %b %Y %H:%M".into());
    let mut builtin = vec![];
    for name in kwarg::<Vec<String>>(kwargs, "columns")?.unwrap_or_default() {
        let sort = SortBy::parse(&name)?;
        if sort != SortBy::Name && !builtin.contains(&sort) {
            builtin.push(sort);
        }
    }
    let ui = current_ui(&UI)?;
    let ctx = ui.ctx().clone();

    let id = ui.make_persistent_id(&view.borrow().id);
    let columns = tree_columns(&view.borrow().columns, &hooks, &builtin);
    let mut values = std::mem::take(&mut view.borrow_mut().values);
    values.refresh();
    let available = ui.available_width();
    let mut layout = Some(TreeLayout::new(available, &columns, &hooks, &view.borrow().columns));
    let origin_x = ui.cursor().min.x;
    // the room of the header, which is drawn once the rows are, over them
    let slot = match (&layout, kwarg::<bool>(kwargs, "header")?.unwrap_or(true)) {
        (Some(layout), true) if layout.finite && !columns.is_empty() => Some(ui.allocate_exact_size(vec2(layout.width, 24.0), Sense::hover()).0),
        _ => None,
    };
    // the columns that are not pinned scroll with the wheel over the tree
    if let Some(layout) = layout.as_mut().filter(|l| l.finite) {
        let clip = ui.clip_rect();
        let seen = Rect::from_min_max(pos2(origin_x, clip.top()), pos2(origin_x + layout.width, clip.bottom()));
        let mut v = view.borrow_mut();
        hscroll_wheel(ui, &mut v.columns, &layout.zones, seen);
        layout.scroll = v.columns.scroll;
    }

    let (mut state, mut nodes, multi_select, show_root, activatable_dirs, scroll_to, rules, given) = {
        let mut v = view.borrow_mut();
        let mut state = std::mem::take(&mut v.state);
        for (path, open) in std::mem::take(&mut v.pending_open) {
            state.set_openness(path, open);
        }
        if let Some(selected) = v.pending_selected.take() {
            state.set_selected(selected);
        }
        // what was removed from the disk is no longer selected
        let selected: Vec<String> = state.selected().iter().filter(|p| Path::new(p).exists()).cloned().collect();
        if selected.len() != state.selected().len() {
            state.set_selected(selected);
        }
        let options = TreeOptions { show_hidden: v.show_hidden };
        let rules = Rules::new(&v.columns, &hooks, v.dirs_first);
        let core = v.core.clone();
        let mut core = lock(&core);
        let mut known = std::mem::take(&mut v.known_dirs);
        let given = v.paths.is_some();
        let nodes = if let Some(paths) = v.paths.clone() {
            // given paths at the top: each one from the listing of its folder
            let mut top = vec![];
            for path in &paths {
                let Some(parent) = path.parent() else { continue };
                let found = core.list(Some(&ctx), parent).iter().find(|e| same_path(&e.path, path)).cloned();
                if let Some(e) = found.filter(|e| options.show_hidden || !e.hidden) {
                    top.push(tree_node(&mut core, &ctx, e, &options, &state, &mut known, false));
                }
            }
            top
        } else if v.show_root {
            match entry(&v.root) {
                Some(root) => vec![tree_node(&mut core, &ctx, root, &options, &state, &mut known, true)],
                None => vec![],
            }
        } else {
            tree_children(&mut core, &ctx, &v.root, &options, &state, &mut known)
        };
        v.known_dirs = known;
        match v.focus.take() {
            Some(true) => ui.memory_mut(|m| m.request_focus(id)),
            Some(false) => ui.memory_mut(|m| m.surrender_focus(id)),
            None if v.keep_focus && !ctx.text_edit_focused() => ui.memory_mut(|m| m.request_focus(id)),
            None => {}
        }
        (state, nodes, v.multi_select, v.show_root, v.activatable_dirs, v.scroll_to.take(), rules, given)
    };
    // the folders are read and free: the functions of Python of the columns can run
    nodes = arrange_nodes(nodes, &rules, &hooks, &mut values, !given);
    fill_badges(&mut nodes, &hooks);
    {
        let mut v = view.borrow_mut();
        v.listing = Arc::new(if columns.iter().any(|(c, _)| c.data_type(&hooks) == DataType::Choice) {
            let mut all = vec![];
            collect_entries(&nodes, &mut all);
            all
        } else {
            vec![]
        });
        v.values = std::mem::take(&mut values);
    }

    if let Some(target) = &scroll_to {
        // the rows out of the window are not drawn, so the row is placed from its index
        if let Some(index) = tree_row(&nodes, target, &mut 0) {
            let height = ui.spacing().interact_size.y;
            let top = ui.cursor().min + vec2(0.0, index as f32 * (height + ui.spacing().item_spacing.y));
            ui.scroll_to_rect(Rect::from_min_size(top, vec2(1.0, height)), None);
        }
    }

    // the view is free while the tree is drawn: the context menu can read and change it
    let mut menu_result = Ok(());
    let mut tree = egui_ltreeview::TreeView::new(id)
        .allow_multi_selection(multi_select)
        .allow_drag_and_drop(false);
    if let Some(menu_fun) = &menu {
        tree = tree.fallback_context_menu(|menu_ui, ids: &Vec<String>| {
            menu_result = run_nested_call(menu_ui, menu_fun, (ids.clone(),));
        });
    }
    let (_, actions) = tree.show_state(ui, &mut state, |builder| {
        add_tree_nodes(builder, &nodes, &ctx, show_root, activatable_dirs, &hooks, &columns, layout.as_ref(), &date_format)
    });

    if let (Some(slot), Some(layout)) = (slot, &layout) {
        tree_header(ui, id, view, &columns, &headers, &hooks, opts, slot, layout);
    }
    if let Some(layout) = layout.as_ref().filter(|l| l.finite) {
        let clip = ui.clip_rect();
        let seen = Rect::from_min_max(pos2(origin_x, clip.top()), pos2(origin_x + layout.width, clip.bottom()));
        hscroll_bar(ui, id, &mut view.borrow_mut().columns, &layout.zones, seen);
    }

    let mut v = view.borrow_mut();
    v.activated.clear();
    for action in actions {
        if let Action::Activate(activate) = action {
            v.activated = activate.selected;
        }
    }
    // a change asked for by the menu wins over the state of this frame
    if let Some(selected) = v.pending_selected.clone() {
        state.set_selected(selected);
    }
    v.selected = state.selected().clone();
    let open: HashSet<String> = v
        .known_dirs
        .iter()
        .filter(|d| state.is_open(d).unwrap_or(false))
        .cloned()
        .collect();
    v.open = open;
    v.state = state;
    let _ = py;
    menu_result
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<FileListView>()?;
    m.add_class::<FileIconView>()?;
    m.add_class::<FileTreeView>()?;
    m.add_function(wrap_pyfunction!(file_list_view, m)?)?;
    m.add_function(wrap_pyfunction!(file_icon_view, m)?)?;
    m.add_function(wrap_pyfunction!(file_tree_view, m)?)?;
    Ok(())
}
