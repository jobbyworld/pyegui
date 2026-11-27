//! File views: FileListView, FileIconView and FileTreeView show folders of the disk, with
//! selection, keyboard, sorting and native icons. Their folders are listed through a FileWatcher,
//! so a change made by any program shows at the next frame.


use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use egui::{
    self, pos2, vec2, Align, Align2, Color32, EventFilter, FontId, Id, Key, Pos2, Rect, Response, Sense,
    Stroke, TextStyle, TextWrapMode,
};
use egui_ltreeview::{Action, NodeBuilder, TreeViewBuilder, TreeViewState};

use crate::columns::{self, hscroll_bar, hscroll_wheel, Cell, ColumnsState, DataType, Filter, HeaderOptions, Value, Zones};
use crate::watcher::{entry, expand, lock, natural_cmp, Core, Entry};

/// What the system gives to the views for the drag and drop with the Finder: the pointer while
/// files are dragged over the window, and the start of a drag of files out of it. The application
/// gives them once (the Python extension does, the Rust ones may); without them a view just does
/// not take part in the drag and drop of the system
#[derive(Clone, Copy)]
pub struct NativeDnd {
    pub pointer: fn() -> Option<Pos2>,
    pub start_drag: fn(&[PathBuf]) -> bool,
}

static DND: OnceLock<NativeDnd> = OnceLock::new();

pub fn set_native_dnd(dnd: NativeDnd) {
    let _ = DND.set(dnd);
}

pub fn native_pointer() -> Option<Pos2> {
    DND.get().and_then(|d| (d.pointer)())
}

pub fn start_drag(paths: &[PathBuf]) -> bool {
    DND.get().is_some_and(|d| (d.start_drag)(paths))
}

/// A state shared with the code that draws and the code that reads it, locked for a moment at a
/// time: a context menu or a callback may read the view while it is drawn
pub struct Shared<T>(Arc<Mutex<T>>);

impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Shared(Arc::new(Mutex::new(value)))
    }
    pub fn borrow(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn borrow_mut(&self) -> MutexGuard<'_, T> {
        self.borrow()
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Shared(self.0.clone())
    }
}

/// The same path, where the disk does not tell capitals apart (Windows): the tags of the Finder
/// keep their paths in lower case
pub fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().replace('/', "\\").to_lowercase() == b.to_string_lossy().replace('/', "\\").to_lowercase()
    } else {
        a == b
    }
}

// what the three views share
pub fn to_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn human_size(size: u64) -> String {
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

pub fn kind(e: &Entry) -> String {
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

/// The date of an item. `format` is a strftime format or `HUMAN_DATES` for the French wording of the
/// Finder ("il y a 5 min", "Hier à 09:10", "12 mars", "12 mars 2025")
pub fn date(time: Option<SystemTime>, format: &str) -> String {
    match time {
        Some(time) => {
            let local = chrono::DateTime::<chrono::Local>::from(time);
            if format == HUMAN_DATES {
                crate::human_date::human_datetime(local.naive_local(), chrono::Local::now().naive_local())
            } else {
                local.format(format).to_string()
            }
        }
        None => "--".into(),
    }
}

/// The `date_format` that says the dates in words, in French
pub const HUMAN_DATES: &str = "human";

/// How much of its color a hidden item keeps in the views
pub const HIDDEN_OPACITY: f32 = 0.5;

pub fn emoji(e: &Entry) -> &'static str {
    if e.is_dir {
        "📁"
    } else {
        "📄"
    }
}

/// Paints the native icon of the entry, or an emoji where the platform gives none
pub fn paint_icon(ui: &egui::Ui, e: &Entry, rect: Rect) {
    match crate::icons::entry_icon_uri(ui.ctx(), &e.path, e.is_dir, rect.width()) {
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
pub fn selection_colors(ui: &egui::Ui, focused: bool) -> (Color32, Color32) {
    let v = ui.visuals();
    if focused {
        (v.selection.bg_fill, v.selection.stroke.color)
    } else {
        (v.text_color().gamma_multiply(0.15), v.strong_text_color())
    }
}

pub fn truncated(ui: &egui::Ui, text: &str, width: f32, color: Option<Color32>) -> Arc<egui::Galley> {
    let mut text = egui::RichText::new(text);
    if let Some(color) = color {
        text = text.color(color);
    }
    egui::WidgetText::from(text).into_galley(ui, Some(TextWrapMode::Truncate), width.max(1.0), TextStyle::Body)
}

/// The keys of a view, read once per frame when it has the focus
#[derive(Default)]
pub struct Keys {
    pub down: bool,
    pub up: bool,
    pub left: bool,
    pub right: bool,
    pub home: bool,
    pub end: bool,
    pub page_up: bool,
    pub page_down: bool,
    pub enter: bool,
    pub select_all: bool,
    pub parent: bool,
    pub shift: bool,
    pub text: String,
}

pub fn read_keys(ui: &egui::Ui) -> Keys {
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
pub fn lock_arrows(ui: &egui::Ui, id: Id) {
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
pub type Menus = Vec<(Response, Vec<String>)>;

/// What the application adds to the items of a view: colored dots (badges) and, for the list,
/// columns it draws itself
#[derive(Default)]
pub struct Hooks<'a> {
    /// the colors of the dots of an item
    pub badges: Option<Box<dyn Fn(&Path) -> Vec<Color32> + 'a>>,
    pub extras: Vec<Extra<'a>>,
}

/// A column of a list drawn by the application
pub struct Extra<'a> {
    pub title: String,
    pub width: f32,
    /// draws the cell of a row, in a ui of the size of the cell
    pub cell: Box<dyn Fn(&mut egui::Ui, &Path) + 'a>,
    /// what the column holds, for its sort and its filter
    pub ty: DataType,
    /// a small icon at the left of the title
    pub icon: Option<crate::columns::ColumnIcon>,
    /// the value of a row: without it the column does not sort or filter
    pub value: Option<Box<dyn Fn(&Path) -> Value + 'a>>,
}

impl Hooks<'_> {
    pub fn badges(&self, path: &Path) -> Vec<Color32> {
        self.badges.as_ref().map_or_else(Vec::new, |f| f(path))
    }
}

/// The dots of the badges, from `left` to the right, centered on `y`
pub fn paint_dots(ui: &egui::Ui, colors: &[Color32], left: f32, y: f32, size: f32) {
    for (i, color) in colors.iter().enumerate() {
        let center = pos2(left + size / 2.0 + i as f32 * (size + 3.0), y);
        ui.painter().circle_filled(center, size / 2.0, *color);
    }
}

pub fn dots_width(colors: &[Color32], size: f32) -> f32 {
    if colors.is_empty() {
        0.0
    } else {
        colors.len() as f32 * (size + 3.0) - 3.0
    }
}

// list and icon views: one folder

#[derive(Clone, Copy, PartialEq)]
pub enum SortBy {
    Name,
    Modified,
    Size,
    Kind,
}

impl SortBy {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "name" => SortBy::Name,
            "modified" => SortBy::Modified,
            "size" => SortBy::Size,
            "kind" => SortBy::Kind,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            SortBy::Name => "name",
            SortBy::Modified => "modified",
            SortBy::Size => "size",
            SortBy::Kind => "kind",
        }
    }

    pub fn data_type(self) -> DataType {
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
pub enum Column {
    Sort(SortBy),
    Extra(usize),
}

impl Column {
    /// The key of the column in the state of the view: its name, or the title of an extra column
    pub fn key(self, hooks: &Hooks<'_>) -> String {
        match self {
            Column::Sort(sort) => sort.name().into(),
            Column::Extra(i) => hooks.extras[i].title.clone(),
        }
    }

    pub fn data_type(self, hooks: &Hooks<'_>) -> DataType {
        match self {
            Column::Sort(sort) => sort.data_type(),
            Column::Extra(i) => hooks.extras[i].ty,
        }
    }

    /// Sorts and filters: a column of Python needs a value function
    pub fn has_value(self, hooks: &Hooks<'_>) -> bool {
        match self {
            Column::Sort(_) => true,
            Column::Extra(i) => hooks.extras[i].value.is_some(),
        }
    }

    pub fn of_key(key: &str, hooks: &Hooks<'_>) -> Option<Column> {
        if let Some(sort) = SortBy::parse(key) {
            return Some(Column::Sort(sort));
        }
        hooks.extras.iter().position(|e| e.title == key).map(Column::Extra)
    }
}

/// The values of the columns of Python, kept for a second: the functions are not called at every
/// frame for every item
#[derive(Default)]
pub struct Values {
    pub map: HashMap<(usize, PathBuf), Value>,
    pub since: Option<Instant>,
}

impl Values {
    pub fn refresh(&mut self) {
        if self.since.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            self.map.clear();
            self.since = Some(Instant::now());
        }
    }
}

/// The value of an item in a column. An error of a Python function is printed and gives no value
pub fn value_of(e: &Entry, column: Column, hooks: &Hooks<'_>, values: &mut Values) -> Value {
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
            let v = function(&e.path);
            values.map.insert(key, v.clone());
            v
        }
    }
}

/// What the user chose: which items stay, and in which order
pub struct Rules {
    pub filters: Vec<(Column, DataType, Filter)>,
    pub sort: Column,
    pub ascending: bool,
    pub dirs_first: bool,
}

impl Rules {
    pub fn new(state: &ColumnsState, hooks: &Hooks<'_>, dirs_first: bool) -> Self {
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
pub fn arrange<T>(
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
pub fn distinct_values<'a>(
    entries: impl Iterator<Item = &'a Entry>,
    key: &str,
    hooks: &Hooks<'_>,
    values: &mut Values,
) -> Vec<String> {
    let Some(column) = Column::of_key(key, hooks) else { return vec![] };
    // a value that some folder has comes first (the folders of the Type column), then the others
    let mut found: HashMap<String, bool> = HashMap::new();
    for e in entries {
        if let Value::Text(t) = value_of(e, column, hooks, values) {
            *found.entry(t).or_insert(false) |= e.is_dir;
        }
    }
    let mut found: Vec<(String, bool)> = found.into_iter().collect();
    found.sort_by(|(a, da), (b, db)| db.cmp(da).then_with(|| natural_cmp(a, b)));
    found.into_iter().map(|(v, _)| v).collect()
}

/// The state of a list or icon view
pub struct FolderView {
    pub id: String,
    pub core: Arc<Mutex<Core>>,
    pub history: Vec<PathBuf>,
    pub position: usize,
    pub show_hidden: bool,
    pub multi_select: bool,
    pub navigate: bool,
    pub dirs_first: bool,
    /// width, order, sort and filters of the columns
    pub columns: ColumnsState,
    /// values of the columns of Python
    pub values: Values,
    /// the items of the folder at the last frame, before the filters (for the choices of a filter)
    pub listing: Arc<Vec<Entry>>,
    pub selected: Vec<PathBuf>,
    pub anchor: Option<PathBuf>,
    pub cursor: Option<PathBuf>,
    pub activated: Vec<PathBuf>,
    /// the item that got a click of the middle button during the last frame (a new tab, in the apps)
    pub middle_clicked: Option<PathBuf>,
    /// the folder under the pointer while files are dragged over the view or dropped on it
    pub drop_target: Option<PathBuf>,
    /// the items shown at the last frame, in their order
    pub shown: Vec<PathBuf>,
    pub scroll_to: Option<PathBuf>,
    pub focus: Option<bool>,
    pub typed: String,
    pub typed_at: Option<Instant>,
    /// where the rubber band started, from the top left of the content, and the selection kept
    pub band: Option<(Pos2, Vec<PathBuf>)>,
    pub folder_changed: bool,
    /// the size of the icons of the icon view
    pub icon_size: f32,
    /// the items shown instead of the folder, e.g. the result of a search
    pub paths: Option<Vec<PathBuf>>,
    /// takes the keyboard focus back at every frame, unless a text field is being edited
    pub keep_focus: bool,
}

impl FolderView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        folder: PathBuf,
        core: Arc<Mutex<Core>>,
        show_hidden: bool,
        multi_select: bool,
        navigate: bool,
        dirs_first: bool,
        sort_by: &str,
    ) -> Self {
        FolderView {
            id,
            core,
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
            middle_clicked: None,
            drop_target: None,
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
        }
    }

    pub fn can_go_back(&self) -> bool {
        self.position > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.position + 1 < self.history.len()
    }

    pub fn folder(&self) -> &Path {
        &self.history[self.position]
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
        self.cursor = None;
    }

    pub fn go(&mut self, folder: &Path) {
        let folder = expand(folder);
        if folder == self.folder() {
            return;
        }
        self.history.truncate(self.position + 1);
        self.history.push(folder);
        self.position = self.history.len() - 1;
        self.entered();
    }

    pub fn entered(&mut self) {
        self.paths = None;
        self.clear_selection();
        self.folder_changed = true;
    }

    pub fn up(&mut self) {
        let child = self.folder().to_path_buf();
        if let Some(parent) = child.parent() {
            self.go(parent);
            self.select_only(child.clone());
            self.scroll_to = Some(child);
        }
    }

    pub fn back(&mut self) {
        if self.position > 0 {
            self.position -= 1;
            self.entered();
        }
    }

    pub fn forward(&mut self) {
        if self.position + 1 < self.history.len() {
            self.position += 1;
            self.entered();
        }
    }

    pub fn select_only(&mut self, path: PathBuf) {
        self.selected = vec![path.clone()];
        self.anchor = Some(path.clone());
        self.cursor = Some(path);
    }

    pub fn select_range(&mut self, to: usize) {
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
    pub fn click(&mut self, path: &Path, modifiers: egui::Modifiers) {
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
    pub fn activate(&mut self, paths: Vec<PathBuf>) {
        if self.navigate && paths.len() == 1 && paths[0].is_dir() {
            self.go(&paths[0].clone());
        } else {
            self.activated = paths;
        }
    }

    /// The items of the folder as the view shows them: filtered and sorted
    pub fn entries(&mut self, ctx: &egui::Context, hooks: &Hooks<'_>) -> Vec<Entry> {
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
    pub fn keyboard(&mut self, keys: &Keys, step_x: usize, step_y: usize, page: usize) {
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
        // nothing selected: Down gives the first item and Up the last one
        let current = if self.selected.is_empty() {
            None
        } else {
            self.cursor.as_ref().and_then(|c| self.shown.iter().position(|p| p == c))
        };
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
pub enum Shape {
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
pub struct Vec2Size {
    pub w: f32,
    pub h: f32,
}

impl Shape {
    /// The rectangle of the item i, from the top left of the content
    pub fn item_rect(&self, i: usize, width: f32) -> Rect {
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

    pub fn content_height(&self, n: usize) -> f32 {
        match self {
            Shape::List { row_height, .. } => n as f32 * row_height,
            Shape::Icons { tile, per_row, .. } => n.div_ceil(*per_row) as f32 * tile.h,
        }
    }

    /// The items that can be seen in this part of the content
    pub fn visible(&self, viewport: Rect, n: usize) -> std::ops::Range<usize> {
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
pub fn default_width(column: Column, hooks: &Hooks<'_>) -> f32 {
    match column {
        Column::Sort(SortBy::Name) => 0.0,
        Column::Sort(SortBy::Modified) => 180.0, // "vendredi dernier à 09:10"
        Column::Sort(SortBy::Size) => 80.0,
        Column::Sort(SortBy::Kind) => 130.0,
        Column::Extra(i) => hooks.extras[i].width,
    }
}

/// Tells the state of the columns every column the view can show (for the settings of the header)
pub fn set_catalog(state: &mut ColumnsState, wanted: &[Column], headers: &[String], hooks: &Hooks<'_>) {
    state.catalog = wanted
        .iter()
        .map(|c| {
            let title = match c {
                Column::Sort(sort) => headers[*sort as usize].clone(),
                Column::Extra(k) => hooks.extras[*k].title.clone(),
            };
            (c.key(hooks), title)
        })
        .collect();
}

/// The columns of a list, in the order the user chose. The name takes what the others leave (at
/// least 160), unless the user gave it a width
pub fn resolve_columns(state: &ColumnsState, wanted: &[Column], hooks: &Hooks<'_>, width: f32) -> Vec<(Column, f32)> {
    let default: Vec<String> = wanted.iter().map(|c| c.key(hooks)).collect();
    let columns: Vec<Column> = state
        .ordered(&default)
        .iter()
        .filter(|key| !state.hidden.contains(key))
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
pub fn list_header(ui: &mut egui::Ui, view: &mut FolderView, shape: &Shape, hooks: &Hooks<'_>, id: Id, opts: HeaderOptions) {
    let Shape::List { columns, headers, row_height, .. } = shape else { return };
    let _ = row_height;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), columns::HEADER_H), Sense::hover());
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
                icon: match column {
                    Column::Extra(k) => hooks.extras[*k].icon.clone(),
                    Column::Sort(_) => None,
                },
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
pub fn list_zones(state: &ColumnsState, columns: &[(Column, f32)], hooks: &Hooks<'_>, width: f32) -> Zones {
    let keys: Vec<String> = columns.iter().map(|(c, _)| c.key(hooks)).collect();
    let widths: Vec<f32> = columns.iter().map(|(_, w)| *w).collect();
    Zones::new(&keys, &widths, state, width, None)
}

#[allow(clippy::too_many_arguments)]
pub fn paint_list_row(
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
        // the separator at the left of the column, inside its zone
        let zone = zones.zone_rect(rect, placed.zone);
        if cell.left() > zone.left() + 0.5 && cell.left() < zone.right() {
            let line = Stroke::new(1.0, row_ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6));
            row_ui.painter().with_clip_rect(zone).vline(cell.left(), rect.y_range(), line);
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
                cell_ui.shrink_clip_rect(visible);
                (hooks.extras[*k].cell)(&mut cell_ui, &e.path);
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
pub fn paint_tile(ui: &egui::Ui, e: &Entry, rect: Rect, icon: f32, badges: &[Color32], selected: bool, hovered: bool, focused: bool) {
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
pub fn draw_folder(ui: &mut egui::Ui, view: &mut FolderView, shape: Shape, hooks: &Hooks<'_>, has_menu: bool, opts: HeaderOptions) -> Menus {
    let viewport = ui.available_width();
    draw_folder_in(ui, view, shape, hooks, has_menu, opts, viewport)
}

pub fn draw_folder_in(
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
    view.middle_clicked = None;
    view.drop_target = None;
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
    if let Shape::List { columns, headers, .. } = &mut shape {
        set_catalog(&mut view.columns, &wanted, headers, hooks);
        *columns = resolve_columns(&view.columns, &wanted, hooks, width);
    }

    match view.focus.take() {
        Some(true) => ui.memory_mut(|m| m.request_focus(id)),
        Some(false) => ui.memory_mut(|m| m.surrender_focus(id)),
        None if view.keep_focus && !ui.ctx().text_edit_focused() && !crate::terminal::terminal_focused(ui.ctx()) => ui.memory_mut(|m| m.request_focus(id)),
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
    // the offset is kept by view and folder, not by the place of the ui: the same view in a pane of a
    // dock or alone shows the same position
    let scroll_key = egui::Id::unique(("pyegui-scroll-offset", &view.id)).with(view.folder());
    let mut area = egui::ScrollArea::vertical().id_salt(id.with(view.folder())).auto_shrink([false, false]);
    // imposed only when the view is drawn in another parent than at the last frame (the dock or
    // not): otherwise egui keeps the offset, and the wheel and the bar move it
    let parent = ui.scope_id();
    if let Some((was_in, offset)) = ctx.data(|d| d.get_temp::<(Id, f32)>(scroll_key)) {
        if was_in != parent {
            area = area.vertical_scroll_offset(offset);
        }
    }
    let mut seen_offset = 0.0;
    let output = area
        .show_viewport(ui, |ui, viewport| {
            seen_offset = viewport.min.y;
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
            // files from the Finder over the view: the system gives the pointer, the window does not
            let files_over = ctx.input(|i| !i.raw.hovered_files.is_empty() || !i.raw.dropped_files.is_empty());
            let pointer = if files_over { native_pointer() } else { None };
            if files_over {
                ctx.request_repaint(); // the window gets no pointer event while the files are dragged
            }
            for i in shape.visible(viewport, n) {
                let e = &entries[i];
                let rect = shape.item_rect(i, width).translate(origin.to_vec2());
                let sense = if opts.drag_out { Sense::click_and_drag() } else { Sense::click() };
                let response = ui.interact(rect.shrink(1.0), id.with(("item", &e.path)), sense);
                if response.drag_started() {
                    // the item dragged is part of what goes out, with the rest of the selection
                    if !view.selected.contains(&e.path) {
                        view.select_only(e.path.clone());
                    }
                    ui.memory_mut(|m| m.request_focus(id));
                    let paths: Vec<PathBuf> = view.selected.iter().cloned().collect();
                    if start_drag(&paths) {
                        ui.ctx().stop_dragging(); // the mouse is the drag session's now
                    }
                }
                if response.middle_clicked() {
                    view.middle_clicked = Some(e.path.clone());
                }
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
                let drop_here = e.is_dir
                    && files_over
                    && pointer.is_some_and(|p| rect.contains(p) && ui.clip_rect().contains(p));
                if drop_here {
                    view.drop_target = Some(e.path.clone());
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
                if drop_here {
                    // after the row: its background would hide the frame
                    ui.painter().rect_filled(rect, 4.0, ui.visuals().selection.bg_fill.gamma_multiply(0.25));
                    ui.painter().rect_stroke(rect.shrink(1.0), 4.0, Stroke::new(2.0, ui.visuals().selection.bg_fill), egui::StrokeKind::Inside);
                }
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
    ctx.data_mut(|d| d.insert_temp(scroll_key, (parent, seen_offset)));
    ctx.data_mut(|d| d.insert_temp(Id::unique(columns::HEADER_SCROLLED), seen_offset > 0.5));
    hscroll_bar(ui, id, &mut view.columns, &zones, output.inner_rect);
    menus
}

/// Selects the items a rectangle drawn on the background touches. Cmd or Shift keeps the selection
#[allow(clippy::too_many_arguments)]
pub fn rubber_band(
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


/// The smallest and the largest icons of an icon view
pub const ICON_SIZES: (f32, f32) = (16.0, 256.0);

pub struct TreeState {
    pub id: String,
    pub core: Arc<Mutex<Core>>,
    pub root: PathBuf,
    pub show_hidden: bool,
    pub multi_select: bool,
    pub show_root: bool,
    pub dirs_first: bool,
    /// width, order, sort and filters of the columns
    pub columns: ColumnsState,
    /// values of the columns of Python
    pub values: Values,
    /// the items shown at the last frame, for the choices of a filter (only kept when a column has some)
    pub listing: Arc<Vec<Entry>>,
    /// taken out while the tree is drawn, so the context menu can read the view
    pub state: TreeViewState<String>,
    /// what Python reads, also while the tree is drawn
    pub selected: Vec<String>,
    pub open: HashSet<String>,
    /// what Python asked for, given to the state at the next frame
    pub pending_selected: Option<Vec<String>>,
    pub pending_open: Vec<(String, bool)>,
    /// the rows shown at the last frame, in their order (the items of the open folders included)
    pub shown: Vec<String>,
    pub known_dirs: HashSet<String>,
    pub activated: Vec<String>,
    /// the row that got a click of the middle button during the last frame
    pub middle_clicked: Option<String>,
    /// the folder under the pointer while files are dragged over the tree or dropped on it
    pub drop_target: Option<String>,
    pub focus: Option<bool>,
    pub keep_focus: bool,
    pub scroll_to: Option<String>,
    /// what was typed to find a row by its name, and when
    pub typed: String,
    pub typed_at: Option<Instant>,
    pub activatable_dirs: bool,
    /// the items at the top instead of the items of the root
    pub paths: Option<Vec<PathBuf>>,
    /// the state of the tree when it is drawn by the generic grid (`TreeParams::grid`)
    pub grid: crate::grid::GridState,
}


pub struct Node {
    pub entry: Entry,
    pub id: String,
    pub badges: Vec<Color32>,
    /// the items of an open folder
    pub children: Option<Vec<Node>>,
}

pub struct TreeOptions {
    pub show_hidden: bool,
}

/// The items of a folder as nodes, in the order of the disk: arrange puts them in order once the
/// listing is free again
pub fn tree_children(
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
pub fn arrange_nodes(nodes: Vec<Node>, rules: &Rules, hooks: &Hooks<'_>, values: &mut Values, sort_top: bool) -> Vec<Node> {
    let mut nodes = arrange(nodes, |n| &n.entry, rules, hooks, values, false, sort_top);
    for node in &mut nodes {
        if let Some(children) = node.children.take() {
            node.children = Some(arrange_nodes(children, rules, hooks, values, true));
        }
    }
    nodes
}

pub fn collect_entries(nodes: &[Node], into: &mut Vec<Entry>) {
    for node in nodes {
        into.push(node.entry.clone());
        if let Some(children) = &node.children {
            collect_entries(children, into);
        }
    }
}

/// The columns at the right end of the rows of the tree, in the order of the user: the columns of
/// Python, then the ones of the view that were asked for
pub fn tree_columns(state: &ColumnsState, hooks: &Hooks<'_>, builtin: &[SortBy]) -> Vec<(Column, f32)> {
    let wanted: Vec<Column> =
        (0..hooks.extras.len()).map(Column::Extra).chain(builtin.iter().map(|s| Column::Sort(*s))).collect();
    let default: Vec<String> = wanted.iter().map(|c| c.key(hooks)).collect();
    state
        .ordered(&default)
        .iter()
        .filter(|key| !state.hidden.contains(key))
        .filter_map(|key| wanted.iter().copied().find(|c| c.key(hooks) == *key))
        .map(|c| (c, state.widths.get(&c.key(hooks)).copied().unwrap_or_else(|| default_width(c, hooks))))
        .collect()
}

/// The titles of the columns of the view: name, modified, size and kind

pub fn tree_node(
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
pub fn fill_badges(nodes: &mut [Node], hooks: &Hooks<'_>) {
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
pub struct TreeLayout {
    pub width: f32,
    pub name: f32,
    pub zones: Zones,
    pub scroll: f32,
    /// the width of the tree is known: the header can be drawn
    pub finite: bool,
    /// the left edge of the tree, where the header starts: the cells of the rows are placed from it
    pub origin_x: Option<f32>,
}

impl TreeLayout {
    /// The width of the name: the one the user chose, else what the columns leave. Without a width
    /// (a tree in a ui as wide as it wants) the name has none and the columns end the rows
    pub fn new(width: f32, columns: &[(Column, f32)], hooks: &Hooks<'_>, state: &ColumnsState) -> Self {
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
        TreeLayout { width, name, zones, scroll, finite, origin_x: None }
    }
}

/// The right part of a row of the tree: the columns, from the right of the name, and the badges
/// at the end of the name. The cells are placed from the row, the same way as the titles of the
/// header, so they stay under them whatever the room
pub fn with_tree_accessory<'a>(
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
        // from the left edge of the tree, like the header: the right of a row is not the right of the tree
        let left = layout.origin_x.unwrap_or(area.right() - layout.width);
        let row = Rect::from_min_max(pos2(left, area.top()), pos2(left + layout.width, area.bottom()));
        // the left edge of the first column
        let first = row.left() + layout.name;
        // the room of the columns (and of the badges) is taken from the label, which truncates its
        // text to the name column instead of running under the cells
        if layout.finite {
            let dots = if badges.is_empty() { 0.0 } else { dots_width(&badges, 9.0) + 8.0 };
            ui.allocate_space(vec2((area.right() - first).max(0.0) + dots, 1.0));
        }
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
                    (extra.cell)(&mut cell_ui, &entry.path);
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
pub fn tree_header(
    ui: &mut egui::Ui,
    id: Id,
    view: &Shared<TreeState>,
    columns: &[(Column, f32)],
    headers: &[String; 4],
    hooks: &Hooks<'_>,
    opts: HeaderOptions,
    slot: Rect,
    layout: &TreeLayout,
) {
    let shift = (ui.clip_rect().top() - slot.top()).max(0.0);
    let rect = slot.translate(vec2(0.0, shift));
    if shift > 0.5 && !opts.tinted {
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
        icon: None,
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
            icon: match column {
                Column::Extra(k) => hooks.extras[*k].icon.clone(),
                Column::Sort(_) => None,
            },
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

pub fn with_tree_icon(b: NodeBuilder<'_, String>, uri: Option<String>, side: f32, hidden: bool) -> NodeBuilder<'_, String> {
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

pub fn add_tree_nodes<'a>(
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
        let uri = crate::icons::entry_icon_uri(ctx, &node.entry.path, node.entry.is_dir, side);
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

/// The node shown at a row, counting the rows the same way as tree_row
pub fn tree_node_at<'a>(nodes: &'a [Node], index: usize, row: &mut usize) -> Option<&'a Node> {
    for node in nodes {
        if *row == index {
            return Some(node);
        }
        *row += 1;
        if let Some(children) = &node.children {
            if let Some(found) = tree_node_at(children, index, row) {
                return Some(found);
            }
        }
    }
    None
}

/// The ids of the rows shown, in order
pub fn tree_rows(nodes: &[Node], rows: &mut Vec<String>) {
    for node in nodes {
        rows.push(node.id.clone());
        if let Some(children) = &node.children {
            tree_rows(children, rows);
        }
    }
}

/// The row of a node among the rows shown
pub fn tree_row(nodes: &[Node], id: &str, row: &mut usize) -> Option<usize> {
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


/// What a tree is drawn with, besides its state and its hooks
pub struct TreeParams {
    pub opts: HeaderOptions,
    /// the titles of the name, modified, size and kind columns
    pub headers: [String; 4],
    pub date_format: String,
    /// the columns of the view to show, after the ones of the application
    pub builtin: Vec<SortBy>,
    /// the pinned header over the rows
    pub header: bool,
    /// draw the tree with the generic data grid (`grid::show`) instead of `egui_ltreeview`
    pub grid: bool,
}

impl Default for TreeParams {
    fn default() -> Self {
        TreeParams {
            opts: HeaderOptions { resizable: true, pinnable: true, reorderable: true, filterable: true, drag_out: true, grow_left: false, tinted: false, settings: true },
            headers: default_headers(),
            date_format: DATE_FORMAT.into(),
            builtin: vec![],
            header: true,
            grid: false,
        }
    }
}

pub const DATE_FORMAT: &str = HUMAN_DATES;

pub fn default_headers() -> [String; 4] {
    ["Name".into(), "Date Modified".into(), "Size".into(), "Kind".into()]
}

/// Draws a tree. `menu` is called inside the context menu with the ids (paths) concerned
pub fn draw_tree(
    ui: &mut egui::Ui,
    view: &Shared<TreeState>,
    hooks: &Hooks<'_>,
    params: &TreeParams,
    menu: Option<&dyn Fn(&mut egui::Ui, &Vec<String>)>,
) {
    if params.grid {
        return draw_tree_grid(ui, view, hooks, params, menu);
    }
    let TreeParams { opts, headers, date_format, builtin, header, .. } = params;
    let opts = *opts;
    let ctx = ui.ctx().clone();

    let id = ui.make_persistent_id(&view.borrow().id);
    {
        let wanted: Vec<Column> =
            (0..hooks.extras.len()).map(Column::Extra).chain(builtin.iter().map(|s| Column::Sort(*s))).collect();
        set_catalog(&mut view.borrow_mut().columns, &wanted, headers, hooks);
    }
    let columns = tree_columns(&view.borrow().columns, hooks, builtin);
    let mut values = std::mem::take(&mut view.borrow_mut().values);
    values.refresh();
    let available = ui.available_width();
    let mut layout = Some(TreeLayout::new(available, &columns, hooks, &view.borrow().columns));
    let origin_x = ui.cursor().min.x;
    if let Some(l) = layout.as_mut() {
        l.origin_x = Some(origin_x);
    }
    // the room of the header, which is drawn once the rows are, over them
    let slot = match (&layout, *header) {
        (Some(layout), true) if layout.finite && (!columns.is_empty() || !view.borrow().columns.catalog.is_empty()) => Some(ui.allocate_exact_size(vec2(layout.width, columns::HEADER_H), Sense::hover()).0),
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

    let (mut state, mut nodes, multi_select, show_root, activatable_dirs, mut scroll_to, rules, given) = {
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
        let rules = Rules::new(&v.columns, hooks, v.dirs_first);
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
            None if v.keep_focus && !ctx.text_edit_focused() && !crate::terminal::terminal_focused(&ctx) => ui.memory_mut(|m| m.request_focus(id)),
            None => {}
        }
        (state, nodes, v.multi_select, v.show_root, v.activatable_dirs, v.scroll_to.take(), rules, given)
    };
    // the folders are read and free: the functions of Python of the columns can run. The choices of a
    // filter are the ones of what is loaded, before the filters (none kept still leaves them all to pick)
    let all_entries = if columns.iter().any(|(c, _)| c.data_type(hooks) == DataType::Choice) {
        let mut all = vec![];
        collect_entries(&nodes, &mut all);
        all
    } else {
        vec![]
    };
    nodes = arrange_nodes(nodes, &rules, hooks, &mut values, !given);
    fill_badges(&mut nodes, hooks);
    {
        let mut v = view.borrow_mut();
        v.listing = Arc::new(all_entries);
        v.values = std::mem::take(&mut values);
    }

    // typing the start of a name selects the first row that begins with it
    if ui.memory(|m| m.has_focus(id)) {
        let keys = read_keys(ui);
        if !keys.text.trim().is_empty() {
            let mut v = view.borrow_mut();
            if v.typed_at.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
                v.typed.clear();
            }
            let typed = format!("{}{}", v.typed, keys.text.to_lowercase());
            v.typed = typed.clone();
            v.typed_at = Some(Instant::now());
            let found = v
                .shown
                .iter()
                .find(|p| Path::new(p).file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().starts_with(&typed)))
                .cloned();
            if let Some(path) = found {
                state.set_selected(vec![path.clone()]);
                scroll_to = Some(path);
            }
        }
    }

    // nothing selected: Down gives the first row and Up the last one
    if ui.memory(|m| m.has_focus(id)) && state.selected().is_empty() {
        let (down, up) = ui.input_mut(|i| {
            (i.consume_key(egui::Modifiers::NONE, Key::ArrowDown), i.consume_key(egui::Modifiers::NONE, Key::ArrowUp))
        });
        let picked = {
            let v = view.borrow();
            if down {
                v.shown.first().cloned()
            } else if up {
                v.shown.last().cloned()
            } else {
                None
            }
        };
        if let Some(path) = picked {
            state.set_selected(vec![path.clone()]);
            scroll_to = Some(path);
        }
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
    let mut tree = egui_ltreeview::TreeView::new(id)
        .allow_multi_selection(multi_select)
        .allow_drag_and_drop(false); // the tree would draw its own drop markers and panic once the pointer is gone
    if let Some(menu_fun) = menu {
        tree = tree.fallback_context_menu(|menu_ui, ids: &Vec<String>| menu_fun(menu_ui, ids));
    }
    let tree_top = ui.cursor().min;
    let selected_before = state.selected().clone(); // the press of a drag selects the row pressed alone
    // the rows go under the pinned header: they are not drawn (nor hovered) where the header is
    let full_clip = ui.clip_rect();
    if let Some(slot) = &slot {
        let shift = (full_clip.top() - slot.top()).max(0.0);
        ctx.data_mut(|d| d.insert_temp(Id::unique(columns::HEADER_SCROLLED), shift > 0.5));
        // not `clamp`: a clip rect of a view squeezed to nothing has its top under its bottom, and clamp panics
        let below = (slot.bottom() + shift).max(full_clip.top()).min(full_clip.bottom().max(full_clip.top()));
        ui.shrink_clip_rect(Rect::from_min_max(pos2(full_clip.left(), below), full_clip.max));
    }
    // a tree of a known width: the rows keep to it, so the names are cut before the columns
    if let Some(l) = layout.as_ref().filter(|l| l.finite) {
        tree = tree.min_width(l.width).max_width(l.width);
    }
    let rows_top = slot.as_ref().map(|slot| (slot.bottom() + (full_clip.top() - slot.top()).max(0.0)).max(full_clip.top()).min(full_clip.bottom().max(full_clip.top()))).unwrap_or(full_clip.top());
    let (tree_response, actions) = tree.show_state(ui, &mut state, |builder| {
        add_tree_nodes(builder, &nodes, &ctx, show_root, activatable_dirs, hooks, &columns, layout.as_ref(), date_format)
    });
    ui.set_clip_rect(full_clip);
    // the separators of the columns, one line over all the rows (not one piece per row)
    if let Some(l) = layout.as_ref().filter(|l| l.finite) {
        let line = Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6));
        let clip = Rect::from_min_max(pos2(full_clip.left(), rows_top), full_clip.max);
        let painter = ui.painter().with_clip_rect(clip);
        let row = Rect::from_min_size(pos2(origin_x, tree_top.y), vec2(l.width, tree_response.rect.bottom() - tree_top.y));
        for placed in l.zones.placed.iter().filter(|p| p.index > 0) {
            let zone = l.zones.zone_rect(row, placed.zone);
            let x = l.zones.cell_rect(row, placed, l.scroll).left();
            if x > zone.left() + 0.5 && x < zone.right() {
                painter.vline(x, row.y_range(), line);
            }
        }
    }

    // files from the Finder or from the window over the tree: the folder under the pointer takes them
    let mut drop_target = None;
    if ctx.input(|i| !i.raw.hovered_files.is_empty() || !i.raw.dropped_files.is_empty()) {
        ctx.request_repaint(); // the window gets no pointer event while the files are dragged
        if let Some(p) = native_pointer().filter(|p| ui.clip_rect().contains(*p) && tree_response.rect.x_range().contains(p.x)) {
            let step = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
            if p.y >= tree_top.y {
                let index = ((p.y - tree_top.y) / step) as usize;
                if let Some(node) = tree_node_at(&nodes, index, &mut 0).filter(|n| n.entry.is_dir) {
                    let top = tree_top.y + index as f32 * step;
                    let row = Rect::from_min_max(pos2(tree_response.rect.left(), top), pos2(tree_response.rect.right(), top + step - ui.spacing().item_spacing.y));
                    ui.painter().rect_filled(row, 4.0, ui.visuals().selection.bg_fill.gamma_multiply(0.25));
                    ui.painter().rect_stroke(row.shrink(1.0), 4.0, Stroke::new(2.0, ui.visuals().selection.bg_fill), egui::StrokeKind::Inside);
                    drop_target = Some(node.id.clone());
                }
            }
        }
    }
    {
        let mut v = view.borrow_mut();
        v.drop_target = drop_target;
        v.shown.clear();
        tree_rows(&nodes, &mut v.shown);
    }

    if let (Some(slot), Some(layout)) = (slot, &layout) {
        tree_header(ui, id, view, &columns, headers, hooks, opts, slot, layout);
    }
    if let Some(layout) = layout.as_ref().filter(|l| l.finite) {
        let clip = ui.clip_rect();
        let seen = Rect::from_min_max(pos2(origin_x, clip.top()), pos2(origin_x + layout.width, clip.bottom()));
        hscroll_bar(ui, id, &mut view.borrow_mut().columns, &layout.zones, seen);
    }

    let mut v = view.borrow_mut();
    v.middle_clicked = None;
    if ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Middle)) {
        if let Some(p) = ctx.input(|i| i.pointer.interact_pos()).filter(|p| tree_response.rect.contains(*p) && ui.clip_rect().contains(*p)) {
            let step = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
            if p.y >= tree_top.y {
                v.middle_clicked = tree_node_at(&nodes, ((p.y - tree_top.y) / step) as usize, &mut 0).map(|n| n.id.clone());
            }
        }
    }
    v.activated.clear();
    for action in actions {
        match action {
            Action::Activate(activate) => v.activated = activate.selected,
            _ => {}
        }
    }
    // a change asked for by the menu wins over the state of this frame
    if let Some(selected) = v.pending_selected.clone() {
        state.set_selected(selected);
    }
    let mut dragged: Vec<String> = vec![];
    if opts.drag_out && tree_response.drag_started() {
        let now = state.selected().clone();
        // a drag from a row of the selection takes the whole selection: the tree, which takes the
        // start of the drag for a click, selected the row pressed alone
        if now.len() == 1 && selected_before.len() > 1 && selected_before.contains(&now[0]) {
            state.set_selected(selected_before.clone());
            dragged = selected_before;
        } else {
            dragged = now;
        }
    }
    v.selected = state.selected().clone();
    if !dragged.is_empty() {
        let paths: Vec<PathBuf> = dragged.iter().map(PathBuf::from).collect();
        if start_drag(&paths) {
            ctx.stop_dragging();
        }
    }
    let open: HashSet<String> = v
        .known_dirs
        .iter()
        .filter(|d| state.is_open(d).unwrap_or(false))
        .cloned()
        .collect();
    v.open = open;
    v.state = state;
}

impl TreeState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        root: PathBuf,
        core: Arc<Mutex<Core>>,
        show_hidden: bool,
        multi_select: bool,
        show_root: bool,
        dirs_first: bool,
        activatable_dirs: bool,
    ) -> Self {
        TreeState {
            id,
            core,
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
            shown: vec![],
            known_dirs: HashSet::new(),
            activated: vec![],
            middle_clicked: None,
            drop_target: None,
            focus: None,
            keep_focus: false,
            scroll_to: None,
            typed: String::new(),
            typed_at: None,
            activatable_dirs,
            paths: None,
            grid: crate::grid::GridState::default(),
        }
    }

    /// Selects these paths (at the next frame)
    pub fn select_paths(&mut self, selected: &[PathBuf]) {
        self.selected = selected.iter().map(|p| to_str(&expand(p))).collect();
        self.pending_selected = Some(self.selected.clone());
    }

    /// Opens or closes a folder (at the next frame)
    pub fn set_open(&mut self, path: &Path, open: bool) {
        let path = to_str(&expand(path));
        if open {
            self.open.insert(path.clone());
        } else {
            self.open.remove(&path);
        }
        self.pending_open.push((path, open));
    }

    pub fn is_open(&self, path: &Path) -> bool {
        self.open.contains(&to_str(&expand(path)))
    }

    /// Opens the folders above a path, selects it and scrolls to it
    pub fn reveal(&mut self, path: &Path) {
        let path = expand(path);
        let mut folder = path.parent();
        while let Some(f) = folder {
            if !f.starts_with(&self.root) || (f == self.root && !self.show_root) {
                break;
            }
            self.set_open(f, true);
            folder = f.parent();
        }
        self.select_paths(std::slice::from_ref(&path));
        self.scroll_to = Some(to_str(&path));
    }
}

/// The columns of a list, in the order of the view: the name, the columns of the application, then
/// the built-in ones named
pub fn list_columns(names: &[String], extras: usize) -> Vec<(Column, f32)> {
    let mut columns = vec![(Column::Sort(SortBy::Name), 0.0)];
    columns.extend((0..extras).map(|i| (Column::Extra(i), 0.0)));
    for name in names {
        if let Some(column) = SortBy::parse(name).filter(|c| *c != SortBy::Name) {
            columns.push((Column::Sort(column), 0.0));
        }
    }
    columns
}


// -- the tree on the generic grid ----------------------------------------------------------------------------

/// The nodes of a folder, from the listing, the folders open (`open`) read too
fn grid_children(core: &mut Core, ctx: &egui::Context, dir: &Path, show_hidden: bool, open: &HashSet<String>, known: &mut HashSet<String>) -> Vec<Node> {
    let all = core.list(Some(ctx), dir);
    let entries: Vec<Entry> = all.iter().filter(|e| show_hidden || !e.hidden).cloned().collect();
    entries.into_iter().map(|e| grid_node(core, ctx, e, show_hidden, open, known)).collect()
}

fn grid_node(core: &mut Core, ctx: &egui::Context, entry: Entry, show_hidden: bool, open: &HashSet<String>, known: &mut HashSet<String>) -> Node {
    let id = to_str(&entry.path);
    let children = if entry.is_dir {
        known.insert(id.clone());
        open.contains(&id).then(|| grid_children(core, ctx, &entry.path, show_hidden, open, known))
    } else {
        None
    };
    Node { entry, id, badges: vec![], children }
}

/// The nodes as flat rows of the grid, the children after their parent
fn grid_rows(nodes: &[Node], parent: Option<&str>, ctx: &egui::Context, columns: &[(Column, f32)], date_format: &str, out: &mut Vec<crate::grid::GridRow>) {
    for node in nodes {
        let e = &node.entry;
        let uri = crate::icons::entry_icon_uri(ctx, &e.path, e.is_dir, 16.0);
        let name = if uri.is_some() { e.name.clone() } else { format!("{}  {}", emoji(e), e.name) };
        let mut cells = vec![name];
        for (column, _) in columns {
            cells.push(match column {
                Column::Extra(_) => String::new(),
                Column::Sort(SortBy::Modified) => date(e.modified, date_format),
                Column::Sort(SortBy::Size) if e.is_dir => "--".into(),
                Column::Sort(SortBy::Size) => human_size(e.size),
                Column::Sort(_) => kind(e),
            });
        }
        out.push(crate::grid::GridRow {
            id: node.id.clone(),
            cells,
            parent: parent.map(String::from),
            readonly: true,
            icon: uri,
            expandable: e.is_dir,
            badges: node.badges.clone(),
            dim: e.hidden,
        });
        if let Some(children) = &node.children {
            grid_rows(children, Some(&node.id), ctx, columns, date_format, out);
        }
    }
}

/// `draw_tree` on the generic grid: the same state, the same hooks, the same output (`selected`, `open`, `shown`,
/// `activated`, `middle_clicked`, `drop_target`), with the columns, the keyboard and the lasso of the grid
pub fn draw_tree_grid(
    ui: &mut egui::Ui,
    view: &Shared<TreeState>,
    hooks: &Hooks<'_>,
    params: &TreeParams,
    menu: Option<&dyn Fn(&mut egui::Ui, &Vec<String>)>,
) {
    use crate::grid::{show, GridColumn, GridOptions};
    let TreeParams { opts, headers, date_format, builtin, .. } = params;
    let ctx = ui.ctx().clone();
    let available = ui.available_width();
    if !available.is_finite() {
        return;
    }
    let grid_id = view.borrow().id.clone();
    let focus_id = ui.make_persistent_id(("data_grid", grid_id.as_str())).with("focus");
    {
        let wanted: Vec<Column> = (0..hooks.extras.len()).map(Column::Extra).chain(builtin.iter().map(|s| Column::Sort(*s))).collect();
        set_catalog(&mut view.borrow_mut().columns, &wanted, headers, hooks);
    }
    // the columns after the name, all of them (the grid hides the ones the user hid), in the order of Python then of the view
    let all: Vec<(Column, f32)> = (0..hooks.extras.len())
        .map(Column::Extra)
        .chain(builtin.iter().map(|s| Column::Sort(*s)))
        .map(|c| (c, default_width(c, hooks)))
        .collect();

    // the state of the view and of the grid, taken out while the tree is drawn
    let mut values = std::mem::take(&mut view.borrow_mut().values);
    values.refresh();
    let (mut gstate, nodes_and_rules) = {
        let mut v = view.borrow_mut();
        let mut g = std::mem::take(&mut v.grid);
        g.columns = std::mem::take(&mut v.columns);
        for (path, open) in std::mem::take(&mut v.pending_open) {
            if open {
                g.expanded.insert(path);
            } else {
                g.expanded.remove(&path);
            }
        }
        if let Some(selected) = v.pending_selected.take() {
            g.cursor = selected.last().cloned();
            g.anchor = g.cursor.clone();
            g.selected = selected;
        }
        g.selected.retain(|p| Path::new(p).exists());
        match v.focus.take() {
            Some(true) => ctx.memory_mut(|m| m.request_focus(focus_id)),
            Some(false) => ctx.memory_mut(|m| m.surrender_focus(focus_id)),
            None if v.keep_focus && !ctx.text_edit_focused() && !crate::terminal::terminal_focused(&ctx) => ctx.memory_mut(|m| m.request_focus(focus_id)),
            None => {}
        }
        if let Some(target) = v.scroll_to.take() {
            g.scroll_to = Some(target);
        }
        let rules = Rules::new(&g.columns, hooks, v.dirs_first);
        let core = v.core.clone();
        let mut core = lock(&core);
        let mut known = std::mem::take(&mut v.known_dirs);
        let given = v.paths.is_some();
        let show_hidden = v.show_hidden;
        // the root is open the first time it is seen
        if v.show_root && !known.contains(&to_str(&v.root)) {
            g.expanded.insert(to_str(&v.root));
        }
        let nodes = if let Some(paths) = v.paths.clone() {
            let mut top = vec![];
            for path in &paths {
                let Some(parent) = path.parent() else { continue };
                let found = core.list(Some(&ctx), parent).iter().find(|e| same_path(&e.path, path)).cloned();
                if let Some(e) = found.filter(|e| show_hidden || !e.hidden) {
                    top.push(grid_node(&mut core, &ctx, e, show_hidden, &g.expanded, &mut known));
                }
            }
            top
        } else if v.show_root {
            match entry(&v.root) {
                Some(root) => vec![grid_node(&mut core, &ctx, root, show_hidden, &g.expanded, &mut known)],
                None => vec![],
            }
        } else {
            grid_children(&mut core, &ctx, &v.root, show_hidden, &g.expanded, &mut known)
        };
        v.known_dirs = known;
        (g, (nodes, rules, given, v.multi_select, v.activatable_dirs))
    };
    let (mut nodes, rules, given, multi_select, activatable_dirs) = nodes_and_rules;
    let all_entries = if all.iter().any(|(c, _)| c.data_type(hooks) == DataType::Choice) {
        let mut all = vec![];
        collect_entries(&nodes, &mut all);
        all
    } else {
        vec![]
    };
    nodes = arrange_nodes(nodes, &rules, hooks, &mut values, !given);
    fill_badges(&mut nodes, hooks);
    {
        let mut v = view.borrow_mut();
        v.listing = Arc::new(all_entries);
        v.values = std::mem::take(&mut values);
    }

    // typing the start of a name selects the first row that begins with it
    let focused = ctx.memory(|m| m.has_focus(focus_id));
    if focused {
        let keys = read_keys(ui);
        if !keys.text.trim().is_empty() {
            let mut v = view.borrow_mut();
            if v.typed_at.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
                v.typed.clear();
            }
            let typed = format!("{}{}", v.typed, keys.text.to_lowercase());
            v.typed = typed.clone();
            v.typed_at = Some(Instant::now());
            let found = v.shown.iter().find(|p| Path::new(p).file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().starts_with(&typed))).cloned();
            if let Some(path) = found {
                gstate.selected = vec![path.clone()];
                gstate.cursor = Some(path.clone());
                gstate.anchor = Some(path.clone());
                gstate.scroll_to = Some(path);
            }
        }
    }

    let mut rows = vec![];
    grid_rows(&nodes, None, &ctx, &all, date_format, &mut rows);
    let mut dirs: HashSet<String> = HashSet::new();
    fn collect_dirs(nodes: &[Node], into: &mut HashSet<String>) {
        for n in nodes {
            if n.entry.is_dir {
                into.insert(n.id.clone());
            }
            if let Some(c) = &n.children {
                collect_dirs(c, into);
            }
        }
    }
    collect_dirs(&nodes, &mut dirs);

    // the name takes what the other columns leave, unless the user chose its width
    let others: f32 = all.iter().filter(|(c, _)| !gstate.columns.hidden.contains(&c.key(hooks))).map(|(_, w)| *w).sum();
    let mut columns: Vec<GridColumn> = vec![{
        let mut c = GridColumn::new("name", &headers[0]);
        c.width = (available - others).max(120.0);
        c
    }];
    for (column, width) in &all {
        let mut c = GridColumn::new(&column.key(hooks), &match column {
            Column::Sort(sort) => headers[*sort as usize].clone(),
            Column::Extra(k) => hooks.extras[*k].title.clone(),
        });
        c.ty = column.data_type(hooks);
        c.width = *width;
        c.custom = matches!(column, Column::Extra(_));
        c.icon = match column {
            Column::Extra(k) => hooks.extras[*k].icon.clone(),
            Column::Sort(_) => None,
        };
        columns.push(c);
    }
    let options = GridOptions {
        header: *opts,
        multi_select,
        striped: false,
        tree: true,
        sortable: true,
        id: grid_id,
        drag: opts.drag_out,
        tree_column: Some("name".into()),
        keep_order: true,
        ..Default::default()
    };
    let out = {
        let mut cell = |ui: &mut egui::Ui, row: &crate::grid::GridRow, column: &GridColumn, rect: Rect| -> bool {
            let Some(extra) = hooks.extras.iter().find(|e| e.title == column.key) else { return false };
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(4.0, 0.0))).layout(egui::Layout::left_to_right(Align::Center)));
            child.shrink_clip_rect(rect);
            (extra.cell)(&mut child, Path::new(&row.id));
            true
        };
        let mut context = |ui: &mut egui::Ui, ids: &[String]| {
            if let Some(menu_fun) = menu {
                menu_fun(ui, &ids.to_vec());
            }
        };
        show(ui, &mut gstate, &columns, &rows, &options, &mut cell, &mut context)
    };

    // files from the Finder or from the window over the tree: the folder under the pointer takes them
    let mut drop_target = None;
    if ctx.input(|i| !i.raw.hovered_files.is_empty() || !i.raw.dropped_files.is_empty()) {
        ctx.request_repaint();
        if let Some(p) = native_pointer().filter(|p| ui.clip_rect().contains(*p)) {
            if let Some((id, rect)) = out.row_rects.iter().find(|(_, r)| r.contains(p)).filter(|(id, _)| dirs.contains(id)) {
                ui.painter().rect_filled(*rect, 4.0, ui.visuals().selection.bg_fill.gamma_multiply(0.25));
                ui.painter().rect_stroke(rect.shrink(1.0), 4.0, Stroke::new(2.0, ui.visuals().selection.bg_fill), egui::StrokeKind::Inside);
                drop_target = Some(id.clone());
            }
        }
    }
    let mut middle = None;
    if ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Middle)) {
        if let Some(p) = ctx.input(|i| i.pointer.interact_pos()) {
            middle = out.row_rects.iter().find(|(_, r)| r.contains(p) && ui.clip_rect().contains(p)).map(|(id, _)| id.clone());
        }
    }
    // a double click on a folder opens or closes it, unless the folders are activatable
    let mut activated = out.activated.clone();
    if !activatable_dirs && activated.len() == 1 && dirs.contains(&activated[0]) {
        let id = activated.remove(0);
        if !gstate.expanded.remove(&id) {
            gstate.expanded.insert(id);
        }
    }
    if let Some(ids) = &out.drag_started {
        let paths: Vec<PathBuf> = ids.iter().map(PathBuf::from).collect();
        if start_drag(&paths) {
            ctx.stop_dragging();
        }
    }

    let mut v = view.borrow_mut();
    v.drop_target = drop_target;
    v.middle_clicked = middle;
    v.activated = activated;
    v.shown = out.shown.clone();
    // a change asked for by the menu wins over the state of this frame
    if let Some(selected) = v.pending_selected.clone() {
        gstate.cursor = selected.last().cloned();
        gstate.selected = selected;
    }
    v.selected = gstate.selected.clone();
    v.open = v.known_dirs.iter().filter(|d| gstate.expanded.contains(*d)).cloned().collect();
    v.columns = std::mem::take(&mut gstate.columns);
    v.grid = gstate;
}
