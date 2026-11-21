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

use crate::actions::{self, ACTIONS};
use crate::dock::{self, Pane};
use crate::model::{self, History, SortKey};
use crate::terminals::{self, Terminals};
use crate::views;

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
    show_hidden: bool,
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
            show_hidden: false,
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
    terminals: Terminals,
    pub show_hidden: bool,
    pub search: String,
    /// the items shown, sorted and filtered; made again when the folder, the sort, the filter or the disk change
    pub items: Vec<Entry>,
    items_key: Option<(PathBuf, SortKey, bool, bool, String, u64, bool, u64)>,
    /// the depth of each item (0 except in the tree view)
    pub depths: Vec<usize>,
    pub open_folders: std::collections::BTreeSet<PathBuf>,
    pub selected: HashSet<PathBuf>,
    pub anchor: Option<usize>,
    /// asks the view to scroll to the item the keyboard just selected
    pub reveal: Option<usize>,
    path_text: Option<String>,
    pub preview_text: Option<(PathBuf, String)>,
    /// copied or cut paths (true: cut)
    pub clipboard: Option<(Vec<PathBuf>, bool)>,
    pub transfer: Transfer,
    /// a move waits here for its files, to select them
    pasted_cut: bool,
    rename: Option<(PathBuf, String)>,
    confirm_delete: Option<Vec<PathBuf>>,
    pub notice: Option<String>,
    dock: Option<egui_tiles::Tree<Pane>>,
    /// what the system told about the disk, newest last
    pub activity: std::collections::VecDeque<String>,
    select_after: Vec<PathBuf>,
}

impl FinderApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<PathBuf>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let font_path = terminals::nerd_font();
        cc.egui_ctx.set_fonts(fonts(font_path.as_deref()));
        let saved: Saved = cc
            .storage
            .and_then(|s| s.get_string(STATE_KEY))
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let home = home();
        let folder = start
            .map(|p| expand(&p))
            .filter(|p| p.is_dir())
            .or(saved.folder.filter(|p| p.is_dir()))
            .unwrap_or(home);
        let folder_for_terminal = folder.clone();
        let app = FinderApp {
            core: default_core(),
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
            terminals: Terminals::new(&folder_for_terminal, font_path.as_deref()),
            show_hidden: saved.show_hidden,
            search: String::new(),
            items: Vec::new(),
            items_key: None,
            depths: Vec::new(),
            open_folders: saved.open_folders.into_iter().collect(),
            selected: HashSet::new(),
            anchor: None,
            reveal: None,
            path_text: None,
            preview_text: None,
            clipboard: None,
            transfer: Transfer::new(),
            pasted_cut: false,
            rename: None,
            confirm_delete: None,
            notice: None,
            dock: Some(dock::restore(saved.dock.as_deref())),
            activity: Default::default(),
            select_after: Vec::new(),
        };
        app.apply_theme(&cc.egui_ctx);
        app
    }

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
        self.selected.clear();
        self.anchor = None;
        self.search.clear();
        self.path_text = None;
    }

    /// Opens a folder here, a file with the application of the system
    pub fn open(&mut self, path: &Path) {
        if path.is_dir() && !path.extension().is_some_and(|x| x == "app") {
            self.go(path.to_path_buf());
        } else {
            open_with_system(path);
        }
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
        ctx.set_theme(match self.theme {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        });
        let accent = ACCENTS[self.accent].1;
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let mut s = style::default_style(theme);
            style::accent_style(&mut s, accent);
            ctx.set_style_of(theme, s);
        }
    }

    /// The folder listed, filtered and sorted (in the tree view, with the open folders below their
    /// parent). Kept until something changes
    fn refresh_items(&mut self, ctx: &egui::Context) {
        let version = lock(&self.core).version();
        let tree = self.view == ViewMode::Tree;
        let open_hash = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            self.open_folders.hash(&mut h);
            h.finish()
        };
        let key = (self.folder().to_path_buf(), self.sort_key, self.ascending, self.show_hidden, self.search.clone(), version, tree, open_hash);
        if self.items_key.as_ref() == Some(&key) {
            return;
        }
        let mut items = Vec::new();
        let mut depths = Vec::new();
        self.collect(ctx, &self.folder().to_path_buf(), 0, tree, &mut items, &mut depths);
        self.items = items;
        self.depths = depths;
        // what disappeared from the disk is no longer selected
        let present: HashSet<&PathBuf> = self.items.iter().map(|e| &e.path).collect();
        self.selected.retain(|p| present.contains(p));
        self.items_key = Some(key);
        if !self.select_after.is_empty() {
            let wanted: HashSet<PathBuf> = std::mem::take(&mut self.select_after).into_iter().collect();
            let found: Vec<usize> = self.items.iter().enumerate().filter(|(_, e)| wanted.contains(&e.path)).map(|(i, _)| i).collect();
            if let Some(first) = found.first() {
                self.selected = found.iter().map(|i| self.items[*i].path.clone()).collect();
                self.anchor = Some(*first);
                self.reveal = Some(*first);
            }
        }
    }

    fn collect(&self, ctx: &egui::Context, folder: &Path, depth: usize, tree: bool, items: &mut Vec<Entry>, depths: &mut Vec<usize>) {
        let listing = lock(&self.core).list(Some(ctx), folder);
        let needle = self.search.to_lowercase();
        let mut level: Vec<Entry> = listing
            .iter()
            .filter(|e| (self.show_hidden || !e.hidden) && (needle.is_empty() || e.name.to_lowercase().contains(&needle) || (tree && e.is_dir)))
            .cloned()
            .collect();
        model::sort_entries(&mut level, self.sort_key, self.ascending);
        for e in level {
            let open = tree && e.is_dir && self.open_folders.contains(&e.path);
            let path = e.path.clone();
            items.push(e);
            depths.push(depth);
            if open && depth < 24 {
                self.collect(ctx, &path, depth + 1, tree, items, depths);
            }
        }
    }

    pub fn toggle_folder(&mut self, path: &Path) {
        if !self.open_folders.remove(path) {
            self.open_folders.insert(path.to_path_buf());
        }
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

    pub fn paste(&mut self) {
        let Some((paths, cut)) = self.clipboard.clone() else { return };
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
            self.select_after = done.into_iter().filter(|(folder, _)| folder == self.folder()).map(|(_, target)| target).collect();
            if self.pasted_cut && self.transfer.running() == 0 {
                self.clipboard = None;
                self.pasted_cut = false;
            }
            self.items_key = None;
        }
        if let Some(error) = self.transfer.errors().into_iter().next() {
            self.notice = Some(error);
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

    pub fn sort_by(&mut self, key: SortKey) {
        if self.sort_key == key {
            self.ascending = !self.ascending;
        } else {
            self.sort_key = key;
            self.ascending = true;
        }
    }

    /// A click on an item: Cmd or Ctrl adds or removes it, Shift extends from the anchor
    pub fn click_item(&mut self, index: usize, modifiers: Modifiers) {
        let path = self.items[index].path.clone();
        if modifiers.command {
            if !self.selected.remove(&path) {
                self.selected.insert(path);
            }
            self.anchor = Some(index);
        } else if modifiers.shift && self.anchor.is_some() {
            let (a, b) = (self.anchor.unwrap().min(index), self.anchor.unwrap().max(index));
            self.selected = self.items[a..=b].iter().map(|e| e.path.clone()).collect();
        } else {
            self.selected.clear();
            self.selected.insert(path);
            self.anchor = Some(index);
        }
    }

    fn selected_index(&self) -> Option<usize> {
        self.anchor.filter(|i| *i < self.items.len() && self.selected.contains(&self.items[*i].path))
    }

    fn keyboard(&mut self, ctx: &egui::Context, columns: usize) {
        if ctx.egui_wants_keyboard_input() || pyegui_core::terminal::terminal_focused(ctx) || self.rename.is_some() || self.confirm_delete.is_some() {
            return;
        }
        actions::run_shortcuts(self, ctx);
        let (parent, open, clear, left, right, up_key, down_key) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::Backspace),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
                i.consume_key(Modifiers::NONE, Key::ArrowLeft),
                i.consume_key(Modifiers::NONE, Key::ArrowRight),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
            )
        });
        if parent {
            self.parent();
        }
        if clear {
            self.selected.clear();
            self.search.clear();
        }
        if open {
            for path in self.selected_paths() {
                self.open(&path);
            }
        }
        let tree = self.view == ViewMode::Tree;
        if tree && (left || right) {
            // the arrows fold and unfold the folder selected
            if let Some(i) = self.selected_index() {
                let e = self.items[i].clone();
                if e.is_dir && (right != self.open_folders.contains(&e.path)) {
                    self.toggle_folder(&e.path);
                }
            }
        }
        let step: isize = if self.view == ViewMode::Icons {
            if left { -1 } else if right { 1 } else if up_key { -(columns as isize) } else if down_key { columns as isize } else { 0 }
        } else if up_key {
            -1
        } else if down_key {
            1
        } else {
            0
        };
        if step != 0 && !self.items.is_empty() {
            let current = self.selected_index().map(|i| i as isize).unwrap_or(if step > 0 { -1 } else { self.items.len() as isize });
            let next = (current + step).clamp(0, self.items.len() as isize - 1) as usize;
            self.selected.clear();
            self.selected.insert(self.items[next].path.clone());
            self.anchor = Some(next);
            self.reveal = Some(next);
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(if self.show_sidebar { "◧" } else { "◨" }).on_hover_text("Barre latérale").clicked() {
                self.show_sidebar = !self.show_sidebar;
            }
            if ui.add_enabled(self.history.can_back(), egui::Button::new("◀")).on_hover_text("Précédent (⌘[)").clicked() {
                self.back();
            }
            if ui.add_enabled(self.history.can_forward(), egui::Button::new("▶")).on_hover_text("Suivant (⌘])").clicked() {
                self.forward();
            }
            let parent = self.folder().parent().map(Path::to_path_buf);
            if ui.add_enabled(parent.is_some(), egui::Button::new("▲")).on_hover_text("Dossier parent (⌘↑)").clicked() {
                if let Some(p) = parent {
                    self.go(p);
                }
            }
            ui.separator();
            ui.selectable_value(&mut self.view, ViewMode::List, "☰ Liste");
            ui.selectable_value(&mut self.view, ViewMode::Icons, "▦ Icônes");
            ui.selectable_value(&mut self.view, ViewMode::Tree, "🗂 Arbre");
            ui.separator();
            ui.checkbox(&mut self.show_hidden, "Cachés");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("⚙", |ui| self.appearance_menu(ui));
                if ui.selectable_label(self.show_terminal, ">_").on_hover_text("Terminal (⌘T)").clicked() {
                    self.show_terminal = !self.show_terminal;
                }
                if ui.selectable_label(self.show_dock, "◨").on_hover_text("Volets (⌘J)").clicked() {
                    self.show_dock = !self.show_dock;
                }
                let edit = egui::TextEdit::singleline(&mut self.search).hint_text("Rechercher").desired_width(180.0);
                ui.add(edit);
            });
        });
    }

    pub fn appearance_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(180.0);
        ui.label(RichText::new("Thème").strong());
        let before = (self.theme, self.accent);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.theme, ThemeChoice::System, "Système");
            ui.selectable_value(&mut self.theme, ThemeChoice::Light, "Clair");
            ui.selectable_value(&mut self.theme, ThemeChoice::Dark, "Sombre");
        });
        ui.label(RichText::new("Accent").strong());
        ui.horizontal(|ui| {
            for (i, (name, color)) in ACCENTS.iter().enumerate() {
                let text = RichText::new(if self.accent == i { "●" } else { "○" }).color(*color).size(18.0);
                if ui.add(egui::Button::new(text).frame(false)).on_hover_text(*name).clicked() {
                    self.accent = i;
                }
            }
        });
        if before != (self.theme, self.accent) {
            self.apply_theme(ui.ctx());
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Terminal").strong());
        ui.add(egui::Slider::new(&mut self.terminal_opacity, 0.0..=1.0).text("Opacité"));
        ui.checkbox(&mut self.show_terminal, "Afficher le terminal (⌘T)");
    }

    fn path_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(mut text) = self.path_text.take() {
                let edit = ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
                edit.request_focus();
                if edit.lost_focus() {
                    if ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.go(expand(Path::new(text.trim())));
                    }
                } else {
                    self.path_text = Some(text);
                }
                return;
            }
            let folder = self.folder().to_path_buf();
            let mut target = None;
            let mut accumulated = PathBuf::new();
            for component in folder.components() {
                accumulated.push(component);
                let name = match component {
                    std::path::Component::RootDir => "/".to_string(),
                    other => other.as_os_str().to_string_lossy().into_owned(),
                };
                if ui.add(egui::Button::new(name).frame(false)).clicked() {
                    target = Some(accumulated.clone());
                }
                ui.label("›");
            }
            if ui.add(egui::Button::new("✎").frame(false)).on_hover_text("Saisir un chemin").clicked() {
                self.path_text = Some(folder.to_string_lossy().into_owned());
            }
            if let Some(t) = target {
                self.go(t);
            }
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(RichText::new("Favoris").weak().small());
        let home = home();
        let mut places: Vec<(String, PathBuf)> = vec![("Départ".into(), home.clone())];
        for (label, name) in [
            ("Bureau", "Desktop"),
            ("Documents", "Documents"),
            ("Téléchargements", "Downloads"),
            ("Images", "Pictures"),
            ("Musique", "Music"),
            ("Vidéos", if cfg!(target_os = "macos") { "Movies" } else { "Videos" }),
        ] {
            let path = home.join(name);
            if path.is_dir() {
                places.push((label.into(), path));
            }
        }
        if cfg!(target_os = "macos") {
            places.push(("Applications".into(), "/Applications".into()));
        }
        places.push(("Racine".into(), if cfg!(windows) { "C:\\".into() } else { "/".into() }));
        let ctx = ui.ctx().clone();
        for (label, path) in places {
            let current = self.folder() == path;
            ui.horizontal(|ui| {
                if let Some(uri) = icons::icon_uri(&ctx, &path.to_string_lossy(), 18.0) {
                    ui.add(egui::Image::new(uri).fit_to_exact_size(egui::vec2(18.0, 18.0)));
                }
                let response = ui.selectable_label(current, label);
                if response.clicked() {
                    self.go(path.clone());
                }
            });
        }
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
                if model::is_image(&e.path) {
                    let uri = format!("file://{}", e.path.to_string_lossy());
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
        ui.horizontal(|ui| {
            if self.transfer.running() > 0 {
                ui.add(egui::ProgressBar::new(self.transfer.progress() as f32).desired_width(140.0).animate(true));
                ui.weak(format!("{} en cours", self.transfer.running()));
            }
            if let Some((paths, cut)) = &self.clipboard {
                ui.weak(format!("{} {}", paths.len(), if *cut { "coupé(s)" } else { "copié(s)" }));
            }
            ui.weak(text);
        });
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
            show_hidden: self.show_hidden,
            dock: self.dock.as_ref().and_then(|d| serde_json::to_string(d).ok()),
            open_folders: self.open_folders.iter().cloned().collect(),
        };
        if let Ok(json) = serde_json::to_string(&saved) {
            storage.set_string(STATE_KEY, json);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_transfer();
        self.poll_activity();
        self.refresh_items(&ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            self.folder().file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Finder".into()),
        ));

        if self.show_sidebar {
            egui::Panel::left("sidebar").default_size(170.0).min_size(130.0).show_inside(ui, |ui| self.sidebar(ui));
        }
        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            ui.add_space(4.0);
            self.toolbar(ui);
            ui.add_space(2.0);
            self.path_bar(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("status").show_inside(ui, |ui| self.status_bar(ui));
        if self.show_terminal {
            let folder = self.folder().to_path_buf();
            self.terminals.follow(&folder);
            let opacity = self.terminal_opacity;
            egui::Panel::bottom("terminal").resizable(true).default_size(220.0).min_size(80.0).show_inside(ui, |ui| {
                self.terminals.ui(ui, &folder, opacity);
            });
        }
        if self.show_dock {
            egui::Panel::right("dock").default_size(300.0).min_size(200.0).show_inside(ui, |ui| {
                if let Some(mut tree) = self.dock.take() {
                    dock::show(self, &mut tree, ui);
                    self.dock = Some(tree);
                }
            });
        }
        let mut columns = 1;
        egui::CentralPanel::default().show_inside(ui, |ui| {
            columns = match self.view {
                ViewMode::List | ViewMode::Tree => {
                    views::list(self, ui);
                    1
                }
                ViewMode::Icons => views::icons(self, ui),
            };
        });
        self.keyboard(&ctx, columns);
        self.modals(&ctx);
        if self.transfer.running() > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
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
