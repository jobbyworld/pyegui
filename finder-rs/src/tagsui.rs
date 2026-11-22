//! The tags on screen: the pane of the tags (tree, filter, put on the selection), the dots of the list,
//! the recent pane and the window that creates or edits a tag.

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText};

use crate::app::FinderApp;
use crate::tagstore::{How, Tag, TagStore, NEW_TAG_COLORS};

pub fn parse_color(hex: &str) -> Color32 {
    let t = hex.trim_start_matches('#');
    match u32::from_str_radix(t, 16) {
        Ok(v) if t.len() == 6 => Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8),
        _ => Color32::GRAY,
    }
}

pub fn to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// The window that manages the tags: the tree, and the name, color and parent of the tag chosen
#[derive(Default)]
pub struct TagManager {
    selected: Option<i64>,
    /// the tag whose values are in the fields
    loaded: Option<i64>,
    name: String,
    color: Color32,
    parent: Option<i64>,
}

/// The window that puts tags on the selection: search or type a path (`a/b/c`) to create it
#[derive(Default)]
pub struct TagPicker {
    query: String,
    shown_query: String,
    index: usize,
    first: bool,
}

impl TagPicker {
    pub fn new() -> Self {
        TagPicker { first: true, ..Default::default() }
    }
}

/// What a menu asks for, read by `modals` (the menus have no access to the app)
#[derive(Clone, Copy, PartialEq, Default)]
pub enum Ask {
    #[default]
    Manager,
    Picker,
}

pub fn ask(ctx: &egui::Context, what: Ask) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("tag-ask"), what));
}

/// The dots of an item: the color and the name of each tag, strongest first
pub fn dots(ui: &mut egui::Ui, tags: &[(Color32, String, How)]) {
    for (color, name, how) in tags.iter().take(6) {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        let center = rect.center();
        match how {
            How::Own => ui.painter().circle_filled(center, 4.5, *color),
            // inherited: a ring
            _ => ui.painter().circle_stroke(center, 4.0, egui::Stroke::new(1.5, *color)),
        };
        response.on_hover_text(match how {
            How::Own => name.clone(),
            How::Folder => format!("{name} (dossier)"),
            How::Parent => format!("{name} (parent)"),
        });
    }
}

pub fn pane(app: &mut FinderApp, ui: &mut egui::Ui) {
    if app.tags.is_none() {
        ui.weak("Base des étiquettes indisponible");
        return;
    }
    ui.horizontal(|ui| {
        if ui.button("Gérer…").clicked() {
            app.tag_manager = Some(TagManager::default());
        }
        if ui.add_enabled(!app.selected.is_empty(), egui::Button::new("Étiqueter…")).clicked() {
            app.tag_picker = Some(TagPicker::new());
        }
        if app.tag_filter.is_some() && ui.button("Tout afficher").clicked() {
            app.set_tag_filter(None);
        }
    });
    ui.add_space(4.0);
    let tags: Vec<Tag> = app.tags.as_ref().map(|t| t.all()).unwrap_or_default();
    if tags.is_empty() {
        ui.weak("Aucune étiquette. Créez-en une, puis cochez-la sur la sélection.");
    }
    let selection = app.selected_paths();
    // for each tag: how many of the selected items carry it themselves
    let own: Vec<std::collections::HashSet<i64>> = app.tags.as_ref().map(|t| selection.iter().map(|p| t.own(p)).collect()).unwrap_or_default();
    let mut toggle: Option<(i64, bool)> = None;
    let mut filter: Option<i64> = None;
    let mut manage: Option<Option<i64>> = None;
    let mut delete: Option<i64> = None;
    for tag in &tags {
        let on = own.iter().filter(|set| set.contains(&tag.id)).count();
        ui.horizontal(|ui| {
            ui.add_space(tag.depth as f32 * 14.0);
            let state = if selection.is_empty() { "○" } else if on == selection.len() { "●" } else if on > 0 { "◐" } else { "○" };
            let check = ui.add_enabled(!selection.is_empty(), egui::Button::new(RichText::new(state).color(parse_color(&tag.color))).frame(false));
            if check.on_hover_text("Mettre sur la sélection / retirer").clicked() {
                toggle = Some((tag.id, on != selection.len()));
            }
            let current = app.tag_filter == Some(tag.id);
            let label = ui.selectable_label(current, RichText::new(&tag.name));
            if label.clicked() {
                filter = Some(tag.id);
            }
            label.context_menu(|ui| {
                if ui.button("Modifier…").clicked() {
                    manage = Some(Some(tag.id));
                    ui.close();
                }
                if ui.button("Supprimer").clicked() {
                    delete = Some(tag.id);
                    ui.close();
                }
            });
        });
    }
    if let Some((id, add)) = toggle {
        if let Some(store) = app.tags.as_mut() {
            for path in &selection {
                if add { store.tag(path, id) } else { store.untag(path, id) }
            }
        }
    }
    if let Some(id) = filter {
        let next = if app.tag_filter == Some(id) { None } else { Some(id) };
        app.set_tag_filter(next);
    }
    if let Some(selected) = manage {
        app.tag_manager = Some(TagManager { selected, ..Default::default() });
    }
    if let Some(id) = delete {
        if let Some(store) = app.tags.as_mut() {
            store.delete(id);
        }
        if app.tag_filter == Some(id) {
            app.set_tag_filter(None);
        }
    }
}

/// Draws the windows asked for (manager, picker) and opens those the menus asked for
pub fn modal(app: &mut FinderApp, ctx: &egui::Context) {
    match ctx.data_mut(|d| d.remove_temp::<Ask>(egui::Id::new("tag-ask"))) {
        Some(Ask::Manager) => app.tag_manager = Some(TagManager::default()),
        Some(Ask::Picker) if !app.selected.is_empty() => app.tag_picker = Some(TagPicker::new()),
        _ => {}
    }
    manager_modal(app, ctx);
    picker_modal(app, ctx);
}

fn manager_modal(app: &mut FinderApp, ctx: &egui::Context) {
    let Some(mut m) = app.tag_manager.take() else { return };
    let Some(store) = app.tags.as_mut() else { return };
    let mut close = false;
    let mut deleted: Option<i64> = None;
    egui::Modal::new(egui::Id::new("tag-manager")).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.heading("Étiquettes");
        ui.weak("Une étiquette s'applique à un élément et à son contenu, et ajoute ses étiquettes parentes.");
        ui.add_space(4.0);
        let all = store.all();
        egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
            for tag in &all {
                ui.horizontal(|ui| {
                    ui.add_space(tag.depth as f32 * 14.0);
                    ui.label(RichText::new("●").color(parse_color(&tag.color)));
                    if ui.selectable_label(m.selected == Some(tag.id), &tag.name).clicked() {
                        m.selected = Some(tag.id);
                    }
                });
            }
        });
        ui.separator();
        let tag = m.selected.and_then(|id| all.iter().find(|t| t.id == id)).cloned();
        if tag.is_none() {
            m.selected = None;
        }
        if let Some(tag) = &tag {
            if m.loaded != Some(tag.id) {
                m.loaded = Some(tag.id);
                m.name = tag.name.clone();
                m.color = parse_color(&tag.color);
                m.parent = tag.parent;
            }
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut m.name).hint_text("Nom").desired_width(220.0));
                egui::color_picker::color_edit_button_srgba(ui, &mut m.color, egui::color_picker::Alpha::Opaque);
                for hex in NEW_TAG_COLORS {
                    if ui.add(egui::Button::new(RichText::new("●").color(parse_color(hex))).frame(false)).clicked() {
                        m.color = parse_color(hex);
                    }
                }
            });
            let under = store.subtree(tag.id);
            let label = |id: Option<i64>| id.and_then(|i| all.iter().find(|t| t.id == i)).map_or("(racine)".to_string(), |t| t.name.clone());
            egui::ComboBox::from_label("Parent").selected_text(label(m.parent)).show_ui(ui, |ui| {
                ui.selectable_value(&mut m.parent, None, "(racine)");
                for t in all.iter().filter(|t| !under.contains(&t.id)) {
                    ui.selectable_value(&mut m.parent, Some(t.id), format!("{}{}", "  ".repeat(t.depth), t.name));
                }
            });
            // the fields are written at once, like finder.py
            let name = m.name.trim();
            if !name.is_empty() && name != tag.name {
                store.rename(tag.id, name);
            }
            if to_hex(m.color) != tag.color.to_lowercase() {
                store.recolor(tag.id, &to_hex(m.color));
            }
            if m.parent != tag.parent {
                store.move_tag(tag.id, m.parent);
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Nouvelle étiquette").clicked() {
                let color = NEW_TAG_COLORS[all.len() % NEW_TAG_COLORS.len()];
                m.selected = store.create("Nouvelle étiquette", color, None);
            }
            if let Some(tag) = &tag {
                if ui.button("Nouvelle sous-étiquette").clicked() {
                    let color = NEW_TAG_COLORS[all.len() % NEW_TAG_COLORS.len()];
                    m.selected = store.create("Nouvelle étiquette", color, Some(tag.id));
                }
                if ui.button("Supprimer").clicked() {
                    store.delete(tag.id);
                    deleted = Some(tag.id);
                    m.selected = None;
                }
            }
            if ui.button("Fermer").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }
        });
    });
    if let Some(id) = deleted {
        if app.tag_filter == Some(id) {
            app.set_tag_filter(None);
        }
    }
    if !close {
        app.tag_manager = Some(m);
    }
}

enum Row {
    Tag(Tag, Vec<String>),
    Create(Vec<String>),
}

fn picker_modal(app: &mut FinderApp, ctx: &egui::Context) {
    let Some(mut p) = app.tag_picker.take() else { return };
    let paths = app.selected_paths();
    let Some(store) = app.tags.as_mut() else { return };
    if paths.is_empty() {
        return;
    }
    let mut close = false;
    egui::Modal::new(egui::Id::new("tag-picker")).show(ctx, |ui| {
        ui.set_width(440.0);
        let names: Vec<String> = paths.iter().take(3).map(|q| q.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).collect();
        ui.weak(format!("Étiqueter « {}{} »", names.join(", "), if paths.len() > 3 { " …" } else { "" }));
        let edit = ui.add(egui::TextEdit::singleline(&mut p.query).hint_text("Chercher ou créer une étiquette (a/b/c)").desired_width(f32::INFINITY));
        edit.request_focus();
        let parts = TagStore::split_path(&p.query);
        let mut rows: Vec<Row> = if parts.is_empty() { store.all() } else { store.matching(&parts) }
            .into_iter()
            .map(|t| {
                let path = store.tag_path(t.id);
                Row::Tag(t, path)
            })
            .collect();
        if store.can_create(&parts) {
            rows.push(Row::Create(parts.clone()));
        }
        if p.query != p.shown_query {
            p.shown_query = p.query.clone();
            p.index = 0;
        }
        p.index = p.index.min(rows.len().saturating_sub(1));
        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            p.index = (p.index + 1).min(rows.len().saturating_sub(1));
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            p.index = p.index.saturating_sub(1);
        }
        let own: Vec<std::collections::HashSet<i64>> = paths.iter().map(|q| store.own(q)).collect();
        let mut activate: Option<usize> = None;
        // the Enter that opened the window is not the one of a choice
        if !p.first && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !rows.is_empty() {
            activate = Some(p.index);
        }
        p.first = false;
        ui.add_space(4.0);
        egui::ScrollArea::vertical().max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
            for (i, row) in rows.iter().enumerate() {
                let response = match row {
                    Row::Tag(tag, path) => {
                        let on = own.iter().filter(|set| set.contains(&tag.id)).count();
                        let state = if on == own.len() { "☑" } else if on > 0 { "◪" } else { "☐" };
                        ui.horizontal(|ui| {
                            ui.add_space(if parts.is_empty() { tag.depth as f32 * 14.0 } else { 0.0 });
                            ui.label(RichText::new("●").color(parse_color(&tag.color)));
                            let text = if parts.is_empty() { tag.name.clone() } else { path.join(" / ") };
                            ui.selectable_label(i == p.index, format!("{state} {text}"))
                        })
                        .inner
                    }
                    Row::Create(parts) => ui.selectable_label(i == p.index, format!("＋ Créer « {} »", parts.join(" / "))),
                };
                if response.clicked() {
                    activate = Some(i);
                }
            }
            if rows.is_empty() {
                ui.weak("Aucune étiquette");
            }
        });
        ui.add_space(4.0);
        if ui.button("Fermer").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if let Some(i) = activate {
            match &rows[i] {
                Row::Create(parts) => {
                    if let Some(leaf) = store.create_path(parts) {
                        for q in &paths {
                            store.tag(q, leaf);
                        }
                    }
                    p.query.clear();
                }
                Row::Tag(tag, _) => {
                    let all_on = own.iter().all(|set| set.contains(&tag.id));
                    for q in &paths {
                        if all_on { store.untag(q, tag.id) } else { store.tag(q, tag.id) }
                    }
                }
            }
        }
    });
    if !close {
        app.tag_picker = Some(p);
    }
}

/// The items opened last, the latest first
pub fn recent_pane(app: &mut FinderApp, ui: &mut egui::Ui) {
    let recent = app.tags.as_ref().map(|t| t.recently_opened(40)).unwrap_or_default();
    if recent.is_empty() {
        ui.weak("Rien d'ouvert pour l'instant");
    }
    let mut open: Option<PathBuf> = None;
    for (path, when) in recent {
        let path = PathBuf::from(path);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string_lossy().into_owned());
        ui.horizontal(|ui| {
            let label = ui.add(egui::Button::new(name).frame(false)).on_hover_text(path.to_string_lossy());
            if label.clicked() {
                open = Some(path.clone());
            }
            ui.weak(crate::model::format_date(Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(when))));
        });
    }
    if let Some(path) = open {
        app.reveal_path(&path);
    }
}

/// What the "Étiquettes" submenu of the context menu shows: every tag, and whether all the items
/// selected own it. Only read while a menu is open
pub fn menu_tags(app: &FinderApp, ctx: &egui::Context) -> Vec<(Tag, bool)> {
    let Some(store) = &app.tags else { return Vec::new() };
    if !egui::Popup::is_any_open(ctx) {
        return Vec::new();
    }
    let selected = app.selected_paths();
    let owned: Vec<_> = selected.iter().map(|p| store.own(p)).collect();
    store
        .all()
        .into_iter()
        .map(|tag| {
            let all = !owned.is_empty() && owned.iter().all(|o| o.contains(&tag.id));
            (tag, all)
        })
        .collect()
}

/// The submenu: a click puts the tag on the selection, or takes it off when all have it.
/// Returns the tag toggled and whether it is now on
pub fn menu(ui: &mut egui::Ui, tags: &[(Tag, bool)], enabled: bool) -> Option<(i64, bool)> {
    let mut chosen = None;
    ui.add_enabled_ui(enabled, |ui| {
        ui.menu_button("🏷  Étiquettes", |ui| {
            if tags.is_empty() {
                ui.weak("Aucune étiquette");
            }
            if ui.button("Chercher ou créer…").clicked() {
                ask(ui.ctx(), Ask::Picker);
                ui.close();
            }
            if ui.button("Gérer les étiquettes…").clicked() {
                ask(ui.ctx(), Ask::Manager);
                ui.close();
            }
            ui.separator();
            for (tag, on) in tags {
                ui.horizontal(|ui| {
                    ui.add_space(tag.depth as f32 * 14.0);
                    let text = RichText::new(format!("{} {}", if *on { "☑" } else { "☐" }, tag.name)).color(parse_color(&tag.color));
                    if ui.button(text).clicked() {
                        chosen = Some((tag.id, !*on));
                        ui.close();
                    }
                });
            }
        });
    });
    chosen
}
