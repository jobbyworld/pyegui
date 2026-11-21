//! The tags on screen: the pane of the tags (tree, filter, put on the selection), the dots of the list,
//! the recent pane and the window that creates or edits a tag.

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText};

use crate::app::FinderApp;
use crate::tagstore::{How, Tag};

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

/// The window that creates a tag (`id` None) or edits one
pub struct TagEdit {
    pub id: Option<i64>,
    pub parent: Option<i64>,
    pub name: String,
    pub color: Color32,
}

/// The palette new tags start from, one color after the other
const PALETTE: [&str; 8] = ["#ef4444", "#f97316", "#eab308", "#22c55e", "#06b6d4", "#3b82f6", "#8b5cf6", "#ec4899"];

pub fn new_edit(app: &FinderApp, parent: Option<i64>) -> TagEdit {
    let count = app.tags.as_ref().map_or(0, |t| t.all().len());
    TagEdit { id: None, parent, name: String::new(), color: parse_color(PALETTE[count % PALETTE.len()]) }
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
        if ui.button("＋ Étiquette").clicked() {
            app.tag_edit = Some(new_edit(app, None));
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
    let mut edit: Option<TagEdit> = None;
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
                if ui.button("Sous-étiquette…").clicked() {
                    edit = Some(new_edit(app, Some(tag.id)));
                    ui.close();
                }
                if ui.button("Modifier…").clicked() {
                    edit = Some(TagEdit { id: Some(tag.id), parent: tag.parent, name: tag.name.clone(), color: parse_color(&tag.color) });
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
    if let Some(e) = edit {
        app.tag_edit = Some(e);
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

pub fn modal(app: &mut FinderApp, ctx: &egui::Context) {
    let Some(mut e) = app.tag_edit.take() else { return };
    let mut keep = true;
    let mut apply = false;
    egui::Modal::new(egui::Id::new("tag-edit")).show(ctx, |ui| {
        ui.set_width(300.0);
        ui.heading(if e.id.is_some() { "Modifier l'étiquette" } else { "Nouvelle étiquette" });
        let edit = ui.add(egui::TextEdit::singleline(&mut e.name).hint_text("Nom").desired_width(f32::INFINITY));
        if !edit.has_focus() && !edit.lost_focus() {
            edit.request_focus();
        }
        ui.horizontal(|ui| {
            ui.label("Couleur");
            egui::color_picker::color_edit_button_srgba(ui, &mut e.color, egui::color_picker::Alpha::Opaque);
            for hex in PALETTE {
                let c = parse_color(hex);
                if ui.add(egui::Button::new(RichText::new("●").color(c)).frame(false)).clicked() {
                    e.color = c;
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Annuler").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                keep = false;
            }
            if ui.button("OK").clicked() || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                apply = true;
            }
        });
    });
    if apply && !e.name.trim().is_empty() {
        if let Some(store) = app.tags.as_mut() {
            match e.id {
                Some(id) => {
                    store.rename(id, e.name.trim());
                    store.recolor(id, &to_hex(e.color));
                }
                None => {
                    store.create(e.name.trim(), &to_hex(e.color), e.parent);
                }
            }
        }
    } else if keep && !apply {
        app.tag_edit = Some(e);
    } else if apply {
        app.tag_edit = Some(e);
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
