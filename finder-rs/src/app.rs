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
    /// the contrast and the saturation of the glass or of the picture, in percent (100: unchanged)
    pub contrast: u32,
    pub saturation: u32,
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
        Look { background: Background::default_for_system(), color: None, image: None, blur: 0, contrast: 100, saturation: 100, layers: Default::default(), outline_width: [1.0; 2], outline: [None; 2], text: None }
    }
}

fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"))
}

/// The font files installed (.ttf and .otf) by name, sorted: the choices of the fonts of the app
fn installed_fonts() -> &'static [(String, PathBuf)] {
    static FONTS: std::sync::OnceLock<Vec<(String, PathBuf)>> = std::sync::OnceLock::new();
    FONTS.get_or_init(|| {
        let mut folders = vec![PathBuf::from("/Library/Fonts"), PathBuf::from("/System/Library/Fonts"), PathBuf::from("/System/Library/Fonts/Supplemental")];
        folders.push(PathBuf::from("/usr/share/fonts"));
        folders.push(PathBuf::from(r"C:\Windows\Fonts"));
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            folders.push(home.join("Library/Fonts"));
            folders.push(home.join(".local/share/fonts"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            folders.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
        }
        let mut found = Vec::new();
        let mut pending: Vec<(PathBuf, u8)> = folders.into_iter().map(|f| (f, 0)).collect();
        while let Some((folder, depth)) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&folder) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if depth < 2 {
                        pending.push((path, depth + 1));
                    }
                    continue;
                }
                let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
                if matches!(ext.as_deref(), Some("ttf" | "otf")) {
                    if let Some(stem) = path.file_stem().and_then(|n| n.to_str()) {
                        found.push((stem.to_owned(), path));
                    }
                }
            }
        }
        found.sort_by_key(|(name, _)| name.to_lowercase());
        found.dedup_by(|a, b| a.0 == b.0);
        found
    })
}

/// The size of a font in points, a small field to drag or type in
fn size_drag(size: &mut f32, range: std::ops::RangeInclusive<f32>) -> egui::DragValue<'_> {
    egui::DragValue::new(size).range(range).speed(0.1).max_decimals(0).suffix(" pt")
}

/// A list of the installed fonts, "Par défaut" first; true when the choice changed
fn font_combo(ui: &mut egui::Ui, id: &str, current: &mut Option<PathBuf>, fonts: &[(String, PathBuf)]) -> bool {
    let label = current.as_ref().and_then(|p| p.file_stem()).and_then(|n| n.to_str()).unwrap_or("Par défaut").to_owned();
    let mut picked: Option<Option<PathBuf>> = None;
    egui::ComboBox::from_id_salt(id).width(110.0).selected_text(label).show_ui(ui, |ui| {
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            if ui.selectable_label(current.is_none(), "Par défaut").clicked() {
                picked = Some(None);
            }
            for (name, path) in fonts {
                if ui.selectable_label(current.as_ref() == Some(path), name).clicked() {
                    picked = Some(Some(path.clone()));
                }
            }
        });
    });
    match picked {
        Some(choice) => {
            *current = choice;
            true
        }
        None => false,
    }
}

/// A color button (the picker has its own popup); true when the color was changed
fn picker(ui: &mut egui::Ui, color: &mut egui::Color32, alpha: egui::color_picker::Alpha) -> bool {
    egui::color_picker::color_edit_button_srgba(ui, color, alpha).changed()
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
    /// the font files chosen for the interface and for the terminal (None: the default ones)
    font_ui: Option<PathBuf>,
    font_terminal: Option<PathBuf>,
    /// the size of the texts of the interface (the body, 14 by default) and of the terminal (13)
    font_size: f32,
    terminal_font_size: f32,
    /// the widths of the columns of the list and of the tree, by view and column key
    column_widths: std::collections::BTreeMap<String, std::collections::BTreeMap<String, f32>>,
    /// the look of the light theme and of the dark theme, kept apart
    looks: [Look; 2],
    /// an accent chosen with the color picker
    accent_custom: Option<[u8; 3]>,
    show_hidden: bool,
    /// Windows: the folders in the blue of the Finder, not the yellow of the Explorer
    blue_folders: bool,
    dock: Option<String>,
    open_folders: Vec<PathBuf>,
    /// the sections of the sidebar that are folded (Favoris, Emplacements, Étiquettes)
    closed_sections: Vec<String>,
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
            font_ui: None,
            font_terminal: None,
            font_size: 14.0,
            terminal_font_size: 13.0,
            column_widths: Default::default(),
            looks: Default::default(),
            accent_custom: None,
            show_hidden: false,
            blue_folders: !cfg!(target_os = "macos"),
            dock: None,
            open_folders: Vec::new(),
            closed_sections: Vec::new(),
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
    pub font_ui: Option<PathBuf>,
    pub font_terminal: Option<PathBuf>,
    pub font_size: f32,
    pub terminal_font_size: f32,
    pub column_widths: std::collections::BTreeMap<String, std::collections::BTreeMap<String, f32>>,
    pub looks: [Look; 2],
    pub accent_custom: Option<[u8; 3]>,
    /// the picture behind the glasses, blurred: the file, the blur and its texture
    bg_texture: Option<(PathBuf, [u32; 3], egui::TextureHandle)>,
    applied_filter: [u32; 3],
    /// the blur once the slider is let go (while it is dragged the window server and the picture are not asked again at each frame)
    settled_blur: Option<[[u32; 3]; 2]>,
    title: String,
    /// a window opened by the first one: it has no native backdrop and closes on its own
    secondary: bool,
    /// the other windows, drawn after this one
    windows: Vec<(egui::ViewportId, Box<FinderApp>)>,
    pub new_window: Option<PathBuf>,
    window_serial: u64,
    backdrop_applied: bool,
    #[cfg(target_os = "macos")]
    native: Option<pyegui_core::window::NativeWindow>,
    corners_applied: bool,
    /// the keyboard goes back to the view (after a navigation, a search field that is still focused lets it go)
    focus_view: bool,
    custom_titlebar: bool,
    terminals: Terminals,
    pub show_hidden: bool,
    blue_folders: bool,
    closed_sections: std::collections::BTreeSet<String>,
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
    /// the tag asked to be deleted (its name, until the answer)
    confirm_tag_delete: Option<(i64, String)>,
    /// the action palette: (query, highlighted row)
    palette: Option<(String, usize)>,
    /// the palette shows the tree of the tags (the page that slides in) and its highlighted row
    palette_tags: bool,
    palette_cursor: usize,
    /// what is typed in the field of the page of the tags (a name or a path a/b/c)
    palette_tag_query: String,
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
    tag_counts: std::cell::RefCell<(u64, std::collections::HashMap<i64, usize>)>,
    /// the favorites of the sidebar as drawn at the last frame: their ids, the middle of their rows, the
    /// area of the section (where a folder can be dropped to add one)
    fav_ids: Vec<i64>,
    fav_rows: Vec<f32>,
    fav_rect: Option<egui::Rect>,
    /// the place (an index among the rows) where a favorite being moved, or a folder being dragged over, goes
    fav_insert: Option<usize>,
    /// the favorite being moved with the mouse
    fav_drag: Option<i64>,
    /// the tag being moved with the mouse in the sidebar
    tag_drag: Option<i64>,
    /// the tag whose trash was clicked once in the sidebar (red: a second click deletes it)
    tag_armed: Option<i64>,
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

    /// Puts the selected items among the favorites, or the item shown (the file of the viewer, else the
    /// folder) when nothing is selected
    pub fn add_favorites(&mut self) {
        let mut paths = self.selected_paths();
        if paths.is_empty() {
            paths.push(self.viewer_path().map(Path::to_path_buf).unwrap_or_else(|| self.folder().to_path_buf()));
        }
        if let Some(store) = &mut self.tags {
            for path in paths {
                store.add_favorite(&path);
            }
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
        let saved: Saved = cc
            .storage
            .and_then(|s| s.get_string(STATE_KEY))
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        cc.egui_ctx.set_fonts(fonts(terminals::nerd_font().as_deref(), saved.font_ui.as_deref(), saved.font_terminal.as_deref()));
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
            font_ui: self.font_ui.clone(),
            font_terminal: self.font_terminal.clone(),
            font_size: self.font_size,
            terminal_font_size: self.terminal_font_size,
            column_widths: self.column_widths.clone(),
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
        let mut views = Views::new(&core, &folder, saved.show_hidden);
        views.set_column_widths(&saved.column_widths);
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
            font_ui: saved.font_ui,
            font_terminal: saved.font_terminal,
            font_size: saved.font_size.clamp(9.0, 24.0),
            terminal_font_size: saved.terminal_font_size.clamp(8.0, 30.0),
            column_widths: saved.column_widths.clone(),
            looks: saved.looks,
            accent_custom: saved.accent_custom,
            bg_texture: None,
            applied_filter: [0, 100, 100],
            settled_blur: None,
            title: String::new(),
            secondary: false,
            windows: Vec::new(),
            new_window: None,
            window_serial: 0,
            backdrop_applied: false,
            #[cfg(target_os = "macos")]
            native: None,
            corners_applied: false,
            focus_view: false,
            custom_titlebar,
            terminals: Terminals::new(&folder_for_terminal, font_path.as_deref()),
            show_hidden: saved.show_hidden,
            blue_folders: saved.blue_folders,
            closed_sections: saved.closed_sections.iter().cloned().collect(),
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
            confirm_tag_delete: None,
            palette: None,
            palette_tags: false,
            palette_cursor: 0,
            palette_tag_query: String::new(),
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
            tag_counts: Default::default(),
            fav_ids: Vec::new(),
            fav_rows: Vec::new(),
            fav_rect: None,
            fav_insert: None,
            fav_drag: None,
            tag_drag: None,
            tag_armed: None,
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

    /// The blur, the contrast and the saturation of both looks, kept as they were while the pointer is
    /// down (a slider being dragged)
    fn settled_blur(&mut self, ctx: &egui::Context) -> [[u32; 3]; 2] {
        let wanted = [0, 1].map(|d| [self.looks[d].blur, self.looks[d].contrast, self.looks[d].saturation]);
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
            // the size of the texts of the interface: every size of the style follows the body one (14)
            let scale = self.font_size / 14.0;
            if (scale - 1.0).abs() > 0.001 {
                for font in s.text_styles.values_mut() {
                    font.size *= scale;
                }
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
        // over the favorites of the sidebar: the folders are added there
        if self.favorites_drop(ctx, &hovered, &dropped) {
            return;
        }
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
        if let Some((id, name)) = self.confirm_tag_delete.take() {
            let mut keep = true;
            let mut yes = false;
            egui::Modal::new(egui::Id::new("delete-tag")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.heading("Supprimer l'étiquette ?");
                ui.label(format!("« {name} » est retirée de tous les éléments. Les fichiers et les sous-étiquettes sont conservés."));
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
                if let Some(store) = self.tags.as_mut() {
                    store.delete(id);
                }
                self.set_tag_filter(None);
            } else if keep {
                self.confirm_tag_delete = Some((id, name));
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
    /// Opens the palette (a right click does too: it replaces the context menu)
    pub fn open_palette(&mut self) {
        if self.palette.is_none() {
            self.palette = Some((String::new(), 0));
            self.palette_tags = false;
            self.palette_cursor = 0;
            self.palette_tag_query.clear();
        }
    }

    /// The palette: the actions to search and run, and the entry "Étiquettes", a page that slides in with the
    /// tree of the tags, each with a box to check for the selected items. Up and Down move, Enter or Space
    /// checks, Left or Backspace goes back, Esc closes the whole popup
    fn palette_modal(&mut self, ctx: &egui::Context) {
        let page_id = egui::Id::new("palette-page");
        let Some((mut query, mut index)) = self.palette.take() else {
            // closed: everything is reset, so that the next opening (a key, a right click) starts on the main
            // page, with empty fields, and does not slide
            self.palette_tags = false;
            self.palette_cursor = 0;
            self.palette_tag_query.clear();
            ctx.animate_bool_with_time(page_id, false, 0.0);
            return;
        };
        let slide = ctx.animate_bool_with_time_and_easing(page_id, self.palette_tags, 0.22, egui::emath::easing::cubic_out);
        let on_tags = self.palette_tags;
        let needle = crate::search::fold(query.trim());
        let mut rows: Vec<Option<usize>> = Vec::new(); // None: the entry of the tags
        if crate::search::fold("Étiquettes").contains(&needle) || "tags".contains(&needle) {
            rows.push(None);
        }
        rows.extend(actions::ACTIONS.iter().enumerate().filter(|(_, a)| (a.enabled)(self) && crate::search::fold(a.label).contains(&needle)).map(|(i, _)| i).map(Some));
        // the rows of the page of the tags: the tags (the tree, or what the field finds), and the
        // creation of what the field holds when it does not exist yet
        enum TagItem {
            Tag(usize),
            Create(Vec<String>),
        }
        let parts = tagsui_split(&self.palette_tag_query);
        let (tags, can_create): (Vec<(crate::tagstore::Tag, Vec<String>)>, bool) = match &self.tags {
            Some(store) if slide > 0.0 || on_tags => {
                if parts.is_empty() {
                    (self.tag_list().into_iter().map(|t| (t, Vec::new())).collect(), false)
                } else {
                    (store.matching(&parts).into_iter().map(|t| { let path = store.tag_path(t.id); (t, path) }).collect(), store.can_create(&parts))
                }
            }
            _ => (Vec::new(), false),
        };
        let mut items: Vec<TagItem> = Vec::new();
        items.extend((0..tags.len()).map(TagItem::Tag));
        if can_create {
            items.push(TagItem::Create(parts.clone()));
        }
        let selection = self.selected_paths();
        let owned: Vec<_> = match &self.tags {
            Some(store) if slide > 0.0 || on_tags => selection.iter().map(|p| store.own(p)).collect(),
            _ => Vec::new(),
        };
        let state_of = |id: i64| -> u8 {
            let have = owned.iter().filter(|o| o.contains(&id)).count();
            if owned.is_empty() || have == 0 { 0 } else if have == owned.len() { 2 } else { 1 }
        };
        self.palette_cursor = self.palette_cursor.min(items.len().saturating_sub(1));
        let typing = !self.palette_tag_query.is_empty();
        let (down, up, enter, escape, left, right, space, backspace) = ctx.input_mut(|i| {
            let n = Modifiers::NONE;
            let both = on_tags;
            (
                i.consume_key(n, Key::ArrowDown),
                i.consume_key(n, Key::ArrowUp),
                i.consume_key(n, Key::Enter),
                i.consume_key(n, Key::Escape),
                both && !typing && i.consume_key(n, Key::ArrowLeft),
                !on_tags && query.is_empty() && i.consume_key(n, Key::ArrowRight),
                both && !typing && i.consume_key(n, Key::Space),
                both && !typing && i.consume_key(n, Key::Backspace),
            )
        });
        let mut close = escape;
        let mut chosen: Option<usize> = None;
        let mut open_tags = false;
        let mut toggled: Option<(i64, bool)> = None;
        let mut create: Option<Vec<String>> = None;
        if on_tags {
            if down {
                self.palette_cursor = (self.palette_cursor + 1).min(items.len().saturating_sub(1));
            }
            if up {
                self.palette_cursor = self.palette_cursor.saturating_sub(1);
            }
            if left || backspace {
                self.palette_tags = false;
            }
            if enter || (space && !typing) {
                match items.get(self.palette_cursor) {
                    Some(TagItem::Tag(k)) => {
                        let tag = &tags[*k].0;
                        if !selection.is_empty() {
                            toggled = Some((tag.id, state_of(tag.id) != 2));
                        }
                    }
                    Some(TagItem::Create(parts)) => create = Some(parts.clone()),
                    None => {}
                }
            }
        } else {
            index = index.min(rows.len().saturating_sub(1));
            if down {
                index = (index + 1).min(rows.len().saturating_sub(1));
            }
            if up {
                index = index.saturating_sub(1);
            }
            if enter || right {
                match rows.get(index) {
                    Some(None) => open_tags = true,
                    Some(Some(i)) if enter => chosen = Some(*i),
                    _ => {}
                }
            }
        }
        let shortcuts: Vec<String> = rows.iter().map(|r| r.and_then(|i| actions::ACTIONS[i].shortcut.as_ref().map(|s| ctx.format_shortcut(s))).unwrap_or_default()).collect();
        let accent = self.accent_color();
        let page_size = egui::vec2(460.0, 360.0);
        let mut cursor = self.palette_cursor;
        let mut tag_query = self.palette_tag_query.clone();
        let mut go_back = false;
        let response = egui::Modal::new(egui::Id::new("palette")).show(ctx, |ui| {
            ui.set_width(page_size.x);
            let (area, _) = ui.allocate_exact_size(page_size, egui::Sense::hover());
            // the main page leaves to the left while the tags come in from the right, and back
            if slide < 1.0 {
                let mut page = ui.new_child(egui::UiBuilder::new().max_rect(area.translate(egui::vec2(-slide * area.width(), 0.0))));
                page.set_clip_rect(area);
                let edit = page.add(egui::TextEdit::singleline(&mut query).hint_text("Rechercher une action…").desired_width(f32::INFINITY));
                if !on_tags && !edit.has_focus() {
                    edit.request_focus();
                }
                if edit.changed() {
                    index = 0;
                }
                page.add_space(4.0);
                if rows.is_empty() {
                    page.weak("Aucune action");
                }
                egui::ScrollArea::vertical().max_height(page_size.y - 40.0).show(&mut page, |ui| {
                    for (row, entry) in rows.iter().enumerate() {
                        let (rect, r) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::click());
                        if row == index && !on_tags {
                            ui.painter().rect_filled(rect, 6.0, ui.visuals().selection.bg_fill.gamma_multiply(0.45));
                            if down || up {
                                ui.scroll_to_rect(rect, None);
                            }
                        } else if r.hovered() {
                            ui.painter().rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(127, 127, 127, 30));
                        }
                        let color = ui.visuals().text_color();
                        let (icon, label) = match entry {
                            None => ("tag", "Étiquettes"),
                            Some(i) => (actions::ACTIONS[*i].icon, actions::ACTIONS[*i].label),
                        };
                        glyphs::image(glyphs::by_name(icon), 16.0, color).paint_at(ui, egui::Rect::from_center_size(rect.left_center() + egui::vec2(18.0, 0.0), egui::vec2(16.0, 16.0)));
                        ui.painter().text(rect.left_center() + egui::vec2(34.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(14.0), color);
                        let right_text = if entry.is_none() { "›" } else { shortcuts[row].as_str() };
                        ui.painter().text(rect.right_center() - egui::vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, right_text, egui::FontId::proportional(12.0), ui.visuals().weak_text_color());
                        if r.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                            index = row;
                        }
                        if r.clicked() {
                            match entry {
                                None => open_tags = true,
                                Some(i) => chosen = Some(*i),
                            }
                        }
                    }
                });
            }
            if slide > 0.0 {
                let mut page = ui.new_child(egui::UiBuilder::new().max_rect(area.translate(egui::vec2((1.0 - slide) * area.width(), 0.0))));
                page.set_clip_rect(area);
                page.horizontal(|ui| {
                    ui.set_min_width(area.width() - 8.0);
                    if glyphs::button(ui, glyphs::CHEVRON_LEFT, 14.0, false, "Retour (←)").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        go_back = true;
                    }
                    ui.label(RichText::new("Étiquettes").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(RichText::new("↑↓  Espace : cocher  ←  Échap").small());
                    });
                });
                let edit = page.add(egui::TextEdit::singleline(&mut tag_query).hint_text("Chercher ou créer une étiquette (a/b/c)").desired_width(f32::INFINITY));
                if on_tags && !edit.has_focus() {
                    edit.request_focus();
                }
                if edit.changed() {
                    cursor = 0;
                }
                if selection.is_empty() {
                    page.weak("Aucun élément sélectionné : les cases sont inactives");
                }
                egui::ScrollArea::vertical().max_height(page_size.y - 84.0).show(&mut page, |ui| {
                    let highlighted = cursor;
                    let row = |ui: &mut egui::Ui, n: usize, depth: usize, glyph: glyphs::Glyph, tint: Color32, label: &str, state: Option<u8>| {
                        let (rect, r) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
                        if n == highlighted {
                            ui.painter().rect_filled(rect, 6.0, ui.visuals().selection.bg_fill.gamma_multiply(0.45));
                            if down || up {
                                ui.scroll_to_rect(rect, None);
                            }
                        } else if r.hovered() {
                            ui.painter().rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(127, 127, 127, 30));
                        }
                        let x = rect.left() + 10.0 + depth as f32 * 18.0;
                        let text_x;
                        if let Some(state) = state {
                            let name = ["checkbox-off", "checkbox-mixed", "checkbox-on"][state as usize];
                            let box_tint = if state > 0 { accent } else { ui.visuals().weak_text_color() };
                            glyphs::image(glyphs::by_name(name), 16.0, box_tint).paint_at(ui, egui::Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), egui::vec2(16.0, 16.0)));
                            ui.painter().circle_filled(egui::pos2(x + 30.0, rect.center().y), 5.0, tint);
                            text_x = x + 42.0;
                        } else {
                            glyphs::image(glyph, 16.0, tint).paint_at(ui, egui::Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), egui::vec2(16.0, 16.0)));
                            text_x = x + 26.0;
                        }
                        let color = if state.is_some() && selection.is_empty() { ui.visuals().weak_text_color() } else { ui.visuals().text_color() };
                        ui.painter().text(egui::pos2(text_x, rect.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(14.0), color);
                        r
                    };
                    for (n, item) in items.iter().enumerate() {
                        match item {
                            TagItem::Tag(k) => {
                                let (tag, path) = &tags[*k];
                                let state = state_of(tag.id);
                                // the tree when nothing is typed, else the full path of what is found
                                let (depth, label) = if parts.is_empty() { (tag.depth, tag.name.clone()) } else { (0, path.join(" / ")) };
                                if row(ui, n, depth, glyphs::by_name("dot"), tagsui::parse_color(&tag.color), &label, Some(state)).clicked() {
                                    cursor = n;
                                    if !selection.is_empty() {
                                        toggled = Some((tag.id, state != 2));
                                    }
                                }
                            }
                            TagItem::Create(new_parts) => {
                                let label = format!("Créer « {} »  ↵", new_parts.join(" / "));
                                if row(ui, n, 0, glyphs::FOLDER_ENTER, accent, &label, None).clicked() {
                                    cursor = n;
                                    create = Some(new_parts.clone());
                                }
                            }
                        }
                    }
                    if items.is_empty() {
                        ui.weak("Aucune étiquette");
                    }
                });
            }
        });
        self.palette_cursor = cursor;
        self.palette_tag_query = tag_query;
        if open_tags {
            self.palette_tags = true;
            self.palette_cursor = 0;
            self.palette_tag_query.clear();
        }
        if let Some(parts) = create {
            // the new tag is put on the selection, and the field is emptied for the next one
            if let Some(store) = self.tags.as_mut() {
                if let Some(leaf) = store.create_path(&parts) {
                    for path in &selection {
                        store.tag(path, leaf);
                    }
                }
            }
            self.palette_tag_query.clear();
            self.palette_cursor = 0;
        }
        if go_back {
            self.palette_tags = false;
        }
        if let Some((tag, on)) = toggled {
            if let Some(store) = self.tags.as_mut() {
                for path in &selection {
                    if on {
                        store.tag(path, tag)
                    } else {
                        store.untag(path, tag)
                    }
                }
            }
        }
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
        // for the checks of the look (FINDER_SHOT): the first tag shown
        if std::env::var_os("FINDER_TAGHEADER").is_some() && ctx.cumulative_frame_nr() == 20 {
            if let Some(tag) = self.tag_list().first() {
                self.set_tag_filter(Some(tag.id));
            }
        }
        // for the checks of the look (FINDER_SHOT): the palette opened by itself
        if let Some(page) = std::env::var_os("FINDER_PALETTE") {
            if ctx.cumulative_frame_nr() == 20 {
                self.palette = Some((String::new(), 0));
                self.palette_tags = page == "tags";
                if let Some(q) = std::env::var_os("FINDER_TAGQUERY") {
                    self.palette_tag_query = q.to_string_lossy().into_owned();
                }
            }
        }
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
        // a file open in the viewer: Backspace closes it, like Escape (it does not go up a folder)
        if self.viewer.is_some() {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace) | i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.viewer = None;
            }
            return;
        }
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
        self.terminals.shell.font_size = self.terminal_font_size;
        self.terminals.python.font_size = self.terminal_font_size;
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
        let mut show_search = search_width >= SEARCH_MIN;
        let show_view = row >= TOOLBAR_FIXED - 120.0;
        // what the right part takes, to keep the name of the folder out of it
        let right_of = |search: bool| 2.0 * 34.0 + 12.0 + if search { search_width + 8.0 } else { 0.0 } + if show_view { 3.0 * 32.0 + 8.0 } else { 0.0 };
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
            // the search is hidden as soon as the whole combo of the folder no longer fits
            if show_search {
                let folder = self.viewer_path().map(Path::to_path_buf).unwrap_or_else(|| self.folder().to_path_buf());
                let name = folder.file_name().map_or_else(|| folder.display().to_string(), |n| n.to_string_lossy().into_owned());
                let text = ui.painter().layout_no_wrap(name, egui::FontId::proportional(15.0), egui::Color32::WHITE).size().x;
                let natural = text + 6.0 + 16.0 + 6.0 + 6.0 + 12.0 + 6.0 + 8.0;
                if ui.available_width() - right_of(true) < natural {
                    show_search = false;
                }
            }
            let room = (ui.available_width() - right_of(show_search)).max(60.0);
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

    /// Light, dark or the system theme, and the settings of the look: the four cells of one segmented
    /// control, the last one opens the popup of the appearance
    fn theme_control(&mut self, ui: &mut egui::Ui) {
        let before = self.theme;
        // the panel is not a popup of egui (only one is open at a time: the picker of a color would close
        // it): it stays open until the gear or its cross is clicked
        let open_id = egui::Id::new("appearance-open");
        let mut open = ui.ctx().data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
        // for the checks of the look (FINDER_SHOT): the panel opened by itself
        if std::env::var_os("FINDER_APPEARANCE").is_some() && ui.ctx().cumulative_frame_nr() == 20 {
            open = true;
        }
        let choice = glyphs::segmented(ui, "theme", 14.0, &[
            glyphs::cell(glyphs::SUN, self.theme == ThemeChoice::Light, "Clair (⌘7)"),
            glyphs::cell(glyphs::MOON, self.theme == ThemeChoice::Dark, "Sombre (⌘8)"),
            glyphs::cell(glyphs::THEME_SYSTEM, self.theme == ThemeChoice::System, "Thème du système (⌘9)"),
            glyphs::cell(glyphs::GEAR, open, if open { "" } else { "Apparence" }),
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
        let gear = &choice.responses[3];
        let mut open = open ^ gear.clicked();
        if open {
            let pos = egui::pos2(gear.rect.right(), gear.rect.top() - 8.0);
            egui::Area::new(egui::Id::new("appearance-panel"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(pos)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if glyphs::button(ui, glyphs::CLOSE, 12.0, false, "Fermer").clicked() {
                                open = false;
                            }
                            ui.label(RichText::new("Apparence").heading());
                        });
                        ui.separator();
                        self.appearance_menu(ui);
                    });
                });
        }
        ui.ctx().data_mut(|d| d.insert_temp(open_id, open));
    }

    /// The popup of the appearance: the accent, then the settings of the theme shown (light and dark
    /// are kept apart). Every color is a picker with its transparency
    pub fn appearance_menu(&mut self, ui: &mut egui::Ui) {
        use egui::color_picker::Alpha;
        ui.set_width(250.0);
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
        ui.spacing_mut().slider_width = 120.0;
        // the rail of a slider is drawn with this fill, transparent in the glass look
        let rail = ui.visuals().text_color().gamma_multiply(0.18);
        ui.visuals_mut().widgets.inactive.bg_fill = rail;
        // the accent is one color
        ui.horizontal(|ui| {
            ui.label(RichText::new("Accent").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut color = self.accent_color();
                if picker(ui, &mut color, Alpha::Opaque) {
                    self.accent_custom = Some([color.r(), color.g(), color.b()]);
                    self.apply_theme(ui.ctx());
                }
            });
        });
        // the fonts: one for the interface, one for the terminal
        let installed = installed_fonts();
        let mut fonts_changed = false;
        let mut size_changed = false;
        egui::Grid::new("font-grid").num_columns(2).spacing([10.0, 6.0]).min_col_width(70.0).show(ui, |ui| {
            ui.label("Police");
            ui.horizontal(|ui| {
                fonts_changed |= font_combo(ui, "font-ui", &mut self.font_ui, installed);
                size_changed |= ui.add(size_drag(&mut self.font_size, 9.0..=24.0)).on_hover_text("Taille").changed();
            });
            ui.end_row();
            ui.label("Terminal");
            ui.horizontal(|ui| {
                fonts_changed |= font_combo(ui, "font-terminal", &mut self.font_terminal, installed);
                ui.add(size_drag(&mut self.terminal_font_size, 8.0..=30.0)).on_hover_text("Taille");
            });
            ui.end_row();
        });
        if size_changed {
            self.apply_theme(ui.ctx());
        }
        if fonts_changed {
            ui.ctx().set_fonts(fonts(terminals::nerd_font().as_deref(), self.font_ui.as_deref(), self.font_terminal.as_deref()));
        }
        let dark = ui.visuals().dark_mode;
        let d = dark as usize;
        let mut look = self.looks[d].clone();
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new(if dark { "Thème sombre" } else { "Thème clair" }).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Par défaut").on_hover_text("Remet le fond, les vitres, les contours et le texte de ce thème").clicked() {
                    look = Look::default();
                }
            });
        });
        egui::Grid::new("look-grid").num_columns(2).spacing([10.0, 6.0]).min_col_width(70.0).show(ui, |ui| {
            ui.label("Fond");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if backdrop_name().is_some() {
                    ui.selectable_value(&mut look.background, Background::Glass, "Vitre");
                }
                ui.selectable_value(&mut look.background, Background::Image, "Image");
                ui.selectable_value(&mut look.background, Background::Color, "Couleur");
            });
            ui.end_row();
            if look.background != Background::Color {
                // the blur of the glass of the system is only set on macOS
                let unavailable = look.background == Background::Glass && !cfg!(target_os = "macos");
                ui.label("Flou");
                let slider = ui.add_enabled(!unavailable, egui::Slider::new(&mut look.blur, 0..=40));
                if unavailable {
                    slider.on_disabled_hover_text("Non disponible");
                }
                ui.end_row();
                let percent = |v: f64, _: std::ops::RangeInclusive<usize>| format!("{v:.0} %");
                ui.label("Contraste");
                ui.add_enabled(!unavailable, egui::Slider::new(&mut look.contrast, 50..=200).custom_formatter(percent));
                ui.end_row();
                ui.label("Saturation");
                ui.add_enabled(!unavailable, egui::Slider::new(&mut look.saturation, 0..=200).custom_formatter(percent));
                ui.end_row();
            }
            match look.background {
                Background::Image => {
                    ui.label("Image");
                    ui.horizontal(|ui| {
                        let mut path = look.image.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                        if ui.add(egui::TextEdit::singleline(&mut path).hint_text("Chemin").desired_width(100.0)).changed() {
                            look.image = if path.trim().is_empty() { None } else { Some(expand(Path::new(path.trim()))) };
                        }
                        let selection = self.selected.iter().next().filter(|p| is_image(p)).cloned();
                        if ui.add_enabled(selection.is_some(), egui::Button::new("Sélection")).on_hover_text("Utiliser l'image sélectionnée").clicked() {
                            look.image = selection;
                        }
                    });
                    ui.end_row();
                }
                Background::Color => {
                    let default = if dark { egui::Color32::from_rgb(22, 24, 30) } else { egui::Color32::from_rgb(235, 235, 240) };
                    let mut color = look.color.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b)).unwrap_or(default);
                    ui.label("Couleur");
                    if picker(ui, &mut color, Alpha::Opaque) {
                        look.color = Some([color.r(), color.g(), color.b()]);
                    }
                    ui.end_row();
                }
                Background::Glass => {}
            }
            // the glasses: tint and opacity in one picker
            for (i, (layer, name)) in LAYERS.iter().enumerate() {
                let (tint, opacity) = style::layer_values(*layer, dark);
                let mut color = egui::Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), (opacity * 255.0).round() as u8);
                ui.label(*name);
                if picker(ui, &mut color, Alpha::OnlyBlend) {
                    let [r, g, b, a] = color.to_srgba_unmultiplied();
                    // a fully transparent color has lost its tint: the old one is kept
                    if a > 0 {
                        look.layers[i].tint = Some([r, g, b]);
                    }
                    look.layers[i].opacity = Some(a as f32 / 255.0);
                }
                ui.end_row();
            }
            // the outlines: color with its alpha, and thickness
            for (k, name) in ["Contour actif", "Contour inactif"].into_iter().enumerate() {
                let (_, current) = self.outline(&ui.visuals().clone(), k == 0);
                let mut color = look.outline[k].map(|[r, g, b, a]| egui::Color32::from_rgba_unmultiplied(r, g, b, a)).unwrap_or(current);
                ui.label(name);
                ui.horizontal(|ui| {
                    if picker(ui, &mut color, Alpha::OnlyBlend) {
                        let [r, g, b, a] = color.to_srgba_unmultiplied();
                        look.outline[k] = Some([r, g, b, a]);
                    }
                    ui.add(egui::DragValue::new(&mut look.outline_width[k]).range(0.0..=6.0).speed(0.05).max_decimals(1).suffix(" pt"))
                        .on_hover_text("Épaisseur");
                });
                ui.end_row();
            }
            let mut text = look.text.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b)).unwrap_or(ui.visuals().text_color());
            ui.label("Texte");
            if picker(ui, &mut text, Alpha::Opaque) {
                look.text = Some([text.r(), text.g(), text.b()]);
            }
            ui.end_row();
        });
        if look != self.looks[d] {
            self.looks[d] = look;
            self.apply_layers();
            self.apply_theme(ui.ctx());
        }
        ui.separator();
        egui::Grid::new("look-terminal").num_columns(2).spacing([10.0, 6.0]).min_col_width(70.0).show(ui, |ui| {
            ui.label("Terminal");
            ui.add(egui::Slider::new(&mut self.terminal_opacity, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
            ui.end_row();
        });
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
        let mut ask_delete = false;
        let mut new_color: Option<String> = None;
        // a band as high as the toolbar, across the pane: the sheet of the pane is left clear behind it (see
        // `pane_ui`), so it has the tint of the toolbar
        let size = egui::vec2(ui.available_width(), TOP_H);
        let band = ui
            .allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
                // the color of the tag: a picker in place of the dot
                let mut color = tagsui::parse_color(&tag.color);
                if egui::color_picker::color_edit_button_srgba(ui, &mut color, egui::color_picker::Alpha::Opaque).on_hover_text("Couleur de l'étiquette").changed() {
                    new_color = Some(tagsui::to_hex(color));
                }
                ui.label(RichText::new(&tag.name).strong());
                ui.weak(format!("{count} élément(s) avec cette étiquette ou ses sous-étiquettes"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::image(glyphs::image(glyphs::CLOSE, 12.0, ui.visuals().text_color())).frame(false).min_size(egui::vec2(28.0, 22.0))).on_hover_text("Revenir au dossier").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        close = true;
                    }
                    if ui.add(egui::Button::image(glyphs::image(glyphs::by_name("trash"), 14.0, ui.visuals().text_color())).frame(false).min_size(egui::vec2(28.0, 22.0))).on_hover_text("Supprimer l'étiquette").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        ask_delete = true;
                    }
                });
            })
            .response
            .rect;
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(TAG_BAND), band));
        if ask_delete {
            self.confirm_tag_delete = Some((tag.id, tag.name.clone()));
        }
        if let (Some(hex), Some(store)) = (new_color, self.tags.as_mut()) {
            store.recolor(tag.id, &hex);
        }
        if close {
            self.set_tag_filter(None);
        }
    }

    /// The tags of the store, read once per version of the store
    /// The number of items tagged with each tag (or a tag under it), kept until the store changes
    fn tag_count(&self, id: i64) -> usize {
        let Some(store) = &self.tags else { return 0 };
        let mut known = self.tag_counts.borrow_mut();
        if known.0 != store.version + 1 {
            let counts = store.all().iter().map(|t| (t.id, store.tagged_with(t.id).len())).collect();
            *known = (store.version + 1, counts);
        }
        known.1.get(&id).copied().unwrap_or(0)
    }

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
        self.place_row_with(ui, glyph, name, on, egui::Sense::click())
    }

    fn place_row_with(&self, ui: &mut egui::Ui, glyph: glyphs::Glyph, name: &str, on: bool, sense: egui::Sense) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), sense);
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
            let (open, header) = self.section_header(ui, "Favoris");
            if open {
                self.favorites_section(ui, header.top(), &folder);
            } else {
                self.fav_rect = None;
                self.fav_rows.clear();
            }
            ui.add_space(12.0);
            let (open, _) = self.section_header(ui, "Emplacements");
            let drives: Vec<(String, PathBuf)> = if cfg!(windows) {
                ('A'..='Z').map(|c| (format!("{c}:"), PathBuf::from(format!("{c}:\\")))).filter(|(_, p)| p.is_dir()).collect()
            } else {
                vec![(root_name(), PathBuf::from("/"))]
            };
            for (name, path) in drives.into_iter().filter(|_| open) {
                if self.place_row(ui, glyphs::DRIVE, &name, folder == path).clicked() {
                    self.go(path);
                }
            }
            ui.add_space(12.0);
            let (open, _) = self.section_header(ui, "Étiquettes");
            // folded: no tag is drawn
            let tags = if open { self.tag_list() } else { Vec::new() };
            if open && tags.is_empty() {
                ui.weak("Aucune étiquette");
            }
            let mut filter = None;
            let mut fold = None;
            // the rows drawn (index in `tags`, rectangle): where a tag being moved can be dropped
            let mut visible: Vec<(usize, egui::Rect)> = Vec::new();
            let mut delete_tag: Option<i64> = None;
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
                let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click_and_drag());
                visible.push((index, rect));
                if response.drag_started() {
                    self.tag_drag = Some(tag.id);
                }
                if self.tag_drag == Some(tag.id) {
                    ui.painter().rect_filled(rect, 7.0, ui.visuals().panel_fill.gamma_multiply(0.6));
                }
                if on {
                    ui.painter().rect_filled(rect, 7.0, ui.visuals().selection.bg_fill);
                } else if response.hovered() {
                    ui.painter().rect_filled(rect, 7.0, ui.visuals().widgets.hovered.weak_bg_fill);
                }
                let x = rect.left() + 6.0 + tag.depth as f32 * 14.0;
                let color = if on { Color32::WHITE } else { ui.visuals().text_color() };
                if has_children {
                    // the arrow: its own click folds, the rest of the row shows the tag
                    let arrow = egui::Rect::from_center_size(egui::pos2(x + 7.0, rect.center().y), egui::vec2(20.0, 24.0));
                    let arrow_response = ui.interact(arrow, ui.id().with(("tag-fold", tag.id)), egui::Sense::click());
                    // a large chevron: to the right when folded, down when open
                    let chevron = if folded { glyphs::CHEVRON_RIGHT } else { glyphs::CHEVRON_DOWN };
                    glyphs::image(chevron, 16.0, color).paint_at(ui, egui::Rect::from_center_size(arrow.center(), egui::vec2(16.0, 16.0)));
                    arrow_response.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                    if arrow_response.clicked() {
                        fold = Some(tag.id);
                    }
                }
                ui.painter().circle_filled(egui::pos2(x + 20.0, rect.center().y), 5.0, tagsui::parse_color(&tag.color));
                ui.painter().text(egui::pos2(x + 34.0, rect.center().y), egui::Align2::LEFT_CENTER, &tag.name, egui::TextStyle::Body.resolve(ui.style()), color);
                // the trash, at the right: seen when the row is hovered; a first click turns it red, a second one
                // deletes the tag, and it is gray again as soon as the pointer leaves it
                let armed = self.tag_armed == Some(tag.id);
                let mut over_trash = false;
                let shown_trash = (ui.rect_contains_pointer(rect) && self.tag_drag.is_none()) || armed;
                if !shown_trash {
                    let count = self.tag_count(tag.id);
                    let tint = if on { Color32::from_white_alpha(200) } else { ui.visuals().weak_text_color() };
                    ui.painter().text(egui::pos2(rect.right() - 10.0, rect.center().y), egui::Align2::RIGHT_CENTER, count.to_string(), egui::TextStyle::Small.resolve(ui.style()), tint);
                }
                if shown_trash {
                    let trash = egui::Rect::from_center_size(egui::pos2(rect.right() - 16.0, rect.center().y), egui::vec2(24.0, 22.0));
                    let trash_response = ui.interact(trash, ui.id().with(("tag-trash", tag.id)), egui::Sense::click());
                    over_trash = trash_response.hovered();
                    let tint = if armed {
                        Color32::from_rgb(230, 60, 60)
                    } else if on {
                        Color32::from_white_alpha(200)
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    if armed || over_trash {
                        ui.painter().rect_filled(trash, 6.0, if armed { Color32::from_rgba_unmultiplied(230, 60, 60, 36) } else { Color32::from_rgba_unmultiplied(127, 127, 127, 40) });
                    }
                    glyphs::image(glyphs::by_name("trash"), 14.0, tint).paint_at(ui, egui::Rect::from_center_size(trash.center(), egui::vec2(14.0, 14.0)));
                    trash_response.clone().on_hover_text(if armed { "Cliquer encore pour supprimer" } else { "Supprimer l'étiquette" }).on_hover_cursor(egui::CursorIcon::PointingHand);
                    if trash_response.clicked() {
                        if armed {
                            delete_tag = Some(tag.id);
                        } else {
                            self.tag_armed = Some(tag.id);
                        }
                    }
                }
                if armed && !over_trash {
                    self.tag_armed = None;
                }
                if response.clicked() && fold != Some(tag.id) && !over_trash {
                    filter = Some(tag.id);
                }
            }
            self.tag_dragging(ui, &tags, &visible);
            if let Some(id) = delete_tag {
                if let Some(store) = self.tags.as_mut() {
                    store.delete(id);
                }
                self.tag_armed = None;
                if self.tag_filter == Some(id) {
                    self.set_tag_filter(None);
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

    /// Moving a tag of the sidebar with the mouse: dropped on the middle of another one it goes under it, on the
    /// upper or lower edge it goes before or after it (same level), below the last row it goes to the root
    fn tag_dragging(&mut self, ui: &mut egui::Ui, tags: &[crate::tagstore::Tag], visible: &[(usize, egui::Rect)]) {
        let Some(moving) = self.tag_drag else { return };
        let ctx = ui.ctx().clone();
        let pointer = ctx.pointer_latest_pos();
        let Some(from) = tags.iter().position(|t| t.id == moving) else {
            self.tag_drag = None;
            return;
        };
        // the tag and what is under it cannot receive it
        let end = tags[from + 1..].iter().position(|t| t.depth <= tags[from].depth).map_or(tags.len(), |n| from + 1 + n);
        let inside = |i: usize| i >= from && i < end;
        enum Drop {
            Into(usize),
            Before(usize),
            After(usize),
            RootEnd,
        }
        let mut drop = None;
        if let Some(p) = pointer.filter(|p| ui.max_rect().x_range().contains(p.x)) {
            if let Some((i, rect)) = visible.iter().find(|(_, r)| r.y_range().contains(p.y)) {
                if !inside(*i) {
                    let f = (p.y - rect.top()) / rect.height();
                    drop = Some(if f < 0.25 { Drop::Before(*i) } else if f > 0.75 { Drop::After(*i) } else { Drop::Into(*i) });
                }
            } else if visible.last().is_some_and(|(_, r)| p.y > r.bottom() && p.y < r.bottom() + 60.0) {
                drop = Some(Drop::RootEnd);
            }
        }
        if ctx.input(|i| i.pointer.primary_down()) {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            let accent = self.accent_color();
            let stroke = egui::Stroke::new(2.0, accent);
            let row = |i: usize| visible.iter().find(|(k, _)| *k == i).map(|(_, r)| *r);
            let indent = |i: usize| 6.0 + tags[i].depth as f32 * 14.0;
            match &drop {
                Some(Drop::Into(i)) => {
                    if let Some(r) = row(*i) {
                        ui.painter().rect_stroke(r, 7.0, stroke, egui::StrokeKind::Inside);
                    }
                }
                Some(Drop::Before(i)) => {
                    if let Some(r) = row(*i) {
                        ui.painter().line_segment([egui::pos2(r.left() + indent(*i), r.top()), egui::pos2(r.right() - 6.0, r.top())], stroke);
                    }
                }
                Some(Drop::After(i)) => {
                    if let Some(r) = row(*i) {
                        ui.painter().line_segment([egui::pos2(r.left() + indent(*i), r.bottom()), egui::pos2(r.right() - 6.0, r.bottom())], stroke);
                    }
                }
                Some(Drop::RootEnd) => {
                    if let Some((_, r)) = visible.last() {
                        ui.painter().line_segment([egui::pos2(r.left() + 6.0, r.bottom() + 2.0), egui::pos2(r.right() - 6.0, r.bottom() + 2.0)], stroke);
                    }
                }
                None => {}
            }
            if let Some(p) = pointer {
                // the name follows the pointer
                let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("tag-ghost"));
                ctx.layer_painter(layer).text(p + egui::vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, &tags[from].name, egui::TextStyle::Body.resolve(ui.style()), ui.visuals().text_color());
            }
            return;
        }
        // released: the tag goes where the line was
        self.tag_drag = None;
        let Some(drop) = drop else { return };
        // the next tag of the same level, after a tag and what is under it
        let after = |i: usize| {
            let stop = tags[i + 1..].iter().position(|t| t.depth <= tags[i].depth).map_or(tags.len(), |n| i + 1 + n);
            tags.get(stop).filter(|t| t.depth == tags[i].depth).map(|t| t.id)
        };
        let (parent, before) = match drop {
            Drop::Into(i) => (Some(tags[i].id), None),
            Drop::Before(i) => (tags[i].parent, Some(tags[i].id)),
            Drop::After(i) => (tags[i].parent, after(i)),
            Drop::RootEnd => (None, None),
        };
        if let Some(store) = self.tags.as_mut() {
            store.move_tag_at(moving, parent, before);
        }
    }

    /// The title of a section of the sidebar: a click folds or unfolds it, a chevron at the right shows when
    /// the title is hovered (to the right: folded, down: open). Returns whether the section is open
    fn section_header(&mut self, ui: &mut egui::Ui, title: &str) -> (bool, egui::Rect) {
        let open = !self.closed_sections.contains(title);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click());
        let weak = ui.visuals().weak_text_color();
        ui.painter().text(egui::pos2(rect.left() + 2.0, rect.center().y), egui::Align2::LEFT_CENTER, title, egui::TextStyle::Small.resolve(ui.style()), weak);
        if response.hovered() {
            let chevron = if open { glyphs::CHEVRON_DOWN } else { glyphs::CHEVRON_RIGHT };
            glyphs::image(chevron, 16.0, weak).paint_at(ui, egui::Rect::from_center_size(egui::pos2(rect.right() - 14.0, rect.center().y), egui::vec2(16.0, 16.0)));
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            if open {
                self.closed_sections.insert(title.to_owned());
            } else {
                self.closed_sections.remove(title);
            }
        }
        (open, rect)
    }

    /// The favorites (folders and files), kept in the database: a click opens it, a right click opens the menu (new window,
    /// new tab, remove), a drag reorders them, and a folder dropped on the section is added
    fn favorites_section(&mut self, ui: &mut egui::Ui, top: f32, folder: &Path) {
        enum Act {
            Open(PathBuf),
            Window(PathBuf),
            Tab(PathBuf),
            Remove(i64),
            Order(Vec<i64>),
        }
        let favorites: Vec<crate::tagstore::Favorite> = self
            .tags
            .as_ref()
            .map(|t| t.favorites())
            .unwrap_or_default()
            .into_iter()
            .filter(|f| Path::new(&f.path).exists())
            .collect();
        let mut rows = Vec::new();
        let mut act = None;
        for favorite in favorites.iter() {
            let path = PathBuf::from(&favorite.path);
            let response = self.place_row_with(ui, glyphs::by_name(&favorite.icon), &favorite.label, folder == path, egui::Sense::click_and_drag());
            rows.push(response.rect.center().y);
            if self.fav_drag == Some(favorite.id) {
                // the row being moved is faded
                ui.painter().rect_filled(response.rect, 7.0, ui.visuals().panel_fill.gamma_multiply(0.6));
            }
            if response.clicked() {
                act = Some(Act::Open(path.clone()));
            }
            if response.drag_started() {
                self.fav_drag = Some(favorite.id);
            }
            response.context_menu(|ui| {
                if ui.button("Ouvrir dans une nouvelle fenêtre").clicked() {
                    act = Some(Act::Window(path.clone()));
                    ui.close();
                }
                if ui.button("Ouvrir dans un nouvel onglet").clicked() {
                    act = Some(Act::Tab(path.clone()));
                    ui.close();
                }
                ui.separator();
                if ui.button("Retirer des favoris").clicked() {
                    act = Some(Act::Remove(favorite.id));
                    ui.close();
                }
            });
        }
        let ids: Vec<i64> = favorites.iter().map(|f| f.id).collect();
        let bottom = ui.cursor().top();
        let area = ui.max_rect();
        self.fav_rect = Some(egui::Rect::from_min_max(egui::pos2(area.left(), top), egui::pos2(area.right(), bottom.max(top + 20.0))));
        // moving a favorite with the mouse: the place is under the pointer, the drop puts it there
        if let Some(moving) = self.fav_drag {
            let pointer = ui.ctx().pointer_latest_pos();
            if ui.ctx().input(|i| i.pointer.primary_down()) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                if let Some(p) = pointer {
                    self.fav_insert = Some(rows.iter().filter(|y| **y < p.y).count());
                    if let Some(favorite) = favorites.iter().find(|f| f.id == moving) {
                        // the name follows the pointer
                        let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("fav-ghost"));
                        let painter = ui.ctx().layer_painter(layer);
                        painter.text(p + egui::vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, &favorite.label, egui::TextStyle::Body.resolve(ui.style()), ui.visuals().text_color());
                    }
                }
            } else {
                if let (Some(at), Some(from)) = (self.fav_insert, ids.iter().position(|i| *i == moving)) {
                    let mut order = ids.clone();
                    let id = order.remove(from);
                    order.insert(if at > from { at - 1 } else { at }.min(order.len()), id);
                    if order != ids {
                        act = Some(Act::Order(order));
                    }
                }
                self.fav_drag = None;
                self.fav_insert = None;
            }
        }
        // the line where the favorite goes
        if let Some(at) = self.fav_insert {
            if !rows.is_empty() {
                let y = if at == 0 {
                    rows[0] - 14.0
                } else if at >= rows.len() {
                    rows[rows.len() - 1] + 14.0
                } else {
                    (rows[at - 1] + rows[at]) / 2.0
                };
                ui.painter().line_segment([egui::pos2(area.left() + 6.0, y), egui::pos2(area.right() - 6.0, y)], egui::Stroke::new(2.0, self.accent_color()));
            }
        }
        self.fav_ids = ids;
        self.fav_rows = rows;
        match act {
            Some(Act::Open(path)) => self.open(&path),
            Some(Act::Window(path)) => self.new_window = Some(path),
            Some(Act::Tab(path)) => self.open_in_new_tab(path),
            Some(Act::Remove(id)) => {
                if let Some(store) = &mut self.tags {
                    store.remove_favorite(id);
                }
            }
            Some(Act::Order(order)) => {
                if let Some(store) = &mut self.tags {
                    store.set_favorite_order(&order);
                }
            }
            None => {}
        }
    }

    /// Folders and files dragged over the favorites are added there, at the place under the pointer; returns true
    /// when the files were for the favorites (the folder shown does not take them)
    fn favorites_drop(&mut self, ctx: &egui::Context, hovered: &[PathBuf], dropped: &[PathBuf]) -> bool {
        let pointer = pyegui_core::native_dnd::native_pointer().or_else(|| ctx.pointer_latest_pos());
        let over = pointer.zip(self.fav_rect).filter(|(p, r)| r.contains(*p));
        let Some((pointer, _)) = over else {
            if self.fav_drag.is_none() {
                self.fav_insert = None;
            }
            return false;
        };
        if hovered.is_empty() && dropped.is_empty() {
            return false;
        }
        let at = self.fav_rows.iter().filter(|y| **y < pointer.y).count();
        if !hovered.is_empty() {
            self.fav_insert = Some(at);
        }
        if !dropped.is_empty() {
            self.fav_insert = None;
            let mut order = self.fav_ids.clone();
            let mut place = at.min(order.len());
            if let Some(store) = &mut self.tags {
                for folder in dropped.iter().filter(|p| p.exists()) {
                    if let Some(id) = store.add_favorite(folder) {
                        order.insert(place, id);
                        place += 1;
                    }
                }
                store.set_favorite_order(&order);
            }
        }
        true
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
            font_ui: self.font_ui.clone(),
            font_terminal: self.font_terminal.clone(),
            font_size: self.font_size,
            terminal_font_size: self.terminal_font_size,
            column_widths: self.column_widths.clone(),
            looks: self.looks.clone(),
            accent_custom: self.accent_custom,
            show_hidden: self.show_hidden,
            blue_folders: self.blue_folders,
            dock: self.dock.as_ref().and_then(|d| serde_json::to_string(d).ok()),
            open_folders: self.open_folders.iter().cloned().collect(),
            closed_sections: self.closed_sections.iter().cloned().collect(),
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
        #[cfg(target_os = "macos")]
        pyegui_core::window::mark_window(&*frame);
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
            let filter = if background == Background::Glass { self.settled_blur(ui.ctx())[dark] } else { [0, 100, 100] };
            if filter != self.applied_filter {
                self.applied_filter = filter;
                let _ = pyegui_core::window::set_window_filter(&*frame, filter[0], filter[1] as f32 / 100.0, filter[2] as f32 / 100.0);
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
                let filter = self.settled_blur(ctx)[usize::from(dark)];
                if self.bg_texture.as_ref().is_none_or(|(p, f, _)| *p != path || *f != filter) {
                    self.bg_texture = blurred_texture(ctx, &path, filter).map(|t| (path.clone(), filter, t));
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

    /// The native glass of a second window: found among the windows of the app (its viewport gives no handle),
    /// then the blur like the first window
    #[cfg(target_os = "macos")]
    fn native_backdrop(&mut self, ctx: &egui::Context, glass: bool, dark: bool) {
        if self.native.is_none() && glass && ctx.cumulative_frame_nr() > 2 {
            if let Some(native) = pyegui_core::window::claim_new_window() {
                if let Some(name) = backdrop_name() {
                    let _ = pyegui_core::window::apply_backdrop(&native, name);
                }
                self.native = Some(native);
            }
        }
        if self.native.is_some() {
            let filter = if glass { self.settled_blur(ctx)[usize::from(dark)] } else { [0, 100, 100] };
            if filter != self.applied_filter {
                self.applied_filter = filter;
                if let Some(native) = &self.native {
                    let _ = pyegui_core::window::set_window_filter(native, filter[0], filter[1] as f32 / 100.0, filter[2] as f32 / 100.0);
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
                    // no native backdrop here (no handle of the window): the layers keep their own opacity over a
                    // light base, not over an opaque fill that hides them. A color or a picture is painted by `draw`
                    let dark = ui.visuals().dark_mode;
                    let glass = window.looks[usize::from(dark)].background == Background::Glass;
                    #[cfg(target_os = "macos")]
                    window.native_backdrop(ui.ctx(), glass, dark);
                    // the native backdrop exists on macOS only
                    #[cfg(target_os = "macos")]
                    let has_native = window.native.is_some();
                    #[cfg(not(target_os = "macos"))]
                    let has_native = false;
                    if glass && !has_native {
                        let rect = ui.ctx().content_rect();
                        let base = style::glass_base(dark, None);
                        ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 60));
                    }
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
/// the key of the rectangle of the header of a tag, which the dock leaves clear of the sheet of its pane
pub const TAG_BAND: &str = "finder-tag-band";

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

/// "Work/ Invoice" -> ["Work", "Invoice"]
fn tagsui_split(text: &str) -> Vec<String> {
    crate::tagstore::TagStore::split_path(text)
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
fn fonts(nerd: Option<&Path>, ui_font: Option<&Path>, terminal_font: Option<&Path>) -> egui::FontDefinitions {
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
    // the font chosen for the interface comes first (the titles keep Inter semibold: a file has no bold)
    if let Some(data) = ui_font.and_then(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("chosen".to_owned(), Arc::new(FontData::from_owned(data)));
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "chosen".to_owned());
        fonts.families.get_mut(&style::bold_family()).unwrap().insert(1, "chosen".to_owned());
    }
    // the font of the terminal (the one chosen, else the Nerd Font of the prompt), with the monospace
    // fonts behind it for the characters it does not have
    let mut family: Vec<String> = Vec::new();
    if let Some(data) = terminal_font.or(nerd).and_then(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("nerd".to_owned(), Arc::new(FontData::from_owned(data)));
        family.push("nerd".to_owned());
    }
    family.extend(fonts.families[&FontFamily::Monospace].clone());
    fonts.families.insert(FontFamily::Name("nerd".into()), family);
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

/// The picture, made smaller, blurred (a radius in points of the window) and with its contrast and its
/// saturation changed (percent), as a texture. `filter` is [blur, contrast, saturation]
fn blurred_texture(ctx: &egui::Context, path: &Path, filter: [u32; 3]) -> Option<egui::TextureHandle> {
    let [blur, contrast, saturation] = filter;
    let img = image::open(path).ok()?;
    let img = img.thumbnail(1280, 1280).to_rgba8();
    let mut img = if blur > 0 { image::imageops::blur(&img, blur as f32 / 2.0) } else { img };
    if contrast != 100 || saturation != 100 {
        let (c, s) = (contrast as f32 / 100.0, saturation as f32 / 100.0);
        for px in img.pixels_mut() {
            let [r, g, b] = [px[0] as f32, px[1] as f32, px[2] as f32];
            let luma = 0.299 * r + 0.587 * g + 0.114 * b;
            for (k, v) in [r, g, b].into_iter().enumerate() {
                let v = luma + (v - luma) * s;
                px[k] = ((v - 127.5) * c + 127.5).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture("finder-background", color, egui::TextureOptions::LINEAR))
}
