//! File views: FileListView, FileIconView and FileTreeView show folders of the disk, with
//! selection, keyboard, sorting and native icons. Their folders are listed through a FileWatcher,
//! so a change made by any program shows at the next frame.

use std::collections::HashSet;
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
use pyo3::types::PyDict;

use crate::icons::icon_uri;
use crate::watcher::{default_core, entry, expand, lock, natural_cmp, Core, Entry, FileWatcher};
use crate::{current_ui, kwarg, run_nested_call, UI};

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
    filter: String,
    sort_by: SortBy,
    ascending: bool,
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
            filter: String::new(),
            sort_by: SortBy::parse(sort_by)?,
            ascending: true,
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
    fn entries(&mut self, ctx: &egui::Context) -> Vec<Entry> {
        let folder = self.folder().to_path_buf();
        let all = lock(&self.core).list(Some(ctx), &folder);
        // what was removed from the disk is no longer selected
        let present: HashSet<&Path> = all.iter().map(|e| e.path.as_path()).collect();
        self.selected.retain(|p| present.contains(p.as_path()));
        if self.cursor.as_ref().is_some_and(|c| !present.contains(c.as_path())) {
            self.cursor = self.selected.last().cloned();
        }
        let filter = self.filter.to_lowercase();
        let mut entries: Vec<Entry> = all
            .iter()
            .filter(|e| self.show_hidden || !e.hidden)
            .filter(|e| filter.is_empty() || e.name.to_lowercase().contains(&filter))
            .cloned()
            .collect();
        let sort_by = self.sort_by;
        entries.sort_by(|a, b| {
            let by_name = natural_cmp(&a.name, &b.name);
            let order = match sort_by {
                SortBy::Name => by_name,
                SortBy::Modified => a.modified.cmp(&b.modified).then(by_name),
                SortBy::Size => a.size.cmp(&b.size).then(by_name),
                SortBy::Kind => kind(a).cmp(&kind(b)).then(by_name),
            };
            if self.ascending {
                order
            } else {
                order.reverse()
            }
        });
        if self.dirs_first {
            entries.sort_by_key(|e| !e.is_dir); // stable: the order above is kept inside each group
        }
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
        columns: Vec<(SortBy, f32)>,
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

/// The columns of a list that fit the width: the name takes the rest
fn list_columns(wanted: &[SortBy], width: f32) -> Vec<(SortBy, f32)> {
    let width_of = |c: SortBy| match c {
        SortBy::Name => 0.0,
        SortBy::Modified => 150.0,
        SortBy::Size => 80.0,
        SortBy::Kind => 130.0,
    };
    let mut others: Vec<SortBy> = wanted.iter().copied().filter(|c| *c != SortBy::Name).collect();
    // the last columns go first when the window is narrow
    while !others.is_empty() && width - others.iter().map(|c| width_of(*c)).sum::<f32>() < 160.0 {
        others.pop();
    }
    let name = width - others.iter().map(|c| width_of(*c)).sum::<f32>();
    std::iter::once((SortBy::Name, name))
        .chain(others.into_iter().map(|c| (c, width_of(c))))
        .collect()
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

/// The header of a list: a click on a column sorts by it, a second click reverses the order
fn list_header(ui: &mut egui::Ui, view: &mut FolderView, shape: &Shape, id: Id) {
    let Shape::List { columns, headers, row_height, .. } = shape else { return };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), *row_height), Sense::hover());
    let mut x = rect.left();
    for (i, (column, width)) in columns.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(*width, rect.height()));
        x += width;
        let response = ui.interact(cell, id.with(("header", i)), Sense::click());
        if response.clicked() {
            if view.sort_by == *column {
                view.ascending = !view.ascending;
            } else {
                view.sort_by = *column;
                view.ascending = true;
            }
        }
        let active = view.sort_by == *column;
        let color = if active || response.hovered() {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        let text = &headers[*column as usize];
        let galley = truncated(ui, text, width - 26.0, Some(color));
        let pos = pos2(cell.left() + 8.0, cell.center().y - galley.size().y / 2.0);
        let text_right = pos.x + galley.size().x;
        ui.painter().galley(pos, galley, color);
        if active {
            paint_sort_arrow(ui, pos2(text_right + 9.0, cell.center().y), view.ascending, color);
        }
        if i > 0 {
            ui.painter().vline(
                cell.left(),
                cell.y_range().shrink(4.0),
                Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
            );
        }
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
}

fn paint_list_row(ui: &egui::Ui, e: &Entry, rect: Rect, shape: &Shape, text_color: Option<Color32>) {
    let Shape::List { columns, date_format, .. } = shape else { return };
    let mut x = rect.left();
    for (column, width) in columns {
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(*width, rect.height()));
        x += width;
        match column {
            SortBy::Name => {
                let side = 16.0;
                let icon = Rect::from_center_size(pos2(cell.left() + 8.0 + side / 2.0, cell.center().y), vec2(side, side));
                paint_icon(ui, e, icon);
                let left = icon.right() + 6.0;
                let galley = truncated(ui, &e.name, cell.right() - left - 6.0, text_color);
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
                let left = if *other == SortBy::Size {
                    cell.right() - 8.0 - galley.size().x // numbers are right aligned
                } else {
                    cell.left() + 8.0
                };
                ui.painter().galley(pos2(left, cell.center().y - galley.size().y / 2.0), galley, color);
            }
        }
    }
}

fn paint_tile(ui: &egui::Ui, e: &Entry, rect: Rect, icon: f32, selected: bool, hovered: bool, focused: bool) {
    let icon_rect = Rect::from_center_size(pos2(rect.center().x, rect.top() + 6.0 + icon / 2.0), vec2(icon, icon));
    if selected {
        ui.painter().rect_filled(icon_rect.expand(4.0), 6.0, ui.visuals().text_color().gamma_multiply(0.12));
    } else if hovered {
        ui.painter().rect_filled(rect.shrink(2.0), 6.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
    }
    paint_icon(ui, e, icon_rect);

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
fn draw_folder(ui: &mut egui::Ui, view: &mut FolderView, mut shape: Shape, has_menu: bool) -> Menus {
    let id = ui.make_persistent_id(&view.id);
    let ctx = ui.ctx().clone();
    view.activated.clear();
    view.folder_changed = false;
    let mut menus = Menus::new();

    let width = ui.available_width();
    if let Shape::Icons { icon, tile, per_row } = &mut shape {
        let line = ui.text_style_height(&TextStyle::Body);
        let min_w = (*icon + 44.0).max(84.0);
        *per_row = ((width / min_w).floor() as usize).max(1);
        *tile = Vec2Size {
            w: width / *per_row as f32,
            h: 6.0 + *icon + 6.0 + 2.0 * line + 10.0,
        };
    }
    if let Shape::List { columns, .. } = &mut shape {
        let wanted: Vec<SortBy> = columns.iter().map(|(c, _)| *c).collect();
        *columns = list_columns(&wanted, width);
    }

    match view.focus.take() {
        Some(true) => ui.memory_mut(|m| m.request_focus(id)),
        Some(false) => ui.memory_mut(|m| m.surrender_focus(id)),
        None => {}
    }
    lock_arrows(ui, id);
    let focused = ui.memory(|m| m.has_focus(id));

    if matches!(shape, Shape::List { .. }) {
        list_header(ui, view, &shape, id);
    }
    // the header may have changed the order
    let entries = view.entries(&ctx);

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
    egui::ScrollArea::vertical()
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
                if has_menu {
                    let paths = if selected { view.selected.iter().map(|p| to_str(p)).collect() } else { vec![to_str(&e.path)] };
                    menus.push((response.clone(), paths));
                }
                match &shape {
                    Shape::List { .. } => {
                        if selected {
                            ui.painter().rect_filled(rect, 4.0, fill);
                        } else if response.hovered() {
                            ui.painter().rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
                        } else if i % 2 == 1 {
                            ui.painter().rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
                        }
                        paint_list_row(ui, e, rect, &shape, selected.then_some(selected_text));
                    }
                    Shape::Icons { icon, .. } => {
                        paint_tile(ui, e, rect, *icon, selected, response.hovered(), focused);
                    }
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
    ($class:ident) => {
        #[pymethods]
        impl $class {
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

            /// Only the items whose name contains this text are shown (without case). Empty shows all
            #[getter]
            fn get_filter(&self) -> String {
                self.view.filter.clone()
            }

            #[setter]
            fn set_filter(&mut self, filter: String) {
                self.view.filter = filter;
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

            /// The order of the items: "name", "modified", "size" or "kind". A click on a column of
            /// the list view changes it
            #[getter]
            fn get_sort_by(&self) -> &'static str {
                self.view.sort_by.name()
            }

            #[setter]
            fn set_sort_by(&mut self, sort_by: &str) -> PyResult<()> {
                self.view.sort_by = SortBy::parse(sort_by)?;
                Ok(())
            }

            /// False reverses the order
            #[getter]
            fn get_ascending(&self) -> bool {
                self.view.ascending
            }

            #[setter]
            fn set_ascending(&mut self, ascending: bool) {
                self.view.ascending = ascending;
            }

            /// Gives the keyboard focus to the view (True, the default) or takes it away (False) at
            /// the next frame. A click on the view gives it the focus too
            #[pyo3(signature = (focused=true))]
            fn focus(&mut self, focused: bool) {
                self.view.focus = Some(focused);
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
/// of a name selects it. A click on a column header sorts by it.
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
///     sort_by (str): "name", "modified", "size" or "kind". Default is "name"
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
///     sort_by (str): "name", "modified", "size" or "kind". Default is "name"
///
/// Usage::
///
///     icons = FileIconView("icons", "~/Pictures")
///
///     def update_func(ctx):
///         file_icon_view(icons, icon_size=64)
///         if icons.selected:
///             label(f"{len(icons.selected)} selected")
#[pyclass]
pub(crate) struct FileIconView {
    view: FolderView,
}

folder_view_methods!(FileIconView);

fn parse_columns(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Vec<(SortBy, f32)>> {
    let names = kwarg::<Vec<String>>(kwargs, "columns")?
        .unwrap_or_else(|| vec!["name".into(), "modified".into(), "size".into(), "kind".into()]);
    let mut columns = vec![(SortBy::Name, 0.0)];
    for name in names {
        let column = SortBy::parse(&name)?;
        if column != SortBy::Name {
            columns.push((column, 0.0));
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
///         Default is all three. The last ones are hidden when the view is narrow
///
///     headers (list[str]): the titles of the columns name, modified, size and kind. Default is
///         ["Name", "Date Modified", "Size", "Kind"]
///
///     date_format (str): format of the dates, like strftime. Default is "%d %b %Y %H:%M"
///
///     row_height (float): height of a row. Default is 24
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
#[pyfunction]
#[pyo3(signature = (view, **kwargs))]
unsafe fn file_list_view(view: &Bound<'_, FileListView>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let headers = match kwarg::<Vec<String>>(kwargs, "headers")? {
        Some(h) if h.len() == 4 => [h[0].clone(), h[1].clone(), h[2].clone(), h[3].clone()],
        Some(_) => return Err(PyValueError::new_err("headers needs 4 titles: name, modified, size and kind")),
        None => ["Name".into(), "Date Modified".into(), "Size".into(), "Kind".into()],
    };
    let shape = Shape::List {
        columns: parse_columns(kwargs)?,
        headers,
        date_format: kwarg::<String>(kwargs, "date_format")?.unwrap_or_else(|| "%d %b %Y %H:%M".into()),
        row_height: kwarg::<f32>(kwargs, "row_height")?.unwrap_or(24.0),
    };
    let menu = kwarg::<Bound<'_, PyAny>>(kwargs, "context_menu")?;
    let ui = current_ui(&UI)?;
    let menus = draw_folder(ui, &mut view.borrow_mut().view, shape, menu.is_some());
    run_menus(menus, menu.as_ref())
}

/// Draws a FileIconView: the files of its folder as a grid of icons. It takes the space left in
/// the ui and scrolls; only the visible tiles are drawn.
///
/// Args:
///     view (FileIconView): the state of the view
///
/// Keyword args:
///     icon_size (float): size of the icons. Default is 48
///
///     context_menu (callable): called with the list of paths concerned on a right click, inside a
///         menu, like for file_list_view
///
/// Example::
///
///     icons = FileIconView("icons", "~/Desktop")
///
///     def update_func(ctx):
///         file_icon_view(icons, icon_size=64)
#[pyfunction]
#[pyo3(signature = (view, **kwargs))]
unsafe fn file_icon_view(view: &Bound<'_, FileIconView>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let shape = Shape::Icons {
        icon: kwarg::<f32>(kwargs, "icon_size")?.unwrap_or(48.0).clamp(8.0, 512.0),
        tile: Vec2Size { w: 1.0, h: 1.0 },
        per_row: 1,
    };
    let menu = kwarg::<Bound<'_, PyAny>>(kwargs, "context_menu")?;
    let ui = current_ui(&UI)?;
    let menus = draw_folder(ui, &mut view.borrow_mut().view, shape, menu.is_some());
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
    filter: String,
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
    scroll_to: Option<String>,
}

#[pymethods]
impl FileTreeView {
    #[new]
    #[pyo3(signature = (id, root=PathBuf::from("."), watcher=None, show_hidden=false, multi_select=true, show_root=false, dirs_first=true))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        id: String,
        root: PathBuf,
        watcher: Option<PyRef<'_, FileWatcher>>,
        show_hidden: bool,
        multi_select: bool,
        show_root: bool,
        dirs_first: bool,
    ) -> Self {
        FileTreeView {
            id,
            core: core_of(watcher),
            root: expand(&root),
            show_hidden,
            multi_select,
            show_root,
            dirs_first,
            filter: String::new(),
            state: TreeViewState::default(),
            selected: vec![],
            open: HashSet::new(),
            pending_selected: None,
            pending_open: vec![],
            known_dirs: HashSet::new(),
            activated: vec![],
            focus: None,
            scroll_to: None,
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

    /// Paths of the files activated during the last frame by a double click or Enter
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

    /// Only the files whose name contains this text are shown (without case), the folders always
    #[getter]
    fn get_filter(&self) -> String {
        self.filter.clone()
    }

    #[setter]
    fn set_filter(&mut self, filter: String) {
        self.filter = filter;
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

    /// Gives the keyboard focus to the tree (True, the default) or takes it away (False) at the
    /// next frame
    #[pyo3(signature = (focused=true))]
    fn focus(&mut self, focused: bool) {
        self.focus = Some(focused);
    }
}

/// A node of the tree, built before it is drawn
struct Node {
    entry: Entry,
    id: String,
    /// the items of an open folder
    children: Option<Vec<Node>>,
}

struct TreeOptions {
    show_hidden: bool,
    dirs_first: bool,
    filter: String,
}

fn tree_children(
    core: &mut Core,
    ctx: &egui::Context,
    dir: &Path,
    options: &TreeOptions,
    state: &TreeViewState<String>,
    known_dirs: &mut HashSet<String>,
) -> Vec<Node> {
    let all = core.list(Some(ctx), dir);
    let mut entries: Vec<&Entry> = all
        .iter()
        .filter(|e| options.show_hidden || !e.hidden)
        .filter(|e| e.is_dir || options.filter.is_empty() || e.name.to_lowercase().contains(&options.filter))
        .collect();
    entries.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    if options.dirs_first {
        entries.sort_by_key(|e| !e.is_dir);
    }
    entries
        .into_iter()
        .map(|e| tree_node(core, ctx, e.clone(), options, state, known_dirs, false))
        .collect()
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
    Node { entry, id, children }
}

fn with_tree_icon(b: NodeBuilder<'_, String>, uri: Option<String>, side: f32) -> NodeBuilder<'_, String> {
    match uri {
        Some(uri) => b.icon(move |ui| {
            ui.add(egui::Image::new(uri.as_str()).fit_to_exact_size(vec2(side, side)));
            ui.add_space(4.0);
        }),
        None => b,
    }
}

fn add_tree_nodes(builder: &mut TreeViewBuilder<'_, String>, nodes: &[Node], ctx: &egui::Context, open_by_default: bool) {
    for node in nodes {
        let side = 16.0;
        let uri = icon_uri(ctx, &node.id, side);
        let label = match uri {
            Some(_) => node.entry.name.clone(),
            None => format!("{}  {}", emoji(&node.entry), node.entry.name),
        };
        let with_icon = |b| with_tree_icon(b, uri.clone(), side);
        if node.entry.is_dir {
            builder.node(with_icon(
                NodeBuilder::dir(node.id.clone())
                    .default_open(open_by_default)
                    .activatable(false)
                    .label(label),
            ));
            if let Some(children) = &node.children {
                add_tree_nodes(builder, children, ctx, false);
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
    let ui = current_ui(&UI)?;
    let ctx = ui.ctx().clone();

    let (mut state, nodes, id, multi_select, show_root, scroll_to) = {
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
        let options = TreeOptions {
            show_hidden: v.show_hidden,
            dirs_first: v.dirs_first,
            filter: v.filter.to_lowercase(),
        };
        let core = v.core.clone();
        let mut core = lock(&core);
        let mut known = std::mem::take(&mut v.known_dirs);
        let nodes = if v.show_root {
            match entry(&v.root) {
                Some(root) => vec![tree_node(&mut core, &ctx, root, &options, &state, &mut known, true)],
                None => vec![],
            }
        } else {
            tree_children(&mut core, &ctx, &v.root, &options, &state, &mut known)
        };
        v.known_dirs = known;
        let id = ui.make_persistent_id(&v.id);
        match v.focus.take() {
            Some(true) => ui.memory_mut(|m| m.request_focus(id)),
            Some(false) => ui.memory_mut(|m| m.surrender_focus(id)),
            None => {}
        }
        (state, nodes, id, v.multi_select, v.show_root, v.scroll_to.take())
    };

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
    let (_, actions) = tree.show_state(ui, &mut state, |builder| add_tree_nodes(builder, &nodes, &ctx, show_root));

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
