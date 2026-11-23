//! The window: sidebar, toolbar, the folder in the middle, a preview on the right and a status bar.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, Key, Modifiers, RichText};
use pyegui_core::icons;
use pyegui_core::style;
use pyegui_core::transfer::Transfer;
use pyegui_core::watcher::{default_core, expand, lock, Core, Entry};
use serde::{Deserialize, Serialize};

use crate::actions;
use crate::dock::{self, Pane};
use crate::glyphs;
use pyegui_core::path::{breadcrumb, combo, BreadcrumbOptions, ComboOptions};
use crate::model::{self, History, SortKey};
use crate::tagstore::{How, TagStore};
use crate::tagsui::{self, TagManager, TagPicker};
use crate::terminals::{self, Terminals};
use crate::viewer::{self, Viewer};
use crate::views::{self, Views};

const STATE_KEY: &str = "finder-state";
pub const ACCENTS: [(&str, Color32); 4] = [
    ("Bleu", Color32::from_rgb(59, 130, 246)),
    ("Violet", Color32::from_rgb(139, 92, 246)),
    ("Vert", Color32::from_rgb(34, 197, 94)),
    ("Orange", Color32::from_rgb(249, 115, 22)),
];

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    List,
    Icons,
    Tree,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    System,
    Light,
    Dark,
}

/// What is behind the glasses of the window
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Background {
    /// the native glass of the system (what is behind the window, blurred)
    Glass,
    /// a picture, blurred
    Image,
    /// a plain color
    Color,
}

impl Background {
    fn default_for_system() -> Self {
        if backdrop_name().is_some() { Background::Glass } else { Background::Color }
    }
}

/// The look of one theme (light or dark): the background and the three glasses
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    pub background: Background,
    pub color: Option<[u8; 3]>,
    pub image: Option<PathBuf>,
    /// the blur of the glass or of the picture
    pub blur: u32,
    pub layers: [LayerSetting; 3],
    /// the outline of the tabs and of the panes: thickness and color with its alpha (None: the one of the theme)
    /// [active tabs and panes, inactive tabs]
    pub outline_width: [f32; 2],
    pub outline: [Option<[u8; 4]>; 2],
    /// the color of the text (None: the one of the theme)
    pub text: Option<[u8; 3]>,
}

impl Default for Look {
    fn default() -> Self {
        Look { background: Background::default_for_system(), color: None, image: None, blur: 0, layers: Default::default(), outline_width: [1.0; 2], outline: [None; 2], text: None }
    }
}

fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"))
}

/// The menu of the appearance stays open while a click lands in it or in a popup of a color picker;
/// a click elsewhere (but on its button) closes it
fn close_on_click_outside(ui: &egui::Ui) {
    let ctx = ui.ctx();
    if !ctx.input(|i| i.pointer.any_pressed()) {
        return;
    }
    let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) else { return };
    let on_button = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("gear-rect"))).is_some_and(|r| r.contains(pos));
    let inside = ctx.layer_id_at(pos).is_some_and(|l| matches!(l.order, egui::Order::Foreground | egui::Order::Tooltip));
    if !inside && !on_button {
        ui.close();
    }
}

/// The native backdrop put behind the window when the glass is on
fn backdrop_name() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("liquid_glass")
    } else if cfg!(target_os = "windows") {
        Some("acrylic")
    } else {
        None
    }
}

/// The tint and the opacity chosen for one glass, None for the default ones
#[derive(Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
pub struct LayerSetting {
    pub tint: Option<[u8; 3]>,
    pub opacity: Option<f32>,
}

const LAYERS: [(style::Layer, &str); 3] = [(style::Layer::Sidebar, "Barre latérale"), (style::Layer::Inactive, "Contenu inactif"), (style::Layer::Active, "Contenu actif")];

/// What is kept between two runs
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    folder: Option<PathBuf>,
    view: ViewMode,
    theme: ThemeChoice,
    accent: usize,
    sort_key: SortKey,
    ascending: bool,
    show_sidebar: bool,
    show_dock: bool,
    show_terminal: bool,
    terminal_opacity: f32,
    /// the look of the light theme and of the dark theme, kept apart
    looks: [Look; 2],
    /// an accent chosen with the color picker
    accent_custom: Option<[u8; 3]>,
    show_hidden: bool,
    /// Windows: the folders in the blue of the Finder, not the yellow of the Explorer
    blue_folders: bool,
    dock: Option<String>,
    open_folders: Vec<PathBuf>,
}

impl Default for Saved {
    fn default() -> Self {
        Saved {
            folder: None,
            view: ViewMode::List,
            theme: ThemeChoice::System,
            accent: 0,
            sort_key: SortKey::Name,
            ascending: true,
            show_sidebar: true,
            show_dock: true,
            show_terminal: true,
            terminal_opacity: 0.2,
            looks: Default::default(),
            accent_custom: None,
            show_hidden: false,
            blue_folders: !cfg!(target_os = "macos"),
            dock: None,
            open_folders: Vec::new(),
        }
    }
}

pub struct FinderApp {
    core: Arc<Mutex<Core>>,
    pub history: History,
    pub view: ViewMode,
    pub theme: ThemeChoice,
    pub accent: usize,
    pub sort_key: SortKey,
    pub ascending: bool,
    show_sidebar: bool,
    show_dock: bool,
    show_terminal: bool,
    pub terminal_opacity: f32,
    pub looks: [Look; 2],
    pub accent_custom: Option<[u8; 3]>,
    /// the picture behind the glasses, blurred: the file, the blur and its texture
    bg_texture: Option<(PathBuf, u32, egui::TextureHandle)>,
    applied_blur: u32,
    /// the blur once the slider is let go (while it is dragged the window server and the picture are not asked again at each frame)
    settled_blur: Option<[u32; 2]>,
    title: String,
    /// a window opened by the first one: it has no native backdrop and closes on its own
    secondary: bool,
    /// the other windows, drawn after this one
    windows: Vec<(egui::ViewportId, Box<FinderApp>)>,
    pub new_window: Option<PathBuf>,
    window_serial: u64,
    backdrop_applied: bool,
    corners_applied: bool,
    /// the keyboard goes back to the view (after a navigation, a search field that is still focused lets it go)
    focus_view: bool,
    custom_titlebar: bool,
    terminals: Terminals,
    pub show_hidden: bool,
    blue_folders: bool,
    pub search: String,
    /// the items shown at the last frame, in their order (what the views show: filters and sort included)
    pub items: Vec<Entry>,
    /// the views of the folder (list, icons, tree): the file views of pyegui-core
    pub views: Views,
    pub open_folders: std::collections::BTreeSet<PathBuf>,
    pub selected: HashSet<PathBuf>,
    pub path_text: Option<String>,
    pub preview_text: Option<(PathBuf, String)>,
    /// copied or cut paths (true: cut)
    pub clipboard: Option<(Vec<PathBuf>, bool)>,
    pub transfer: Transfer,
    /// a move waits here for its files, to select them
    pasted_cut: bool,
    rename: Option<(PathBuf, String)>,
    confirm_delete: Option<Vec<PathBuf>>,
    /// the action palette: (query, highlighted row)
    palette: Option<(String, usize)>,
    pub notice: Option<String>,
    dock: Option<egui_tiles::Tree<Pane>>,
    /// what the system told about the disk, newest last
    pub activity: std::collections::VecDeque<String>,
    select_after: Vec<PathBuf>,
    pub tags: Option<TagStore>,
    pub search_state: crate::search::Search,
    pub tag_filter: Option<i64>,
    /// the search or creation field of the tags pane
    pub tags_query: String,
    /// the tags folded in the tree of the sidebar
    tag_folded: HashSet<i64>,
    pub tag_manager: Option<TagManager>,
    pub tag_picker: Option<TagPicker>,
    /// the tags of the rows shown, by path, until the store changes
    tag_cache: std::cell::RefCell<(u64, std::collections::HashMap<PathBuf, std::rc::Rc<Vec<(Color32, String, How)>>>)>,
    /// the tag paths of the rows shown, and the tags of the store with their colors, until the store changes
    tag_paths_cache: std::cell::RefCell<(u64, std::collections::HashMap<PathBuf, std::rc::Rc<tagsui::RowTags>>)>,
    library_cache: std::cell::RefCell<(u64, std::rc::Rc<tagsui::Library>)>,
    /// what the widget of the tags changed in the cells of the rows, applied once the view is drawn
    pub tag_edits: std::cell::RefCell<Vec<tagsui::TagEdit>>,
    opened_cache: std::cell::RefCell<(u64, std::collections::HashMap<String, f64>)>,
    opened_loaded: std::cell::Cell<bool>,
    /// every tag, read once per version of the store (not once per row)
    all_tags: std::cell::RefCell<(u64, Vec<crate::tagstore::Tag>)>,
    viewer: Option<Viewer>,
    /// the tabs of folders that are not shown: their state, by number of tab
    parked_tabs: std::collections::HashMap<u64, TabState>,
    /// the number of the tab whose state is in the fields above
    current_tab: u64,
    next_tab: u64,
    /// a folder asked to open in a new tab while the dock was drawn (the dock is taken out then)
    new_tab: Option<PathBuf>,
    preview_pdf: Option<(PathBuf, pyegui_core::pdf_view::PdfDocument)>,
}

/// What belongs to a tab of folder: the folder and its history, the views, the selection, the search
/// and the file open instead of the folder. The state of the tab shown is in the fields of the app, the
/// others are parked here and swapped in when their tab is shown
pub struct TabState {
    history: History,
    view: ViewMode,
    sort_key: SortKey,
    ascending: bool,
    search: String,
    items: Vec<Entry>,
    views: Views,
    open_folders: std::collections::BTreeSet<PathBuf>,
    selected: HashSet<PathBuf>,
    path_text: Option<String>,
    tag_filter: Option<i64>,
    viewer: Option<Viewer>,
}

impl FinderApp {
    /// The state of a new tab on a folder
    fn new_tab_state(&self, folder: PathBuf) -> TabState {
        TabState {
            views: Views::new(&self.core, &folder, self.show_hidden),
            history: History::new(folder),
            view: self.view,
            sort_key: self.sort_key,
            ascending: self.ascending,
            search: String::new(),
            items: Vec::new(),
            open_folders: Default::default(),
            selected: HashSet::new(),
            path_text: None,
            tag_filter: None,
            viewer: None,
        }
    }

    /// Asks for a folder in a new tab, next to the one shown: it opens at the start of the next frame
    pub fn open_in_new_tab(&mut self, folder: PathBuf) {
        self.new_tab = Some(folder);
    }

    /// What a new tab or window opens: the item selected when it can be shown (a folder, or a file the
    /// viewer reads: markdown, PDF, image, text), else the folder that holds it; nothing selected, the item
    /// shown now (the file of the viewer, else the folder)
    pub fn new_target(&self) -> PathBuf {
        match self.selected_paths().into_iter().next() {
            Some(path) if path.is_dir() => path,
            Some(path) if viewer::can_view(&path) => path,
            Some(path) => path.parent().map(Path::to_path_buf).unwrap_or_else(|| self.folder().to_path_buf()),
            None => self.viewer_path().map(Path::to_path_buf).unwrap_or_else(|| self.folder().to_path_buf()),
        }
    }

    fn make_new_tab(&mut self, target: PathBuf) {
        // a file opens in the viewer of the new tab, in the folder that holds it
        let (folder, file) = if target.is_file() {
            (target.parent().map(Path::to_path_buf).unwrap_or_else(|| self.folder().to_path_buf()), Some(target))
        } else {
            (target, None)
        };
        let id = self.next_tab;
        self.next_tab += 1;
        let mut state = self.new_tab_state(folder);
        if let Some(file) = file {
            state.viewer = Viewer::open(&file, &self.core);
        }
        self.parked_tabs.insert(id, state);
        let after = self.current_tab;
        if let Some(tree) = self.dock.as_mut() {
            dock::add_central(tree, id, after);
        }
        self.show_dock = true;
    }

    /// Puts the state of the tab `id` in the fields of the app, the state that was there is parked
    pub fn swap_tab(&mut self, id: u64) {
        if id == self.current_tab {
            return;
        }
        let Some(mut state) = self.parked_tabs.remove(&id) else { return };
        std::mem::swap(&mut self.history, &mut state.history);
        std::mem::swap(&mut self.view, &mut state.view);
        std::mem::swap(&mut self.sort_key, &mut state.sort_key);
        std::mem::swap(&mut self.ascending, &mut state.ascending);
        std::mem::swap(&mut self.search, &mut state.search);
        std::mem::swap(&mut self.items, &mut state.items);
        std::mem::swap(&mut self.views, &mut state.views);
        std::mem::swap(&mut self.open_folders, &mut state.open_folders);
        std::mem::swap(&mut self.selected, &mut state.selected);
        std::mem::swap(&mut self.path_text, &mut state.path_text);
        std::mem::swap(&mut self.tag_filter, &mut state.tag_filter);
        std::mem::swap(&mut self.viewer, &mut state.viewer);
        self.parked_tabs.insert(self.current_tab, state);
        self.current_tab = id;
    }

    /// A tab is closed: its state is dropped (the one shown is replaced by another tab first)
    pub fn close_tab(&mut self, id: u64) {
        if id == self.current_tab {
            if let Some(&other) = self.parked_tabs.keys().min() {
                self.swap_tab(other);
            }
        }
        self.parked_tabs.remove(&id);
    }

    /// The tab shows a file (in the viewer) and not its folder
    pub fn tab_has_file(&self, id: u64) -> bool {
        if id == self.current_tab {
            self.viewer.is_some()
        } else {
            self.parked_tabs.get(&id).is_some_and(|t| t.viewer.is_some())
        }
    }

    /// The title of a tab: its folder, or its file
    pub fn tab_title(&self, id: u64) -> String {
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Finder".into());
        if id == self.current_tab {
            return self.viewer_path().map(name).unwrap_or_else(|| name(self.folder()));
        }
        match self.parked_tabs.get(&id) {
            Some(t) => t.viewer.as_ref().map(|v| name(&v.path)).unwrap_or_else(|| name(t.history.current())),
            None => String::new(),
        }
    }

    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<PathBuf>, custom_titlebar: bool) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let font_path = terminals::nerd_font();
        cc.egui_ctx.set_fonts(fonts(font_path.as_deref()));
        let saved: Saved = cc
            .storage
            .and_then(|s| s.get_string(STATE_KEY))
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self::build(&cc.egui_ctx, saved, start, custom_titlebar)
    }

    /// Another window of the app: its own folder, view and selection, the look of this one, no
    /// terminal and no dock
    pub fn new_window(&self, ctx: &egui::Context, target: PathBuf) -> Self {
        // a file opens in the viewer of the new window, in the folder that holds it
        let (folder, file) = if target.is_file() {
            (target.parent().map(Path::to_path_buf).unwrap_or_else(|| self.folder().to_path_buf()), Some(target))
        } else {
            (target, None)
        };
        let saved = Saved {
            view: self.view,
            theme: self.theme,
            accent: self.accent,
            looks: self.looks.clone(),
            accent_custom: self.accent_custom,
            show_hidden: self.show_hidden,
            blue_folders: self.blue_folders,
            show_dock: false,
            show_terminal: false,
            terminal_opacity: self.terminal_opacity,
            ..Saved::default()
        };
        let mut app = Self::build(ctx, saved, Some(folder), false);
        app.secondary = true;
        if let Some(file) = file {
            app.viewer = Viewer::open(&file, &self.core);
        }
        app
    }

    fn build(ctx: &egui::Context, saved: Saved, start: Option<PathBuf>, custom_titlebar: bool) -> Self {
        let font_path = terminals::nerd_font();
        let home = home();
        let folder = start
            .map(|p| expand(&p))
            .filter(|p| p.is_dir())
            .or(saved.folder.filter(|p| p.is_dir()))
            .unwrap_or(home);
        let folder_for_terminal = folder.clone();
        pyegui_core::icons::set_blue(saved.blue_folders);
        let core = default_core();
        let views = Views::new(&core, &folder, saved.show_hidden);
        views.open_in_tree(&saved.open_folders);
        let app = FinderApp {
            core,
            history: History::new(folder),
            view: saved.view,
            theme: saved.theme,
            accent: saved.accent.min(ACCENTS.len() - 1),
            sort_key: saved.sort_key,
            ascending: saved.ascending,
            show_sidebar: saved.show_sidebar,
            show_dock: saved.show_dock,
            show_terminal: saved.show_terminal,
            terminal_opacity: saved.terminal_opacity.clamp(0.0, 1.0),
            looks: saved.looks,
            accent_custom: saved.accent_custom,
            bg_texture: None,
            applied_blur: 0,
            settled_blur: None,
            title: String::new(),
            secondary: false,
            windows: Vec::new(),
            new_window: None,
            window_serial: 0,
            backdrop_applied: false,
            corners_applied: false,
            focus_view: false,
            custom_titlebar,
            terminals: Terminals::new(&folder_for_terminal, font_path.as_deref()),
            show_hidden: saved.show_hidden,
            blue_folders: saved.blue_folders,
            search: String::new(),
            items: Vec::new(),
            views,
            open_folders: saved.open_folders.into_iter().collect(),
            selected: HashSet::new(),
            path_text: None,
            preview_text: None,
            clipboard: None,
            transfer: Transfer::new(),
            pasted_cut: false,
            rename: None,
            confirm_delete: None,
            palette: None,
            notice: None,
            dock: Some(dock::restore(saved.dock.as_deref())),
            activity: Default::default(),
            select_after: Vec::new(),
            tags: TagStore::open(None).ok(),
            search_state: Default::default(),
            tag_filter: None,
            tags_query: String::new(),
            tag_folded: HashSet::new(),
            tag_manager: None,
            tag_picker: None,
            tag_cache: Default::default(),
            tag_paths_cache: Default::default(),
            library_cache: Default::default(),
            tag_edits: Default::default(),
            opened_cache: Default::default(),
            opened_loaded: Default::default(),
            all_tags: Default::default(),
            viewer: None,
            parked_tabs: Default::default(),
            current_tab: 0,
            next_tab: 1,
            new_tab: None,
            preview_pdf: None,
        };
        app.apply_theme(ctx);
        app
    }

    #[allow(dead_code)]
    pub fn dock_tree(&mut self) -> &mut Option<egui_tiles::Tree<Pane>> {
        &mut self.dock
    }

    pub fn folder(&self) -> &Path {
        self.history.current()
    }

    pub fn go(&mut self, folder: PathBuf) {
        if folder.is_dir() {
            self.history.visit(folder);
            self.after_navigation();
        }
    }

    fn after_navigation(&mut self) {
        // leaving a tag view: going to a folder shows the folder
        self.tag_filter = None;
        self.viewer = None;
        self.selected.clear();
        self.search.clear();
        self.path_text = None;
        self.focus_view = true;
    }

    /// The markdown file to export: the one selected, else the one open in the central view
    pub fn markdown_target(&self) -> Option<PathBuf> {
        let is_md = |p: &Path| p.extension().is_some_and(|e| matches!(e.to_string_lossy().to_lowercase().as_str(), "md" | "markdown"));
        if self.selected.len() == 1 {
            if let Some(p) = self.selected_paths().into_iter().next().filter(|p| is_md(p)) {
                return Some(p);
            }
        }
        self.viewer.as_ref().map(|v| v.path.clone()).filter(|p| is_md(p))
    }

    /// Opens a folder here, a file with the application of the system
    pub fn open(&mut self, path: &Path) {
        if let Some(store) = self.tags.as_mut() {
            store.opened(path);
        }
        if path.is_dir() && !path.extension().is_some_and(|x| x == "app") {
            self.go(path.to_path_buf());
        } else if let Some(viewer) = Viewer::open(path, &self.core) {
            self.viewer = Some(viewer);
        } else {
            open_with_system(path);
        }
    }

    /// What "Ouvrir avec l'app système" opens: the selected item (a file with its default
    /// application, a folder in the file manager), else the current folder
    pub fn system_target(&self) -> Option<PathBuf> {
        let target = self.viewer_path().map(Path::to_path_buf).or_else(|| self.selected_paths().into_iter().next()).unwrap_or_else(|| self.folder().to_path_buf());
        target.exists().then_some(target)
    }

    pub fn open_in_system(&self) {
        if let Some(path) = self.viewer_path() {
            open_with_system(path);
            return;
        }
        for path in self.selected_paths() {
            open_with_system(&path);
        }
        if self.selected.is_empty() {
            open_with_system(self.folder());
        }
    }

    /// The tint and the opacity of the three glasses are read at each frame by the drawing
    pub fn apply_layers(&self) {
        for (d, look) in self.looks.iter().enumerate() {
            for (i, (layer, _)) in LAYERS.iter().enumerate() {
                let s = look.layers[i];
                style::set_layer(d == 1, *layer, s.tint.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b)), s.opacity);
            }
        }
    }

    /// The accent: the one of the color picker, or one of the four first
    /// The outline of the tabs and panes for a theme: its thickness and its color
    pub fn outline(&self, visuals: &egui::Visuals, active: bool) -> (f32, egui::Color32) {
        let k = usize::from(!active);
        let look = &self.looks[usize::from(visuals.dark_mode)];
        let base = visuals.widgets.noninteractive.bg_stroke.color;
        let color = look.outline[k]
            .map(|[r, g, b, a]| egui::Color32::from_rgba_unmultiplied(r, g, b, a))
            .unwrap_or(if active { base } else { base.gamma_multiply(0.5) });
        (look.outline_width[k], color)
    }

    /// The blur of both looks, kept as it was while the pointer is down (a slider being dragged)
    fn settled_blur(&mut self, ctx: &egui::Context) -> [u32; 2] {
        let wanted = [self.looks[0].blur, self.looks[1].blur];
        let dragging = ctx.input(|i| i.pointer.any_down());
        match self.settled_blur {
            Some(old) if dragging => old,
            _ => {
                self.settled_blur = Some(wanted);
                wanted
            }
        }
    }

    pub fn accent_color(&self) -> egui::Color32 {
        self.accent_custom.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b)).unwrap_or(ACCENTS[self.accent].1)
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
        ctx.set_theme(match self.theme {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        });
        self.apply_layers();
        let accent = self.accent_color();
        // the opacity of each glass is its own setting, whatever is behind
        let mode = Some(style::GlassMode::Full);
        // the clear glass of the content and of the docks, like examples/finder.py
        let levels = style::GlassLevels { scale: 1.2, central: Some(0.3), dock: Some(0.3), ..style::GlassLevels::DEFAULT };
        if let Ok(mut state) = style::THEME.lock() {
            *state = style::ThemeState { accent: Some(accent), glass: mode, levels, tint: None };
        }
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let mut s = style::default_style(theme);
            let look = &self.looks[usize::from(theme == egui::Theme::Dark)];
            match mode {
                Some(mode) => style::modern_glass_style(&mut s, None, accent, mode, levels),
                None => style::accent_style(&mut s, accent),
            }
            // the text of the widgets, not an override of every text: what is selected keeps its own color
            if let Some([r, g, b]) = look.text {
                let c = egui::Color32::from_rgb(r, g, b);
                s.visuals.widgets.noninteractive.fg_stroke.color = c;
                s.visuals.widgets.inactive.fg_stroke.color = c;
            }
            ctx.set_style_of(theme, s);
        }
    }

    /// The items the views show: made from their paths, kept while they and the listings stay the
    /// same. What was asked to select (a new folder, a pasted file) is selected once it is shown
    pub fn set_items(&mut self, ctx: &egui::Context, shown: Vec<PathBuf>) {
        let core = self.core.clone();
        let Some(items) = self.views.items(ctx, &core, shown) else { return };
        self.items = items;
        if !self.select_after.is_empty() {
            let wanted: HashSet<PathBuf> = self.select_after.iter().cloned().collect();
            let found: Vec<PathBuf> = self.items.iter().filter(|e| wanted.contains(&e.path)).map(|e| e.path.clone()).collect();
            if let Some(first) = found.first().cloned() {
                self.select_after.clear();
                self.selected = found.into_iter().collect();
                self.views.scroll_to(&first);
            }
        }
    }

    pub fn set_tag_filter(&mut self, tag: Option<i64>) {
        self.tag_filter = tag;
        self.selected.clear();
    }

    /// Goes to the folder of an item and selects it
    pub fn reveal_path(&mut self, path: &Path) {
        if path.is_dir() {
            self.set_tag_filter(None);
            self.go(path.to_path_buf());
            return;
        }
        if let Some(parent) = path.parent() {
            self.set_tag_filter(None);
            if parent != self.folder() {
                self.go(parent.to_path_buf());
            }
            self.select_after = vec![path.to_path_buf()];
        }
    }

    /// The tags of an item to draw: the color, the name and where the tag comes from. Kept until the store changes
    pub fn row_tags(&self, path: &Path) -> std::rc::Rc<Vec<(Color32, String, How)>> {
        let Some(store) = &self.tags else { return Default::default() };
        let mut cache = self.tag_cache.borrow_mut();
        if cache.0 != store.version {
            *cache = (store.version, Default::default());
        }
        if let Some(found) = cache.1.get(path) {
            return found.clone();
        }
        let effective = store.effective(path);
        let mut known = self.all_tags.borrow_mut();
        // the version is kept plus one: the first value is 0, "never read"
        if known.0 != store.version + 1 {
            *known = (store.version + 1, store.all());
        }
        let all = &known.1;
        let mut tags: Vec<(Color32, String, How)> = all
            .iter()
            .filter_map(|t| effective.get(&t.id).map(|how| (tagsui::parse_color(&t.color), t.name.clone(), *how)))
            .collect();
        tags.sort_by_key(|(_, _, how)| match how { How::Own => 0, How::Folder => 1, How::Parent => 2 });
        let value = std::rc::Rc::new(tags);
        cache.1.insert(path.to_path_buf(), value.clone());
        value
    }

    /// Every tag of the store as a path, with the colors of its levels (read once per version of the store)
    pub fn library(&self) -> std::rc::Rc<tagsui::Library> {
        let Some(store) = &self.tags else { return Default::default() };
        let mut cache = self.library_cache.borrow_mut();
        // the version is kept plus one: the first value is 0, "never read"
        if cache.0 != store.version + 1 {
            *cache = (store.version + 1, std::rc::Rc::new(tagsui::Library::read(&store.all())));
        }
        cache.1.clone()
    }

    /// The tags of an item as paths, its own then those of its folders (the parents of tags are left out)
    pub fn row_tag_paths(&self, path: &Path) -> std::rc::Rc<tagsui::RowTags> {
        let Some(store) = &self.tags else { return Default::default() };
        {
            let mut cache = self.tag_paths_cache.borrow_mut();
            if cache.0 != store.version {
                *cache = (store.version, Default::default());
            }
            if let Some(found) = cache.1.get(path) {
                return found.clone();
            }
        }
        let library = self.library();
        let effective = store.effective(path);
        let mut found: Vec<(&i64, &How)> = effective.iter().filter(|(_, how)| **how != How::Parent).collect();
        found.sort_by_key(|(id, how)| (**how != How::Own, library.order(**id)));
        let row = tagsui::RowTags {
            paths: found.iter().filter_map(|(id, _)| library.path(**id)).collect(),
            inherited: found.iter().filter(|(_, how)| **how == How::Folder).filter_map(|(id, _)| library.path(**id)).collect(),
        };
        let value = std::rc::Rc::new(row);
        self.tag_paths_cache.borrow_mut().1.insert(path.to_path_buf(), value.clone());
        value
    }

    pub fn opened_at(&self, path: &Path) -> Option<std::time::SystemTime> {
        let store = self.tags.as_ref()?;
        let mut cache = self.opened_cache.borrow_mut();
        // reloaded when the store changed (not when it is empty: an empty one would be read at every row of every frame)
        if cache.0 != store.version || !self.opened_loaded.get() {
            *cache = (store.version, store.opened_dates());
            self.opened_loaded.set(true);
        }
        cache.1.get(&crate::tagstore::key(path)).map(|t| std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(*t))
    }

    /// The file shown in the central viewer
    pub fn viewer_path(&self) -> Option<&Path> {
        self.viewer.as_ref().map(|v| v.path.as_path())
    }

    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.items.iter().filter(|e| self.selected.contains(&e.path)).map(|e| e.path.clone()).collect()
    }

    pub fn select_all(&mut self) {
        self.selected = self.items.iter().map(|e| e.path.clone()).collect();
    }

    pub fn back(&mut self) {
        self.history.back();
        self.after_navigation();
    }

    pub fn forward(&mut self) {
        self.history.forward();
        self.after_navigation();
    }

    pub fn parent(&mut self) {
        if let Some(p) = self.folder().parent().map(Path::to_path_buf) {
            self.go(p);
        }
    }

    pub fn toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
    }

    pub fn toggle_terminal(&mut self) {
        self.show_terminal = !self.show_terminal;
    }

    pub fn toggle_dock(&mut self) {
        self.show_dock = !self.show_dock;
    }

    pub fn new_folder(&mut self) {
        let path = actions::unique_folder(self.folder());
        match std::fs::create_dir(&path) {
            Ok(()) => {
                self.select_after = vec![path.clone()];
                self.start_rename(path);
            }
            Err(e) => self.notice = Some(format!("Impossible de créer le dossier: {e}")),
        }
    }

    /// A new folder in the current folder holding the selected items, then its renaming starts
    pub fn group_selection(&mut self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let folder = actions::unique_named(self.folder(), "dossier avec éléments");
        if let Err(e) = std::fs::create_dir(&folder) {
            self.notice = Some(format!("Impossible de créer le dossier: {e}"));
            return;
        }
        let mut failed = Vec::new();
        for path in &paths {
            let Some(name) = path.file_name() else { continue };
            if let Err(e) = std::fs::rename(path, folder.join(name)) {
                failed.push(format!("{}: {e}", name.to_string_lossy()));
            }
        }
        if !failed.is_empty() {
            self.notice = Some(format!("Déplacement impossible: {}", failed.join(", ")));
        }
        self.select_after = vec![folder.clone()];
        self.start_rename(folder);
    }

    pub fn start_rename(&mut self, path: PathBuf) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.rename = Some((path, name));
    }

    pub fn ask_delete(&mut self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        if cfg!(target_os = "macos") {
            self.delete(paths);
        } else {
            self.confirm_delete = Some(paths);
        }
    }

    fn delete(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            if let Err(e) = actions::trash(&path) {
                self.notice = Some(format!("{}: {e}", path.display()));
            }
        }
        self.selected.clear();
    }

    /// Copy or cut of the selection: in the clipboard of the system (the files can be pasted in the
    /// Finder or the Explorer), and kept here too, for the systems that have none for files
    pub fn copy_files(&mut self, cut: bool) {
        let paths = self.selected_paths();
        pyegui_core::clipboard::write_files(&paths, cut);
        self.clipboard = Some((paths, cut));
    }

    /// Paste: the files of the clipboard of the system (copied here or by another program), else the
    /// list kept here. A cut moves them
    pub fn paste(&mut self) {
        let (system, system_cut) = pyegui_core::clipboard::read_files();
        let (paths, cut) = if system.is_empty() {
            let Some(held) = self.clipboard.clone() else { return };
            held
        } else {
            (system, system_cut)
        };
        let folder = self.folder().to_path_buf();
        self.transfer.start(paths, folder, cut);
        if cut {
            self.pasted_cut = true;
        }
    }

    /// The end of the transfers: the new items are selected, the failures shown
    fn poll_transfer(&mut self) {
        let done = self.transfer.finished();
        if !done.is_empty() {
            if self.pasted_cut && self.transfer.running() == 0 {
                // the moved files are now at their new place: the clipboard keeps them as a copy
                let moved: Vec<PathBuf> = done.iter().map(|(_, target)| target.clone()).collect();
                pyegui_core::clipboard::write_files(&moved, false);
            }
            self.select_after = done.into_iter().filter(|(folder, _)| folder == self.folder()).map(|(_, target)| target).collect();
            if self.pasted_cut && self.transfer.running() == 0 {
                self.clipboard = None;
                self.pasted_cut = false;
            }
        }
        if let Some(error) = self.transfer.errors().into_iter().next() {
            self.notice = Some(error);
        }
    }

    /// Files dropped on the window are copied or moved into the folder shown, by the rules of the file
    /// managers (same disk: move, Option/Ctrl copies; two disks: copy, Cmd/Shift moves)
    fn drops(&mut self, ctx: &egui::Context) {
        let hovered = pyegui_core::native_dnd::hovered_files(ctx);
        let dropped = pyegui_core::native_dnd::dropped_files(ctx);
        // the keys held now, from the system: the window gets no key event while files are dragged
        let held = pyegui_core::native_dnd::held_modifiers().unwrap_or_else(|| {
            let m = ctx.input(|i| i.modifiers);
            (m.shift, m.ctrl, m.alt, m.command)
        });
        // a folder of the view under the pointer takes the files, else the folder shown
        let folder = self.views.drop_target(self.view).unwrap_or_else(|| self.folder().to_path_buf());
        if !hovered.is_empty() {
            let operation = pyegui_core::transfer::drop_operation_for(held, &hovered, &folder);
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop-hint")));
            let screen = ctx.content_rect();
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(110));
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                format!("Déposer pour {} dans « {} »", if operation == "move" { "déplacer" } else { "copier" }, folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                egui::FontId::proportional(20.0),
                Color32::WHITE,
            );
        }
        if !dropped.is_empty() {
            let operation = pyegui_core::transfer::drop_operation_for(held, &dropped, &folder);
            // an item already in this folder is not moved onto itself
            let sources: Vec<PathBuf> = dropped.into_iter().filter(|p| !(operation == "move" && p.parent() == Some(folder.as_path()))).collect();
            self.transfer.start(sources, folder, operation == "move");
        }
    }

    fn poll_activity(&mut self) {
        for e in lock(&self.core).take_events() {
            if e.kind.starts_with("Access") {
                continue;
            }
            let names = e.paths.iter().map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).collect::<Vec<_>>().join(", ");
            self.activity.push_back(format!("{}  {}  {}", chrono::Local::now().format("%H:%M:%S"), e.kind, names));
            if self.activity.len() > 300 {
                self.activity.pop_front();
            }
        }
    }

    fn modals(&mut self, ctx: &egui::Context) {
        tagsui::modal(self, ctx);
        if let Some((path, mut name)) = self.rename.take() {
            let mut keep = true;
            let mut apply = false;
            egui::Modal::new(egui::Id::new("rename")).show(ctx, |ui| {
                ui.set_width(320.0);
                ui.heading("Renommer");
                let edit = ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
                if !edit.has_focus() && !edit.lost_focus() {
                    edit.request_focus();
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Annuler").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        keep = false;
                    }
                    if ui.button("Renommer").clicked() || (edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
                        apply = true;
                    }
                });
            });
            if apply {
                let target = path.with_file_name(name.trim());
                if name.trim().is_empty() || name.contains(['/', '\\']) {
                    self.notice = Some("Nom invalide".into());
                } else if target != path {
                    match std::fs::rename(&path, &target) {
                        Ok(()) => self.select_after = vec![target],
                        Err(e) => self.notice = Some(format!("Impossible de renommer: {e}")),
                    }
                }
            } else if keep {
                self.rename = Some((path, name));
            }
        }
        if let Some(paths) = self.confirm_delete.take() {
            let mut keep = true;
            let mut yes = false;
            egui::Modal::new(egui::Id::new("delete")).show(ctx, |ui| {
                ui.set_width(340.0);
                ui.heading("Supprimer définitivement ?");
                ui.label(format!("{} élément(s) seront supprimés, sans corbeille.", paths.len()));
                ui.horizontal(|ui| {
                    if ui.button("Annuler").clicked() {
                        keep = false;
                    }
                    if ui.button(RichText::new("Supprimer").color(Color32::from_rgb(220, 60, 60))).clicked() {
                        yes = true;
                    }
                });
            });
            if yes {
                self.delete(paths);
            } else if keep {
                self.confirm_delete = Some(paths);
            }
        }
        self.palette_modal(ctx);
        if let Some(text) = self.notice.clone() {
            let mut close = false;
            egui::Modal::new(egui::Id::new("notice")).show(ctx, |ui| {
                ui.set_width(340.0);
                ui.label(text);
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
            if close {
                self.notice = None;
            }
        }
    }

    /// The palette: a search field over the enabled actions (case and accents ignored), the arrows
    /// choose, Enter runs on the current selection, Escape closes
    fn palette_modal(&mut self, ctx: &egui::Context) {
        let Some((mut query, mut index)) = self.palette.take() else { return };
        let needle = crate::search::fold(query.trim());
        let shown: Vec<usize> = actions::ACTIONS.iter().enumerate().filter(|(_, a)| (a.enabled)(self) && crate::search::fold(a.label).contains(&needle)).map(|(i, _)| i).collect();
        let (down, up, enter, escape) = ctx.input_mut(|i| {
            (i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::Enter), i.consume_key(Modifiers::NONE, Key::Escape))
        });
        index = index.min(shown.len().saturating_sub(1));
        if down {
            index = (index + 1).min(shown.len().saturating_sub(1));
        }
        if up {
            index = index.saturating_sub(1);
        }
        let mut chosen = if enter { shown.get(index).copied() } else { None };
        let mut close = escape;
        let shortcuts: Vec<String> = shown.iter().map(|&i| actions::ACTIONS[i].shortcut.as_ref().map(|s| ctx.format_shortcut(s)).unwrap_or_default()).collect();
        let response = egui::Modal::new(egui::Id::new("palette")).show(ctx, |ui| {
            ui.set_width(460.0);
            let edit = ui.add(egui::TextEdit::singleline(&mut query).hint_text("Rechercher une action…").desired_width(f32::INFINITY));
            if !edit.has_focus() {
                edit.request_focus();
            }
            if edit.changed() {
                index = 0;
            }
            ui.add_space(4.0);
            if shown.is_empty() {
                ui.weak("Aucune action");
            }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for (row, &i) in shown.iter().enumerate() {
                    let action = &actions::ACTIONS[i];
                    let (rect, r) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::click());
                    if row == index {
                        ui.painter().rect_filled(rect, 6.0, ui.visuals().selection.bg_fill.gamma_multiply(0.45));
                        if down || up {
                            ui.scroll_to_rect(rect, None);
                        }
                    } else if r.hovered() {
                        ui.painter().rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(127, 127, 127, 30));
                    }
                    let color = ui.visuals().text_color();
                    glyphs::image(glyphs::by_name(action.icon), 16.0, color).paint_at(ui, egui::Rect::from_center_size(rect.left_center() + egui::vec2(18.0, 0.0), egui::vec2(16.0, 16.0)));
                    ui.painter().text(rect.left_center() + egui::vec2(34.0, 0.0), egui::Align2::LEFT_CENTER, action.label, egui::FontId::proportional(14.0), color);
                    ui.painter().text(rect.right_center() - egui::vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, &shortcuts[row], egui::FontId::proportional(12.0), ui.visuals().weak_text_color());
                    if r.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                        index = row;
                    }
                    if r.clicked() {
                        chosen = Some(i);
                    }
                }
            });
        });
        if response.should_close() {
            close = true;
        }
        if let Some(i) = chosen {
            (actions::ACTIONS[i].run)(self, ctx);
        } else if !close {
            self.palette = Some((query, index));
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        if self.palette.is_none() && self.rename.is_none() && self.confirm_delete.is_none() && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::P)) {
            self.palette = Some((String::new(), 0));
            return;
        }
        if ctx.text_edit_focused() || pyegui_core::terminal::terminal_focused(ctx) || self.rename.is_some() || self.confirm_delete.is_some() || self.palette.is_some() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space)) {
            self.palette = Some((String::new(), 0));
            return;
        }
        actions::run_shortcuts(self, ctx);
        // the arrows, Enter and the letters are read by the views
        let (parent, clear) = ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::Backspace), i.consume_key(Modifiers::NONE, Key::Escape)));
        if parent {
            self.parent();
        }
        if clear {
            self.selected.clear();
            self.search.clear();
        }
    }

    pub fn dock_shown(&self) -> bool {
        self.show_dock
    }

    pub fn terminals_shown(&self) -> bool {
        self.show_terminal
    }

    /// A terminal pane of the dock
    pub fn terminal_pane(&mut self, ui: &mut egui::Ui, shell: bool) {
        let folder = self.folder().to_path_buf();
        let opacity = self.terminal_opacity;
        if shell {
            self.terminals.shell_ui(ui, &folder, opacity);
        } else {
            self.terminals.python_ui(ui, opacity);
        }
    }

    /// The central pane of the dock, with no title: the file open in the viewer, else the folder
    pub fn central_pane(&mut self, ui: &mut egui::Ui) {
        if let Some(mut viewer) = self.viewer.take() {
            if !viewer::show(&mut viewer, ui, &self.core) {
                self.viewer = Some(viewer);
            }
            return;
        }
        self.tag_header(ui);
        views::show(self, ui);
    }

    /// The toggle of the sidebar, just on the right of the window buttons
    /// The buttons of the window (close, minimize, maximize), drawn like macOS at the left of the top band
    /// when the system has no title bar
    fn window_buttons(&self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        pyegui_core::titlebar::window_controls(ui, 12.0, 8.0);
        ui.add_space(14.0);
    }

    fn sidebar_button(&mut self, ui: &mut egui::Ui) {
        let glyph = glyphs::by_name(if self.show_sidebar { "sidebar-open" } else { "sidebar-closed" });
        if glyphs::button(ui, glyph, 18.0, false, "Afficher / masquer la sidebar (⌘B)").clicked() {
            self.show_sidebar = !self.show_sidebar;
        }
    }

    /// The top band, like the toolbar of examples/finder.py: the navigation and the name of the
    /// folder at the left, the free space moves the window, the view, the search, the terminal and
    /// the panes at the right (each group in a segmented control)
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let outer = ui.max_rect();
        let band = egui::Rect::from_min_size(outer.min, egui::vec2(outer.width(), TOP_H));
        // under the buttons: the free space of the band moves the window
        drag_window(ui, band);
        let row = band.width();
        let search_width = (row - TOOLBAR_FIXED).min(SEARCH_WIDTH);
        let show_search = search_width >= SEARCH_MIN;
        let show_view = row >= TOOLBAR_FIXED - 120.0;
        // what the right part takes, to keep the name of the folder out of it
        let right_part = 2.0 * 34.0 + 12.0 + if show_search { search_width + 8.0 } else { 0.0 } + if show_view { 3.0 * 32.0 + 8.0 } else { 0.0 };
        let height = band.height();
        ui.spacing_mut().interact_size.y = glyphs::CONTROL_H;
        ui.allocate_ui_with_layout(egui::vec2(row, height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            if MAC_TITLEBAR && !self.show_sidebar {
                ui.add_space(76.0); // the window buttons are over the toolbar when the sidebar is hidden
            } else if self.custom_titlebar && !self.show_sidebar {
                self.window_buttons(ui); // the sidebar is hidden: the buttons are at the left of the toolbar
            }
            if !self.show_sidebar {
                self.sidebar_button(ui);
                ui.add_space(6.0);
            }
            let parent = self.folder().parent().map(Path::to_path_buf);
            let reading = self.viewer.is_some();
            let nav = glyphs::segmented(ui, "nav", 16.0, &[
                glyphs::Cell { glyph: glyphs::CHEVRON_LEFT, on: false, enabled: self.history.can_back(), tip: "Précédent (⌘[)" },
                glyphs::Cell { glyph: glyphs::CHEVRON_RIGHT, on: false, enabled: self.history.can_forward(), tip: "Suivant (⌘])" },
                if reading {
                    // in the viewer the button that goes up a folder closes the file
                    glyphs::Cell { glyph: glyphs::CLOSE, on: false, enabled: true, tip: "Fermer (Échap)" }
                } else {
                    glyphs::Cell { glyph: glyphs::CHEVRON_UP, on: false, enabled: parent.is_some(), tip: "Dossier parent (⌘↑)" }
                },
            ]);
            match nav.clicked {
                Some(0) => self.back(),
                Some(1) => self.forward(),
                Some(2) if reading => self.viewer = None,
                Some(2) => {
                    if let Some(p) = parent {
                        self.go(p);
                    }
                }
                _ => {}
            }
            ui.add_space(8.0);
            let system = glyphs::segmented(ui, "system", 16.0, &[
                glyphs::Cell { glyph: glyphs::OPEN, on: false, enabled: self.system_target().is_some(), tip: "Ouvrir avec l'app système (fichier : app par défaut ; dossier : Finder, Explorer…)" },
            ]);
            if system.clicked.is_some() {
                self.open_in_system();
            }
            ui.add_space(8.0);
            let room = (ui.available_width() - right_part).max(60.0);
            self.folder_name(ui, room);
            // the right part, from the edge: the terminal and the panes, the search, the view
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                let toggles = glyphs::segmented(ui, "panes", 18.0, &[
                    glyphs::cell(glyphs::TERMINAL, self.show_terminal, "Terminal (⌘T)"),
                    glyphs::cell(glyphs::by_name(if self.show_dock { "dock-open" } else { "dock-closed" }), false, "Afficher / masquer les volets (⌘J, clic droit : gérer)"),
                ]);
                match toggles.clicked {
                    Some(0) => self.show_terminal = !self.show_terminal,
                    Some(1) => self.show_dock = !self.show_dock,
                    _ => {}
                }
                toggles.responses[1].clone().context_menu(|ui| {
                    if ui.checkbox(&mut self.show_dock, "Afficher / Masquer").clicked() {
                        ui.close();
                    }
                    ui.separator();
                    if let Some(tree) = self.dock.as_mut() {
                        dock::menu(tree, ui);
                    }
                });
                ui.add_space(8.0);
                if show_search {
                    ui.add(egui::TextEdit::singleline(&mut self.search).id(egui::Id::new("search-field")).hint_text("Rechercher").vertical_align(egui::Align::Center).desired_width(search_width));
                    ui.add_space(8.0);
                }
                if show_view {
                    let views = glyphs::segmented(ui, "view", 16.0, &[
                        glyphs::cell(glyphs::TREE, self.view == ViewMode::Tree, "Arbre (⌘1)"),
                        glyphs::cell(glyphs::LIST, self.view == ViewMode::List, "Liste (⌘2)"),
                        glyphs::cell(glyphs::GRID, self.view == ViewMode::Icons, "Icônes (⌘3)"),
                    ]);
                    match views.clicked {
                        Some(0) => self.view = ViewMode::Tree,
                        Some(1) => self.view = ViewMode::List,
                        Some(2) => self.view = ViewMode::Icons,
                        _ => {}
                    }
                }
            });
        });
    }

    /// The name of the folder with its icon, in a combo: a click opens the list of its parent folders
    fn folder_name(&mut self, ui: &mut egui::Ui, room: f32) {
        // the name of the folder, or of the file in the viewer
        let folder = self.viewer_path().map(Path::to_path_buf).unwrap_or_else(|| self.folder().to_path_buf());
        let options = ComboOptions { size: 15.0, max_width: Some(room), always_icon: true, ..Default::default() };
        let out = combo(ui, &folder, &options);
        for (path, response) in out.hits {
            response.context_menu(|ui| self.path_menu(ui, &path));
        }
        if let Some(path) = out.clicked {
            self.go(PathBuf::from(path));
        }
    }

    /// The menu of a folder of the path bar or of the title
    fn path_menu(&mut self, ui: &mut egui::Ui, path: &str) {
        let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string());
        ui.label(RichText::new(name).strong());
        ui.separator();
        if ui.button("Ouvrir").clicked() {
            self.reveal_path(Path::new(path));
            ui.close();
        }
        if ui.button("Copier le chemin").clicked() {
            ui.ctx().copy_text(path.to_string());
            ui.close();
        }
    }

    /// The bottom band: the path at the left, at the right the count, the theme and the look
    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        let row = ui.max_rect();
        let height = BOTTOM_H;
        ui.spacing_mut().interact_size.y = glyphs::CONTROL_H;
        let right_width = 380.0_f32.min(row.width() * 0.5);
        ui.allocate_ui_with_layout(egui::vec2(row.width(), height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let left = egui::vec2((row.width() - right_width).max(60.0), height);
            ui.allocate_ui_with_layout(left, egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                self.path_bar(ui);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.theme_control(ui);
                ui.add_space(4.0);
                self.status_bar(ui);
            });
        });
    }

    /// Light, dark or the system theme (a segmented control), and the menu of the look
    fn theme_control(&mut self, ui: &mut egui::Ui) {
        let before = self.theme;
        let (response, _) = egui::containers::menu::MenuButton::from_button(glyphs::button_of(ui, glyphs::GEAR, 14.0, false))
            // a click in the color popup of a glass must not close the menu that holds it
            .config(egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::IgnoreClicks))
            .ui(ui, |ui| {
                self.appearance_menu(ui);
                close_on_click_outside(ui);
            });
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("gear-rect"), response.rect));
        response.on_hover_text("Apparence");
        ui.add_space(4.0);
        let choice = glyphs::segmented(ui, "theme", 14.0, &[
            glyphs::cell(glyphs::SUN, self.theme == ThemeChoice::Light, "Clair"),
            glyphs::cell(glyphs::MOON, self.theme == ThemeChoice::Dark, "Sombre"),
            glyphs::cell(glyphs::THEME_SYSTEM, self.theme == ThemeChoice::System, "Thème du système"),
        ]);
        match choice.clicked {
            Some(0) => self.theme = ThemeChoice::Light,
            Some(1) => self.theme = ThemeChoice::Dark,
            Some(2) => self.theme = ThemeChoice::System,
            _ => {}
        }
        if before != self.theme {
            self.apply_theme(ui.ctx());
        }
    }

    pub fn appearance_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(180.0);
        ui.label(RichText::new("Thème").strong());
        let before = (self.theme, self.accent);
        let mut changed_accent = false;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.theme, ThemeChoice::System, "Système");
            ui.selectable_value(&mut self.theme, ThemeChoice::Light, "Clair");
            ui.selectable_value(&mut self.theme, ThemeChoice::Dark, "Sombre");
        });
        ui.label(RichText::new("Accent").strong());
        ui.horizontal(|ui| {
            for (i, (name, color)) in ACCENTS.iter().enumerate() {
                let on = self.accent_custom.is_none() && self.accent == i;
                let text = RichText::new(if on { "●" } else { "○" }).color(*color).size(18.0);
                if ui.add(egui::Button::new(text).frame(false)).on_hover_text(*name).clicked() {
                    self.accent = i;
                    self.accent_custom = None;
                    changed_accent = true;
                }
            }
            let mut color = self.accent_color();
            if egui::color_picker::color_edit_button_srgba(ui, &mut color, egui::color_picker::Alpha::Opaque).on_hover_text("Autre couleur").changed() {
                self.accent_custom = Some([color.r(), color.g(), color.b()]);
                changed_accent = true;
            }
        });
        if before != (self.theme, self.accent) || changed_accent {
            self.apply_theme(ui.ctx());
        }
        // the settings below are those of the theme shown: the light and the dark ones are kept apart
        let dark = ui.visuals().dark_mode;
        let d = dark as usize;
        ui.separator();
        ui.label(RichText::new(if dark { "Thème sombre" } else { "Thème clair" }).strong());
        ui.label(RichText::new("Fond").strong());
        let mut look = self.looks[d].clone();
        ui.horizontal(|ui| {
            if backdrop_name().is_some() {
                ui.selectable_value(&mut look.background, Background::Glass, "Vitre");
            }
            ui.selectable_value(&mut look.background, Background::Image, "Image");
            ui.selectable_value(&mut look.background, Background::Color, "Couleur");
        });
        if look.background != Background::Color {
            // the blur of the glass of the system is only set on macOS
            let unavailable = look.background == Background::Glass && !cfg!(target_os = "macos");
            let slider = ui.add_enabled(!unavailable, egui::Slider::new(&mut look.blur, 0..=40).text("Flou"));
            if unavailable {
                slider.on_disabled_hover_text("Non disponible");
            }
        }
        match look.background {
            Background::Image => {
                let mut path = look.image.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut path).hint_text("Chemin de l'image").desired_width(f32::INFINITY)).changed() {
                    look.image = if path.trim().is_empty() { None } else { Some(expand(Path::new(path.trim()))) };
                }
                let selection = self.selected.iter().next().filter(|p| is_image(p)).cloned();
                if ui.add_enabled(selection.is_some(), egui::Button::new("Utiliser la sélection")).clicked() {
                    look.image = selection;
                }
            }
            Background::Color => {
                let (_, default) = (0, if dark { egui::Color32::from_rgb(22, 24, 30) } else { egui::Color32::from_rgb(235, 235, 240) });
                let mut color = look.color.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b)).unwrap_or(default);
                ui.horizontal(|ui| {
                    if egui::color_picker::color_edit_button_srgba(ui, &mut color, egui::color_picker::Alpha::Opaque).changed() {
                        look.color = Some([color.r(), color.g(), color.b()]);
                    }
                    ui.label("Couleur du fond");
                });
            }
            Background::Glass => {}
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Vitres").strong());
        for (i, (layer, name)) in LAYERS.iter().enumerate() {
            let (tint, opacity) = style::layer_values(*layer, dark);
            let (mut color, mut alpha) = (tint, opacity);
            ui.horizontal(|ui| {
                if egui::color_picker::color_edit_button_srgba(ui, &mut color, egui::color_picker::Alpha::Opaque).changed() {
                    look.layers[i].tint = Some([color.r(), color.g(), color.b()]);
                }
                ui.label(*name);
            });
            if ui.add(egui::Slider::new(&mut alpha, 0.0..=1.0).text("Opacité")).changed() {
                look.layers[i].opacity = Some(alpha);
            }
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Contours et texte").strong());
        for (k, name) in ["actifs", "inactifs"].into_iter().enumerate() {
            ui.add(egui::Slider::new(&mut look.outline_width[k], 0.0..=6.0).text(format!("Épaisseur {name}")));
            let (_, current) = self.outline(&ui.visuals().clone(), k == 0);
            let mut color = look.outline[k].map(|[r, g, b, a]| egui::Color32::from_rgba_unmultiplied(r, g, b, a)).unwrap_or(current);
            ui.horizontal(|ui| {
                if egui::color_picker::color_edit_button_srgba(ui, &mut color, egui::color_picker::Alpha::OnlyBlend).changed() {
                    let [r, g, b, a] = color.to_srgba_unmultiplied();
                    look.outline[k] = Some([r, g, b, a]);
                }
                ui.label(format!("Contour {name}"));
            });
        }
        let mut text = look.text.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b)).unwrap_or(ui.visuals().text_color());
        ui.horizontal(|ui| {
            if egui::color_picker::color_edit_button_srgba(ui, &mut text, egui::color_picker::Alpha::Opaque).changed() {
                look.text = Some([text.r(), text.g(), text.b()]);
            }
            ui.label("Couleur du texte");
        });
        if ui.button("Valeurs par défaut").on_hover_text("Remet le fond, les vitres, les contours et le texte de ce thème").clicked() {
            look = Look::default();
        }
        if look != self.looks[d] {
            self.looks[d] = look;
            self.apply_layers();
            self.apply_theme(ui.ctx());
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Terminal").strong());
        ui.add(egui::Slider::new(&mut self.terminal_opacity, 0.0..=1.0).text("Opacité"));
        ui.checkbox(&mut self.show_terminal, "Afficher le terminal (⌘T)");
        if cfg!(windows) && ui.checkbox(&mut self.blue_folders, "Dossiers bleus").changed() {
            pyegui_core::icons::set_blue(self.blue_folders);
        }
    }

    /// The path, with the breadcrumb widget of pyegui: the folders down to the selected item, the
    /// parents shortened when room is missing; "…" turns it into a field to type a path
    fn path_bar(&mut self, ui: &mut egui::Ui) {
        if let Some(mut text) = self.path_text.take() {
            let id = egui::Id::new("path-field");
            let fresh = !ui.memory(|m| m.has_focus(id));
            let mut output = egui::TextEdit::singleline(&mut text).id(id).desired_width(ui.available_width().max(100.0)).show(ui);
            if fresh {
                // the whole path is selected: typing replaces it
                output.response.request_focus();
                output.state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(text.chars().count()),
                )));
                output.state.store(ui.ctx(), id);
            }
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            if enter && (output.response.has_focus() || output.response.lost_focus()) {
                self.go(expand(Path::new(text.trim())));
            } else if ui.input(|i| i.key_pressed(Key::Escape)) || (output.response.lost_focus() && !fresh) {
                // abandoned
            } else {
                self.path_text = Some(text);
            }
            return;
        }
        let here = match self.selected_paths().as_slice() {
            [one] => one.clone(),
            _ => self.folder().to_path_buf(),
        };
        let options = BreadcrumbOptions { max_width: (ui.available_width() - EDIT_BUTTON).max(40.0), ..Default::default() };
        let out = breadcrumb(ui, &here, &options);
        for (path, response) in out.hits {
            response.context_menu(|ui| self.path_menu(ui, &path));
        }
        if let Some(path) = out.clicked {
            self.reveal_path(Path::new(&path));
        }
        if glyphs::small(ui, glyphs::by_name("rename"), "Saisir un chemin").clicked() {
            self.path_text = Some(self.folder().to_string_lossy().into_owned());
        }
    }

    /// Above a tag view: which tag it is, how many items it holds, and the way back to the folder
    fn tag_header(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.tag_filter else { return };
        let Some(tag) = self.tag_list().into_iter().find(|t| t.id == id) else {
            // the tag was deleted
            self.set_tag_filter(None);
            return;
        };
        let count = self.items.len();
        let mut close = false;
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 5.0, tagsui::parse_color(&tag.color));
            ui.label(RichText::new(&tag.name).strong());
            ui.weak(format!("{count} élément(s) avec cette étiquette ou ses sous-étiquettes"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::image(glyphs::image(glyphs::CLOSE, 12.0, ui.visuals().text_color())).frame(false).min_size(egui::vec2(28.0, 22.0))).on_hover_text("Revenir au dossier").clicked() {
                    close = true;
                }
            });
        });
        ui.separator();
        if close {
            self.set_tag_filter(None);
        }
    }

    /// The tags of the store, read once per version of the store
    pub fn tag_list(&self) -> Vec<crate::tagstore::Tag> {
        let Some(store) = &self.tags else { return Vec::new() };
        let mut known = self.all_tags.borrow_mut();
        if known.0 != store.version + 1 {
            *known = (store.version + 1, store.all());
        }
        known.1.clone()
    }

    /// One row of the sidebar: the icon in the accent color (white when selected) and the name
    fn place_row(&self, ui: &mut egui::Ui, glyph: glyphs::Glyph, name: &str, on: bool) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
        let accent = self.accent_color();
        if on {
            ui.painter().rect_filled(rect, 7.0, ui.visuals().selection.bg_fill);
        } else if response.hovered() {
            ui.painter().rect_filled(rect, 7.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let tint = if on { Color32::WHITE } else { accent };
        glyphs::image(glyph, 16.0, tint).paint_at(ui, egui::Rect::from_center_size(egui::pos2(rect.left() + 16.0, rect.center().y), egui::vec2(16.0, 16.0)));
        let color = if on { Color32::WHITE } else { ui.visuals().text_color() };
        ui.painter().text(egui::pos2(rect.left() + 32.0, rect.center().y), egui::Align2::LEFT_CENTER, name, egui::TextStyle::Body.resolve(ui.style()), color);
        response
    }

    /// The sidebar of examples/finder.py: a top band (the window buttons and the toggle), the favorites,
    /// the locations and the tags in a scroll area, a bottom band with the free space of the drive
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        // the panel takes the width it was given (or dragged to): without this it shrinks to its content
        ui.set_min_width(ui.available_width());
        // the top band, as high as the toolbar: the window buttons are over it, the whole band moves the window
        let outer = ui.max_rect();
        let width = ui.available_width().max(60.0);
        drag_window(ui, egui::Rect::from_min_size(outer.min, egui::vec2(width, TOP_H)));
        ui.horizontal(|ui| {
            ui.set_height(TOP_H);
            if MAC_TITLEBAR {
                ui.add_space(76.0);
            } else if self.custom_titlebar {
                self.window_buttons(ui);
            }
            // open, the toggle is at the right of the sidebar; closed, it is next to the window buttons
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                self.sidebar_button(ui);
            });
        });
        let folder = self.folder().to_path_buf();
        egui::Panel::bottom("sidebar_bottom").show_separator_line(false).exact_size(BOTTOM_H).frame(egui::Frame::NONE).show(ui, |ui| {
            let anchor = folder.ancestors().last().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/"));
            if let Some(free) = free_space(&anchor) {
                ui.horizontal_centered(|ui| {
                    ui.add_space(4.0);
                    ui.add(glyphs::image(glyphs::DRIVE, 16.0, ui.visuals().text_color()));
                    ui.weak(format!("{:.0} Go libres", free as f64 / 1e9));
                });
            }
        });
        ui.style_mut().spacing.scroll.fade.strength = 0.0; // no light band at the edge of the list
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Favoris").weak().small());
            let home = home();
            let mut places: Vec<(glyphs::Glyph, String, PathBuf)> = vec![(glyphs::HOME, home.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Départ".into()), home.clone())];
            for (glyph, label, name) in [
                (glyphs::DESKTOP, "Bureau", "Desktop"),
                (glyphs::DOCUMENT, "Documents", "Documents"),
                (glyphs::DOWNLOAD, "Téléchargements", "Downloads"),
                (glyphs::IMAGE, "Images", "Pictures"),
                (glyphs::FOLDER, "Musique", "Music"),
                (glyphs::FOLDER, "Vidéos", if cfg!(target_os = "macos") { "Movies" } else { "Videos" }),
            ] {
                let path = home.join(name);
                if path.is_dir() {
                    places.push((glyph, label.into(), path));
                }
            }
            if cfg!(target_os = "macos") {
                places.push((glyphs::FOLDER, "Applications".into(), "/Applications".into()));
            }
            for (glyph, label, path) in places {
                if self.place_row(ui, glyph, &label, folder == path).clicked() {
                    self.go(path);
                }
            }
            ui.add_space(12.0);
            ui.label(RichText::new("Emplacements").weak().small());
            let drives: Vec<(String, PathBuf)> = if cfg!(windows) {
                ('A'..='Z').map(|c| (format!("{c}:"), PathBuf::from(format!("{c}:\\")))).filter(|(_, p)| p.is_dir()).collect()
            } else {
                vec![(root_name(), PathBuf::from("/"))]
            };
            for (name, path) in drives {
                if self.place_row(ui, glyphs::DRIVE, &name, folder == path).clicked() {
                    self.go(path);
                }
            }
            ui.add_space(12.0);
            ui.label(RichText::new("Étiquettes").weak().small());
            let tags = self.tag_list();
            if tags.is_empty() {
                ui.weak("Aucune étiquette");
            }
            let mut filter = None;
            let mut fold = None;
            // the tree: a tag folded hides what is under it
            let mut hidden_below: Option<usize> = None;
            for (index, tag) in tags.iter().enumerate() {
                if let Some(depth) = hidden_below {
                    if tag.depth > depth {
                        continue;
                    }
                    hidden_below = None;
                }
                let has_children = tags.get(index + 1).is_some_and(|next| next.depth > tag.depth);
                let folded = self.tag_folded.contains(&tag.id);
                if folded && has_children {
                    hidden_below = Some(tag.depth);
                }
                let on = self.tag_filter == Some(tag.id);
                let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
                if on {
                    ui.painter().rect_filled(rect, 7.0, ui.visuals().selection.bg_fill);
                } else if response.hovered() {
                    ui.painter().rect_filled(rect, 7.0, ui.visuals().widgets.hovered.weak_bg_fill);
                }
                let x = rect.left() + 6.0 + tag.depth as f32 * 14.0;
                let color = if on { Color32::WHITE } else { ui.visuals().text_color() };
                if has_children {
                    // the arrow: its own click folds, the rest of the row shows the tag
                    let arrow = egui::Rect::from_center_size(egui::pos2(x + 6.0, rect.center().y), egui::vec2(16.0, 22.0));
                    let arrow_response = ui.interact(arrow, ui.id().with(("tag-fold", tag.id)), egui::Sense::click());
                    let text = if folded { "▸" } else { "▾" };
                    ui.painter().text(arrow.center(), egui::Align2::CENTER_CENTER, text, egui::TextStyle::Small.resolve(ui.style()), color);
                    if arrow_response.clicked() {
                        fold = Some(tag.id);
                    }
                }
                ui.painter().circle_filled(egui::pos2(x + 20.0, rect.center().y), 5.0, tagsui::parse_color(&tag.color));
                ui.painter().text(egui::pos2(x + 34.0, rect.center().y), egui::Align2::LEFT_CENTER, &tag.name, egui::TextStyle::Body.resolve(ui.style()), color);
                if response.clicked() && fold != Some(tag.id) {
                    filter = Some(tag.id);
                }
            }
            if let Some(id) = fold {
                if !self.tag_folded.remove(&id) {
                    self.tag_folded.insert(id);
                }
            }
            if let Some(id) = filter {
                let next = if self.tag_filter == Some(id) { None } else { Some(id) };
                self.set_tag_filter(next);
            }
        });
    }

    pub fn preview(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        let selection: Vec<&Entry> = self.items.iter().filter(|e| self.selected.contains(&e.path)).collect();
        match selection.as_slice() {
            [] => {
                ui.weak(format!("{} éléments", self.items.len()));
            }
            [one] => {
                let e = (*one).clone();
                if let Some(uri) = icons::icon_uri(ui.ctx(), &e.path.to_string_lossy(), 96.0) {
                    ui.vertical_centered(|ui| ui.add(egui::Image::new(uri).fit_to_exact_size(egui::vec2(96.0, 96.0))));
                }
                ui.label(RichText::new(&e.name).strong());
                ui.weak(model::kind_of(&e));
                ui.separator();
                egui::Grid::new("infos").num_columns(2).show(ui, |ui| {
                    if !e.is_dir {
                        ui.weak("Taille");
                        ui.label(model::human_size(e.size));
                        ui.end_row();
                    }
                    ui.weak("Modifié");
                    ui.label(model::format_date(e.modified));
                    ui.end_row();
                });
                ui.separator();
                if e.path.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")) {
                    if self.preview_pdf.as_ref().map(|(p, _)| p) != Some(&e.path) {
                        let doc = pyegui_core::pdf_view::PdfDocument::open(e.path.to_string_lossy().into_owned(), None, false);
                        self.preview_pdf = Some((e.path.clone(), doc));
                    }
                    if let Some((_, doc)) = self.preview_pdf.as_mut() {
                        let info = doc.info();
                        egui::Grid::new("pdf-infos").num_columns(2).show(ui, |ui| {
                            ui.weak("Pages");
                            ui.label(info.pages.map(|p| p.to_string()).unwrap_or_default());
                            ui.end_row();
                            for (label, value) in [("Titre", &info.title), ("Auteur", &info.author)] {
                                if let Some(v) = value {
                                    ui.weak(label);
                                    ui.label(v);
                                    ui.end_row();
                                }
                            }
                        });
                        ui.add_space(6.0);
                        let options = pyegui_core::pdf_view::ThumbnailOptions { size: (ui.available_width() - 12.0).min(260.0), label: false, ..Default::default() };
                        let _ = pyegui_core::pdf_view::pdf_thumbnail(ui, doc, 1, &options);
                    }
                } else if model::is_image(&e.path) {
                    let uri = file_uri(&e.path);
                    ui.add(egui::Image::new(uri).max_width(ui.available_width()).corner_radius(6.0));
                } else if model::is_text(&e.path) && e.size < 512 * 1024 {
                    if self.preview_text.as_ref().map(|(p, _)| p) != Some(&e.path) {
                        let text = std::fs::read_to_string(&e.path).unwrap_or_default();
                        let text: String = text.chars().take(6000).collect();
                        self.preview_text = Some((e.path.clone(), text));
                    }
                    if let Some((_, text)) = &self.preview_text {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.add(egui::Label::new(RichText::new(text).monospace().small()).wrap());
                        });
                    }
                }
            }
            many => {
                ui.label(RichText::new(format!("{} éléments", many.len())).strong());
                let total: u64 = many.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
                ui.weak(model::human_size(total));
            }
        }
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        let folders = self.items.iter().filter(|e| e.is_dir).count();
        let files = self.items.len() - folders;
        let mut text = format!("{folders} dossiers, {files} fichiers");
        if !self.selected.is_empty() {
            text += &format!(" — {} sélectionné(s)", self.selected.len());
        }
        if let Some(error) = lock(&self.core).error() {
            text += &format!(" — ⚠ surveillance indisponible: {error}");
        }
        ui.weak(text);
        if let Some((paths, cut)) = &self.clipboard {
            ui.weak(format!("{} {}", paths.len(), if *cut { "coupé(s)" } else { "copié(s)" }));
        }
        if self.transfer.running() > 0 {
            ui.weak(format!("{} en cours", self.transfer.running()));
            ui.add(egui::ProgressBar::new(self.transfer.progress() as f32).desired_width(100.0).animate(true));
        }
    }
}

impl eframe::App for FinderApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let saved = Saved {
            folder: Some(self.folder().to_path_buf()),
            view: self.view,
            theme: self.theme,
            accent: self.accent,
            sort_key: self.sort_key,
            ascending: self.ascending,
            show_sidebar: self.show_sidebar,
            show_dock: self.show_dock,
            show_terminal: self.show_terminal,
            terminal_opacity: self.terminal_opacity,
            looks: self.looks.clone(),
            accent_custom: self.accent_custom,
            show_hidden: self.show_hidden,
            blue_folders: self.blue_folders,
            dock: self.dock.as_ref().and_then(|d| serde_json::to_string(d).ok()),
            open_folders: self.open_folders.iter().cloned().collect(),
        };
        if let Ok(json) = serde_json::to_string(&saved) {
            storage.set_string(STATE_KEY, json);
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4] // the background is painted by the app (or is the native glass)
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        pyegui_core::native_dnd::pump_drop_input(ctx, raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        screenshot_for_checks(ui.ctx());
        if std::env::var_os("FINDER_TAB2").is_some() && ui.ctx().cumulative_frame_nr() == 20 {
            self.open_in_new_tab(self.folder().to_path_buf()); // for the checks with two tabs of folder
        }
        pyegui_core::native_dnd::set_window(&*frame);
        // the window buttons centered in the top band
        #[cfg(target_os = "macos")]
        pyegui_core::window::place_window_buttons(&*frame, 20.0, TOP_H as f64 / 2.0, 20.0);
        if self.custom_titlebar && !self.corners_applied {
            self.corners_applied = true;
            let _ = pyegui_core::window::set_rounded_corners(&*frame);
        }
        let dark = ui.visuals().dark_mode as usize;
        let background = self.looks[dark].background;
        if background == Background::Glass && !self.backdrop_applied {
            self.backdrop_applied = true;
            if let Some(name) = backdrop_name() {
                if let Err(err) = pyegui_core::window::apply_backdrop(&*frame, name) {
                    self.notice = Some(format!("Verre : {err}"));
                }
            }
        }
        // the blur of the glass: the one of the window server
        #[cfg(target_os = "macos")]
        {
            let blur = if background == Background::Glass { self.settled_blur(ui.ctx())[dark] } else { 0 };
            if blur != self.applied_blur {
                self.applied_blur = blur;
                let _ = pyegui_core::window::set_window_blur(&*frame, blur);
            }
        }
        self.draw(ui);
        self.other_windows(ui.ctx());
    }
}

impl FinderApp {
    /// Under everything: a plain color or the blurred picture (nothing for the glass of the system)
    fn paint_background(&mut self, ctx: &egui::Context, dark: bool) {
        let look = self.looks[dark as usize].clone();
        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::background());
        match look.background {
            Background::Glass => {}
            Background::Color => {
                let default = if dark { egui::Color32::from_rgb(22, 24, 30) } else { egui::Color32::from_rgb(235, 235, 240) };
                let color = look.color.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b)).unwrap_or(default);
                painter.rect_filled(screen, 0.0, color);
            }
            Background::Image => {
                let Some(path) = look.image.clone() else {
                    painter.rect_filled(screen, 0.0, if dark { egui::Color32::from_rgb(22, 24, 30) } else { egui::Color32::from_rgb(235, 235, 240) });
                    return;
                };
                let blur = self.settled_blur(ctx)[usize::from(dark)];
                if self.bg_texture.as_ref().is_none_or(|(p, b, _)| *p != path || *b != blur) {
                    self.bg_texture = blurred_texture(ctx, &path, blur).map(|t| (path.clone(), blur, t));
                }
                match &self.bg_texture {
                    Some((_, _, texture)) => {
                        // the picture covers the window
                        let size = texture.size_vec2();
                        let scale = (screen.width() / size.x).max(screen.height() / size.y);
                        let shown = size * scale;
                        let uv = egui::Rect::from_min_max(
                            egui::pos2((1.0 - screen.width() / shown.x) / 2.0, (1.0 - screen.height() / shown.y) / 2.0),
                            egui::pos2((1.0 + screen.width() / shown.x) / 2.0, (1.0 + screen.height() / shown.y) / 2.0),
                        );
                        painter.image(texture.id(), screen, uv, egui::Color32::WHITE)
;                     }
                    None => { painter.rect_filled(screen, 0.0, egui::Color32::from_gray(if dark { 22 } else { 235 })); }
                }
            }
        }
    }

    /// Opens the windows asked for and draws them; the closed ones are dropped
    fn other_windows(&mut self, ctx: &egui::Context) {
        if let Some(folder) = self.new_window.take() {
            self.window_serial += 1;
            let id = egui::ViewportId::from_hash_of(("finder-window", self.window_serial));
            let window = self.new_window(ctx, folder);
            self.windows.push((id, Box::new(window)));
        }
        let mut open = std::mem::take(&mut self.windows);
        open.retain_mut(|(id, window)| {
            let builder = egui::ViewportBuilder::default()
                .with_title("Finder")
                .with_inner_size([900.0, 600.0])
                .with_min_inner_size([480.0, 320.0])
                .with_transparent(true);
            let builder = if MAC_TITLEBAR {
                builder.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false)
            } else {
                builder
            };
            let mut keep = true;
            ctx.show_viewport_immediate(*id, builder, |ui, _class| {
                ui.push_id(*id, |ui| {
                    // no native backdrop here: the window is filled
                    let rect = ui.ctx().content_rect();
                    ui.painter().rect_filled(rect, 0.0, style::glass_base(ui.visuals().dark_mode, None));
                    window.draw(ui);
                });
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    keep = false;
                }
            });
            // a window opened from this one
            if let Some(folder) = window.new_window.take() {
                self.new_window = Some(folder);
            }
            keep
        });
        open.append(&mut self.windows);
        self.windows = open;
    }

    /// The window: its panels and its views
    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if std::mem::take(&mut self.focus_view) {
            // a field of search that still has the focus would keep the arrows from the view
            ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("search-field")));
        }
        self.paint_background(&ctx, ui.visuals().dark_mode);
        let prof = std::env::var_os("FINDER_PROF").is_some();
        let t0 = std::time::Instant::now();
        self.poll_transfer();
        self.poll_activity();
        let t_refresh = t0.elapsed();
        // only when it changes: a viewport command asks for another frame, so sending it every frame never lets the app sleep
        let title = self.folder().file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Finder".into());
        if self.title != title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        if self.custom_titlebar {
            pyegui_core::titlebar::edge_resize(&ctx);
        }
        if self.show_sidebar {
            // no top margin: the top band of the sidebar is as high as the toolbar
            let frame = egui::Frame::side_top_panel(ui.style()).fill(style::layer_fill(style::Layer::Sidebar, ui.visuals())).stroke(egui::Stroke::NONE).inner_margin(egui::Margin { left: 8, right: 8, top: 0, bottom: 0 });
            egui::Panel::left("sidebar").show_separator_line(false).default_size(170.0).min_size(130.0).frame(frame).show(ui, |ui| self.sidebar(ui));
        }
        // the top and bottom bands: no vertical margin, so what they hold is centered in their height
        let band = |ui: &egui::Ui| egui::Frame::side_top_panel(ui.style()).fill(bar_tint(ui.visuals())).inner_margin(egui::Margin { left: 8, right: 8, top: 0, bottom: 0 });
        if let Some(folder) = self.new_tab.take() {
            self.make_new_tab(folder);
        }
        // the tab of folder shown is the one of the toolbar
        let shown = self.dock.as_ref().and_then(|tree| tree.active_tiles().into_iter().find_map(|id| match tree.tiles.get(id) {
            Some(egui_tiles::Tile::Pane(Pane::Central(n))) => Some(*n),
            _ => None,
        }));
        if let Some(n) = shown {
            self.swap_tab(n);
        }
        egui::Panel::top("toolbar").show_separator_line(false).exact_size(TOP_H).frame(band(ui).stroke(egui::Stroke::NONE)).show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show_separator_line(false).exact_size(BOTTOM_H).frame(band(ui).stroke(egui::Stroke::NONE)).show(ui, |ui| self.bottom_bar(ui));
        if self.show_terminal {
            let folder = self.folder().to_path_buf();
            self.terminals.follow(&folder);
        }
        let t_panels = t0.elapsed();
        // content_body: the dock of the panes around the central pane (no title)
        let mut central = egui::Frame::central_panel(ui.style()).inner_margin(0);
        central = central.fill(egui::Color32::TRANSPARENT); // the dock lays the glass of the bands itself
        egui::CentralPanel::default().frame(central).show(ui, |ui| {
            if let Some(mut tree) = self.dock.take() {
                dock::show(self, &mut tree, ui);
                self.dock = Some(tree);
            }
        });
        let t_central = t0.elapsed();
        self.drops(&ctx);
        self.keyboard(&ctx);
        self.modals(&ctx);
        if prof {
            thread_local! { static FPS: std::cell::Cell<(std::time::Instant, u32)> = std::cell::Cell::new((std::time::Instant::now(), 0)); }
            FPS.with(|f| {
                let (start, n) = f.get();
                if start.elapsed().as_secs_f32() >= 2.0 {
                    eprintln!("{:.1} frames/s", n as f32 / start.elapsed().as_secs_f32());
                    f.set((std::time::Instant::now(), 0));
                } else {
                    f.set((start, n + 1));
                }
            });
            let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
            let total = ms(t0.elapsed());
            if total > 4.0 {
                eprintln!("frame {total:.1} ms: refresh {:.1}, panels {:.1}, central {:.1}, rest {:.1} ({} items)", ms(t_refresh), ms(t_panels) - ms(t_refresh), ms(t_central) - ms(t_panels), total - ms(t_central), self.items.len());
            }
        }
        if self.transfer.running() > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

/// macOS: the content goes under the title bar, with the buttons of the system over the sidebar
/// The name of the volume of the root of the disk
fn root_name() -> String {
    if cfg!(target_os = "macos") { "Macintosh HD".into() } else { "/".into() }
}

/// The free space of the drive of a folder, read every few seconds (not at every frame)
fn free_space(path: &Path) -> Option<u64> {
    thread_local! {
        static CACHE: std::cell::RefCell<Option<(PathBuf, std::time::Instant, Option<u64>)>> = Default::default();
    }
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((p, when, value)) = c.as_ref() {
            if p == path && when.elapsed().as_secs() < 10 {
                return *value;
            }
        }
        let value = fs4::available_space(path).ok();
        *c = Some((path.to_path_buf(), std::time::Instant::now(), value));
        value
    })
}

/// The heights of the top and bottom bands, in the sidebar and in the body
pub const TOP_H: f32 = 40.0;

pub use pyegui_core::style::bar_tint;
/// the bottom bands are as high as the top ones
const BOTTOM_H: f32 = TOP_H;
/// What the "…" button that ends the path takes, with its spacing
const EDIT_BUTTON: f32 = 40.0;
/// What the toolbar keeps for the navigation, the name, the view and the panes
const TOOLBAR_FIXED: f32 = 560.0;
/// The search field: its widest, and the least worth showing
const SEARCH_WIDTH: f32 = 240.0;
const SEARCH_MIN: f32 = 90.0;

pub const MAC_TITLEBAR: bool = cfg!(target_os = "macos");

/// A strip of the window that moves it when it is dragged (double click: maximize)
fn drag_window(ui: &mut egui::Ui, rect: egui::Rect) {
    let response = ui.interact(rect, ui.id().with("drag-window"), egui::Sense::click_and_drag());
    if response.double_clicked() {
        let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    } else if response.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

/// The `file://` uri of a path, as the image loaders of egui want it (`file:///C:/dir/a.png` on Windows,
/// where `file://C:\dir` would be read as a network path)
pub fn file_uri(path: &Path) -> String {
    let p = path.to_string_lossy();
    if cfg!(windows) { format!("file:///{}", p.replace('\\', "/")) } else { format!("file://{p}") }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn open_with_system(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

/// Inter for the texts, with a semibold family for the titles (the style of pyegui needs it)
fn fonts(nerd: Option<&Path>) -> egui::FontDefinitions {
    use egui::{FontData, FontFamily};
    let mut fonts = egui::FontDefinitions::default();
    let proportional = fonts.families[&FontFamily::Proportional].clone();
    for (name, data) in [
        ("Inter-Regular", &include_bytes!("../../src/fonts/Inter-Regular.ttf")[..]),
        ("Inter-SemiBold", &include_bytes!("../../src/fonts/Inter-SemiBold.ttf")[..]),
    ] {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(data)));
    }
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "Inter-Regular".to_owned());
    let bold = std::iter::once("Inter-SemiBold".to_owned()).chain(proportional).collect();
    fonts.families.insert(style::bold_family(), bold);
    // the font of the prompt, with the monospace fonts behind it for the characters it does not have
    if let Some(data) = nerd.and_then(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("nerd".to_owned(), Arc::new(FontData::from_owned(data)));
        let family = std::iter::once("nerd".to_owned()).chain(fonts.families[&FontFamily::Monospace].clone()).collect();
        fonts.families.insert(FontFamily::Name("nerd".into()), family);
    }
    fonts
}

/// `FINDER_SHOT=/path/file.png finder`: saves the window to a PNG after a second and quits, to look at the
/// result without the screen (the checks of the look)
fn screenshot_for_checks(ctx: &egui::Context) {
    let Some(path) = std::env::var_os("FINDER_SHOT").map(std::path::PathBuf::from) else { return };
    thread_local! { static FRAMES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) }; }
    let n = FRAMES.with(|f| {
        f.set(f.get() + 1);
        f.get()
    });
    ctx.request_repaint();
    if n == 90 {
        let ctx2 = ctx.clone();
        ctx.request_screenshot(move |image| {
            let [w, h] = image.size;
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buffer) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = buffer.save(&path);
            }
            ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
            ctx2.request_repaint();
        });
    }
}

/// The picture, made smaller and blurred (a radius in points of the window), as a texture
fn blurred_texture(ctx: &egui::Context, path: &Path, blur: u32) -> Option<egui::TextureHandle> {
    let img = image::open(path).ok()?;
    let img = img.thumbnail(1280, 1280).to_rgba8();
    let img = if blur > 0 { image::imageops::blur(&img, blur as f32 / 2.0) } else { img };
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture("finder-background", color, egui::TextureOptions::LINEAR))
}
