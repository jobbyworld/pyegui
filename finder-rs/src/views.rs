//! The views of a folder: the list, the icons and the tree are the file views of pyegui-core
//! (`FolderView`, `TreeState`), the same code as `FileListView`, `FileIconView` and `FileTreeView`
//! of the Python module. The application keeps the folder, the history, the selection and the sort,
//! gives them to the view before it is drawn and reads what the user did after.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui;
use pyegui_core::columns::{DataType, HeaderOptions, Value};
use pyegui_core::files::{
    draw_folder, draw_tree, Column, Extra, FolderView, Hooks, Shape, Shared, SortBy, TreeParams, TreeState, Vec2Size,
};
use pyegui_core::watcher::{expand, lock, Core, Entry};

use crate::actions::{self, ACTIONS};
use crate::app::{FinderApp, ViewMode};
use crate::model::SortKey;
use crate::tagsui;

const ROW_HEIGHT: f32 = 24.0;
const DATE_FORMAT: &str = "%Y-%m-%d %H:%M";
/// The size of the icons of the icon view
pub const ICON_SIZE: f32 = 56.0;

/// The state of the three views, kept between frames. The list and the icons are taken out of it
/// while they are drawn, so the closures that give the tags can read the application
pub struct Views {
    list: Option<FolderView>,
    icons: Option<FolderView>,
    tree: Shared<TreeState>,
    /// the items that carry the tag of the tag view, until the tag or the store changes
    tagged: Option<((i64, u64), Vec<PathBuf>)>,
    /// the items shown at the last frame, and the version of the listings they were made from
    shown_key: Option<(Vec<PathBuf>, u64)>,
}

impl Views {
    pub fn new(core: &Arc<Mutex<Core>>, folder: &Path, show_hidden: bool) -> Self {
        let make = |id: &str| {
            let mut v = FolderView::new(id.into(), folder.to_path_buf(), core.clone(), show_hidden, true, false, true, "name");
            v.keep_focus = true;
            v
        };
        let mut icons = make("icons");
        icons.icon_size = ICON_SIZE;
        let mut tree = TreeState::new("tree".into(), folder.to_path_buf(), core.clone(), show_hidden, true, false, true, true);
        tree.keep_focus = true;
        Views { list: Some(make("list")), icons: Some(icons), tree: Shared::new(tree), tagged: None, shown_key: None }
    }

    /// The widths of the columns the user chose, by view ("list" and "tree") and column key
    pub fn column_widths(&self) -> BTreeMap<String, BTreeMap<String, f32>> {
        let mut out = BTreeMap::new();
        let sorted = |w: &HashMap<String, f32>| w.iter().map(|(k, v)| (k.clone(), *v)).collect::<BTreeMap<_, _>>();
        if let Some(list) = &self.list {
            out.insert("list".to_owned(), sorted(&list.columns.widths));
        }
        out.insert("tree".to_owned(), sorted(&self.tree.borrow().columns.widths));
        out.retain(|_, w| !w.is_empty());
        out
    }

    /// Gives the views these widths again (what was kept between two runs, or chosen in another tab)
    pub fn set_column_widths(&mut self, widths: &BTreeMap<String, BTreeMap<String, f32>>) {
        if &self.column_widths() == widths {
            return;
        }
        let map = |name: &str| widths.get(name).map(|w| w.iter().map(|(k, v)| (k.clone(), *v)).collect::<HashMap<_, _>>()).unwrap_or_default();
        if let Some(list) = &mut self.list {
            list.columns.set_widths(map("list"));
        }
        self.tree.borrow_mut().columns.set_widths(map("tree"));
    }

    /// Opens these folders in the tree
    pub fn open_in_tree(&self, folders: &[PathBuf]) {
        let mut tree = self.tree.borrow_mut();
        for folder in folders {
            tree.set_open(folder, true);
        }
    }

    /// The folder of the view of the mode under the pointer while files are dragged over it or dropped
    /// on it (the value of the last frame), if there is one
    pub fn drop_target(&self, mode: ViewMode) -> Option<PathBuf> {
        match mode {
            ViewMode::Tree => self.tree.borrow().drop_target.as_ref().map(PathBuf::from),
            ViewMode::Icons => self.icons.as_ref().and_then(|v| v.drop_target.clone()),
            ViewMode::List => self.list.as_ref().and_then(|v| v.drop_target.clone()),
        }
    }

    /// Scrolls the views to an item
    pub fn scroll_to(&mut self, path: &Path) {
        self.tree.borrow_mut().scroll_to = Some(path.to_string_lossy().into_owned());
        for view in [&mut self.list, &mut self.icons].into_iter().flatten() {
            view.scroll_to = Some(path.to_path_buf());
        }
    }
}

fn sort_name(key: SortKey) -> &'static str {
    match key {
        SortKey::Name => "name",
        SortKey::Modified => "modified",
        SortKey::Size => "size",
        SortKey::Kind => "kind",
    }
}

fn sort_key(name: &str) -> Option<SortKey> {
    Some(match name {
        "name" => SortKey::Name,
        "modified" => SortKey::Modified,
        "size" => SortKey::Size,
        "kind" => SortKey::Kind,
        _ => return None,
    })
}

/// The columns that are not in the views: the tags of an item and when it was last opened
fn hooks(app: &FinderApp) -> Hooks<'_> {
    let tags = Extra {
        title: "Étiquettes".into(),
        width: 140.0,
        cell: Box::new(move |ui, path| tagsui::cell(ui, app, path)),
        ty: DataType::Choice,
        value: Some(Box::new(move |path| {
            let tags = app.row_tags(path);
            if tags.is_empty() {
                Value::None
            } else {
                Value::Text(tags.iter().map(|(_, name, _)| name.clone()).collect::<Vec<_>>().join(", "))
            }
        })),
    };
    let opened = Extra {
        title: "Dernière ouverture".into(),
        width: 140.0,
        cell: Box::new(move |ui, path| {
            ui.add(egui::Label::new(egui::RichText::new(crate::model::format_date(app.opened_at(path))).weak()).selectable(false));
        }),
        ty: DataType::Date,
        value: Some(Box::new(move |path| match app.opened_at(path).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()) {
            Some(d) => Value::Num(d.as_secs_f64()),
            None => Value::None,
        })),
    };
    Hooks { badges: None, extras: vec![tags, opened] }
}

fn headers() -> [String; 4] {
    ["Nom".into(), "Modifié".into(), "Taille".into(), "Type".into()]
}

fn options() -> HeaderOptions {
    HeaderOptions { resizable: true, pinnable: true, reorderable: true, filterable: true, drag_out: true, grow_left: false, tinted: true }
}

/// The paths of the tag view, read again only when the tag or the store changes
fn tagged(app: &FinderApp, cache: &mut Option<((i64, u64), Vec<PathBuf>)>) -> Option<Vec<PathBuf>> {
    let tag = app.tag_filter?;
    let store = app.tags.as_ref()?;
    let key = (tag, store.version);
    if cache.as_ref().map(|c| c.0) != Some(key) {
        *cache = Some((key, store.tagged_with(tag).into_iter().map(PathBuf::from).collect()));
    }
    cache.as_ref().map(|c| c.1.clone())
}

fn same_set(selected: &HashSet<PathBuf>, view: &[PathBuf]) -> bool {
    selected.len() == view.len() && view.iter().all(|p| selected.contains(p))
}

/// What the application tells a list or icon view before it is drawn
fn feed_folder(view: &mut FolderView, app: &FinderApp, paths: &Option<Vec<PathBuf>>) {
    if view.folder() != app.folder() {
        view.go(app.folder());
    }
    view.show_hidden = app.show_hidden;
    // a field being typed in keeps the focus
    view.keep_focus = app.path_text.is_none();
    view.columns.set_name_text(app.search.clone());
    view.columns.sort_key = sort_name(app.sort_key).into();
    view.columns.ascending = app.ascending;
    if &view.paths != paths {
        view.paths = paths.clone();
    }
    if !same_set(&app.selected, &view.selected) {
        view.selected = app.selected.iter().cloned().collect();
        view.cursor = view.selected.last().cloned();
        view.anchor = view.cursor.clone();
    }
}

/// What the user did in a view during a frame, applied once the view is free
#[derive(Default)]
struct Outcome {
    action: Option<usize>,
    tag: Option<(i64, bool)>,
    /// a right click: the palette of the actions opens (there is no other context menu)
    palette: bool,
}

impl Outcome {
    fn apply(self, app: &mut FinderApp, ctx: &egui::Context) {
        if self.palette {
            app.open_palette();
        }
        if let Some(action) = self.action {
            (ACTIONS[action].run)(app, ctx);
        }
        if let Some((tag, on)) = self.tag {
            let paths = app.selected_paths();
            if let Some(store) = app.tags.as_mut() {
                for path in &paths {
                    if on {
                        store.tag(path, tag)
                    } else {
                        store.untag(path, tag)
                    }
                }
            }
        }
    }
}

/// What the view read back: the selection, the sort, the items shown, the items opened
struct Back {
    selected: Vec<PathBuf>,
    sort: (String, bool),
    shown: Vec<PathBuf>,
    activated: Vec<PathBuf>,
    folder: Option<PathBuf>,
}

/// Draws the view of the mode (list, icons or tree) in the central area
pub fn show(app: &mut FinderApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let mut cache = app.views.tagged.take();
    let paths = tagged(app, &mut cache);
    app.views.tagged = cache;
    // the widths of the columns are the same in every tab (and kept between two runs)
    let widths = app.column_widths.clone();
    app.views.set_column_widths(&widths);
    let mut out = Outcome::default();
    let back;

    if app.view == ViewMode::Tree {
        let shared = app.views.tree.clone();
        {
            let mut t = shared.borrow_mut();
            let root = expand(app.folder());
            if t.root != root {
                t.root = root;
            }
            t.show_hidden = app.show_hidden;
            t.keep_focus = app.path_text.is_none();
            t.columns.set_name_text(app.search.clone());
            t.columns.sort_key = sort_name(app.sort_key).into();
            t.columns.ascending = app.ascending;
            if t.paths != paths {
                t.paths = paths.clone();
            }
            let mut wanted: Vec<PathBuf> = app.selected.iter().cloned().collect();
            wanted.sort();
            let mut current: Vec<PathBuf> = t.selected.iter().map(PathBuf::from).collect();
            current.sort();
            if wanted != current {
                t.select_paths(&wanted);
            }
        }
        let params = TreeParams {
            opts: options(),
            headers: headers(),
            date_format: DATE_FORMAT.into(),
            builtin: vec![SortBy::Modified, SortBy::Size, SortBy::Kind],
            header: true,
        };
        {
            let hooks = hooks(app);
            let menu_out = std::cell::RefCell::new(Outcome::default());
            // the menu of the tree is the palette: the popup of egui is closed at once
            let menu = |ui: &mut egui::Ui, _ids: &Vec<String>| {
                menu_out.borrow_mut().palette = true;
                ui.close();
            };
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                draw_tree(ui, &shared, &hooks, &params, Some(&menu));
            });
            out = menu_out.into_inner();
        }
        let t = shared.borrow();
        back = Back {
            selected: t.selected.iter().map(PathBuf::from).collect(),
            sort: (t.columns.sort_key.clone(), t.columns.ascending),
            shown: t.shown.iter().map(PathBuf::from).collect(),
            activated: t.activated.iter().map(PathBuf::from).collect(),
            folder: None,
        };
        app.open_folders = t.open.iter().map(PathBuf::from).collect();
    } else {
        let icons = app.view == ViewMode::Icons;
        let mut view = if icons { app.views.icons.take() } else { app.views.list.take() }.expect("view in use");
        feed_folder(&mut view, app, &paths);
        let shape = if icons {
            Shape::Icons { icon: ICON_SIZE, tile: Vec2Size { w: 1.0, h: 1.0 }, per_row: 1 }
        } else {
            use SortBy::*;
            let mut columns: Vec<(Column, f32)> = [Name, Modified, Size, Kind].into_iter().map(|s| (Column::Sort(s), 0.0)).collect();
            columns.extend([(Column::Extra(0), 0.0), (Column::Extra(1), 0.0)]);
            Shape::List { columns, headers: headers(), date_format: DATE_FORMAT.into(), row_height: ROW_HEIGHT }
        };
        let menus = {
            let hooks = hooks(app);
            draw_folder(ui, &mut view, shape, &hooks, true, options())
        };
        for (response, _paths) in menus {
            if response.secondary_clicked() {
                out.palette = true;
            }
        }
        back = Back {
            selected: view.selected.clone(),
            sort: (view.columns.sort_key.clone(), view.columns.ascending),
            shown: view.shown.clone(),
            activated: view.activated.clone(),
            folder: (view.folder() != app.folder()).then(|| view.folder().to_path_buf()),
        };
        if icons {
            app.views.icons = Some(view);
        } else {
            app.views.list = Some(view);
        }
    }

    tagsui::apply_edits(app);
    app.selected = back.selected.into_iter().collect();
    if let Some(key) = sort_key(&back.sort.0) {
        app.sort_key = key;
        app.ascending = back.sort.1;
    }
    app.column_widths = app.views.column_widths();
    app.set_items(&ctx, back.shown);
    if let Some(folder) = back.folder {
        app.go(folder);
    }
    for path in back.activated {
        app.open(&path);
    }
    out.apply(app, &ctx);
}

impl Views {
    /// The items to show as entries, kept while the paths and the listings stay the same
    pub fn items(&mut self, ctx: &egui::Context, core: &Arc<Mutex<Core>>, shown: Vec<PathBuf>) -> Option<Vec<Entry>> {
        let version = lock(core).version();
        if self.shown_key.as_ref().is_some_and(|(paths, v)| *paths == shown && *v == version) {
            return None;
        }
        // the listings of the folders of the items, each read once
        let mut listings: Vec<Arc<Vec<Entry>>> = vec![];
        let mut folders: Vec<&Path> = vec![];
        for path in &shown {
            if let Some(parent) = path.parent() {
                if !folders.contains(&parent) {
                    folders.push(parent);
                    listings.push(lock(core).list(Some(ctx), parent));
                }
            }
        }
        let index: std::collections::HashMap<&Path, &Entry> = listings.iter().flat_map(|l| l.iter()).map(|e| (e.path.as_path(), e)).collect();
        let items: Vec<Entry> = shown.iter().filter_map(|p| index.get(p.as_path()).map(|e| (*e).clone())).collect();
        self.shown_key = Some((shown, version));
        Some(items)
    }
}
