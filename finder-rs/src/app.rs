//! The window: sidebar, toolbar, the folder in the middle, a preview on the right and a status bar.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, Key, Modifiers, RichText};
use pyegui_core::icons;
use pyegui_core::style;
use pyegui_core::watcher::{default_core, expand, lock, Core, Entry};
use serde::{Deserialize, Serialize};

use crate::model::{self, History, SortKey};
use crate::views;

const STATE_KEY: &str = "finder-state";
const ACCENTS: [(&str, Color32); 4] = [
    ("Bleu", Color32::from_rgb(59, 130, 246)),
    ("Violet", Color32::from_rgb(139, 92, 246)),
    ("Vert", Color32::from_rgb(34, 197, 94)),
    ("Orange", Color32::from_rgb(249, 115, 22)),
];

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    List,
    Icons,
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
    show_preview: bool,
    show_hidden: bool,
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
            show_preview: true,
            show_hidden: false,
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
    show_preview: bool,
    pub show_hidden: bool,
    pub search: String,
    /// the items shown, sorted and filtered; made again when the folder, the sort, the filter or the disk change
    pub items: Vec<Entry>,
    items_key: Option<(PathBuf, SortKey, bool, bool, String, u64)>,
    pub selected: HashSet<PathBuf>,
    pub anchor: Option<usize>,
    /// asks the view to scroll to the item the keyboard just selected
    pub reveal: Option<usize>,
    path_text: Option<String>,
    preview_text: Option<(PathBuf, String)>,
}

impl FinderApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<PathBuf>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx.set_fonts(fonts());
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
        let app = FinderApp {
            core: default_core(),
            history: History::new(folder),
            view: saved.view,
            theme: saved.theme,
            accent: saved.accent.min(ACCENTS.len() - 1),
            sort_key: saved.sort_key,
            ascending: saved.ascending,
            show_sidebar: saved.show_sidebar,
            show_preview: saved.show_preview,
            show_hidden: saved.show_hidden,
            search: String::new(),
            items: Vec::new(),
            items_key: None,
            selected: HashSet::new(),
            anchor: None,
            reveal: None,
            path_text: None,
            preview_text: None,
        };
        app.apply_theme(&cc.egui_ctx);
        app
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

    fn apply_theme(&self, ctx: &egui::Context) {
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

    /// The folder listed, filtered and sorted. Kept until something changes
    fn refresh_items(&mut self, ctx: &egui::Context) {
        let version = lock(&self.core).version();
        let key = (self.folder().to_path_buf(), self.sort_key, self.ascending, self.show_hidden, self.search.clone(), version);
        if self.items_key.as_ref() == Some(&key) {
            return;
        }
        let listing = lock(&self.core).list(Some(ctx), self.folder());
        let needle = self.search.to_lowercase();
        let mut items: Vec<Entry> = listing
            .iter()
            .filter(|e| (self.show_hidden || !e.hidden) && (needle.is_empty() || e.name.to_lowercase().contains(&needle)))
            .cloned()
            .collect();
        model::sort_entries(&mut items, self.sort_key, self.ascending);
        self.items = items;
        // what disappeared from the disk is no longer selected
        let present: HashSet<&PathBuf> = self.items.iter().map(|e| &e.path).collect();
        self.selected.retain(|p| present.contains(p));
        self.items_key = Some(key);
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
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (back, forward, up, open, parent, all, clear, left, right, up_key, down_key) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::OpenBracket),
                i.consume_key(Modifiers::COMMAND, Key::CloseBracket),
                i.consume_key(Modifiers::COMMAND, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter) || i.consume_key(Modifiers::COMMAND, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Backspace),
                i.consume_key(Modifiers::COMMAND, Key::A),
                i.consume_key(Modifiers::NONE, Key::Escape),
                i.consume_key(Modifiers::NONE, Key::ArrowLeft),
                i.consume_key(Modifiers::NONE, Key::ArrowRight),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
            )
        });
        if back {
            self.history.back();
            self.after_navigation();
        }
        if forward {
            self.history.forward();
            self.after_navigation();
        }
        if up || parent {
            if let Some(p) = self.folder().parent().map(Path::to_path_buf) {
                self.go(p);
            }
        }
        if all {
            self.selected = self.items.iter().map(|e| e.path.clone()).collect();
        }
        if clear {
            self.selected.clear();
            self.search.clear();
        }
        if open {
            for path in self.selected.clone() {
                self.open(&path);
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
                self.history.back();
                self.after_navigation();
            }
            if ui.add_enabled(self.history.can_forward(), egui::Button::new("▶")).on_hover_text("Suivant (⌘])").clicked() {
                self.history.forward();
                self.after_navigation();
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
            ui.separator();
            ui.checkbox(&mut self.show_hidden, "Cachés");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("⚙", |ui| self.appearance_menu(ui));
                if ui.selectable_label(self.show_preview, "ⓘ").on_hover_text("Aperçu").clicked() {
                    self.show_preview = !self.show_preview;
                }
                let edit = egui::TextEdit::singleline(&mut self.search).hint_text("Rechercher").desired_width(180.0);
                ui.add(edit);
            });
        });
    }

    fn appearance_menu(&mut self, ui: &mut egui::Ui) {
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

    fn preview(&mut self, ui: &mut egui::Ui) {
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
        ui.horizontal(|ui| ui.weak(text));
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
            show_preview: self.show_preview,
            show_hidden: self.show_hidden,
        };
        if let Ok(json) = serde_json::to_string(&saved) {
            storage.set_string(STATE_KEY, json);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
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
        if self.show_preview {
            egui::Panel::right("preview").default_size(240.0).min_size(160.0).show_inside(ui, |ui| self.preview(ui));
        }
        let mut columns = 1;
        egui::CentralPanel::default().show_inside(ui, |ui| {
            columns = match self.view {
                ViewMode::List => {
                    views::list(self, ui);
                    1
                }
                ViewMode::Icons => views::icons(self, ui),
            };
        });
        self.keyboard(&ctx, columns);
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
fn fonts() -> egui::FontDefinitions {
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
    fonts
}
