//! The tags on screen: the pane of the tags (tree, filter, put on the selection), the dots of the list,
//! the recent pane and the window that creates or edits a tag.

use std::path::PathBuf;

use eframe::egui::{self, Color32};

use crate::app::FinderApp;
use crate::tagstore::{How, Tag, TagStore};

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

/// The tags of the store as paths ("Work/Invoice"), with the colors of the levels: what the widget of the
/// tags needs to draw the cell of a row
#[derive(Default)]
pub struct Library {
    ids: std::collections::HashMap<i64, (String, usize)>,
    pub colors: std::collections::HashMap<String, Color32>,
    /// every path, for the list of the field
    pub paths: Vec<String>,
}

impl Library {
    pub fn read(all: &[Tag]) -> Self {
        let mut library = Library::default();
        for (order, tag) in all.iter().enumerate() {
            let mut names = vec![];
            let mut at = Some(tag.id);
            while let Some(t) = at.and_then(|id| all.iter().find(|t| t.id == id)) {
                names.push(t.name.replace('/', "-"));
                at = t.parent;
                if names.len() > 64 {
                    break;
                }
            }
            names.reverse();
            let path = names.join("/");
            library.colors.insert(path.clone(), parse_color(&tag.color));
            library.paths.push(path.clone());
            library.ids.insert(tag.id, (path, order));
        }
        library
    }

    pub fn path(&self, id: i64) -> Option<String> {
        self.ids.get(&id).map(|(path, _)| path.clone())
    }

    pub fn order(&self, id: i64) -> usize {
        self.ids.get(&id).map_or(usize::MAX, |(_, order)| *order)
    }
}

/// The tags of a row as paths: all of them, and those that come from a folder
#[derive(Default)]
pub struct RowTags {
    pub paths: Vec<String>,
    pub inherited: Vec<String>,
}

/// What the widget of the tags changed in the cell of a row
pub struct TagEdit {
    pub path: PathBuf,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub colors: Vec<(String, Color32)>,
}

/// The cell of the tags of a row, the widget of pyegui: small chips (the last level of each tag, the full
/// path on hover), those of the folders desaturated, a cross on the own ones and a field to add one while the
/// mouse is over the cell
pub fn cell(ui: &mut egui::Ui, app: &FinderApp, path: &std::path::Path) {
    use pyegui_core::tags::{tags_edit, Tags, TagsOptions};
    let row = app.row_tag_paths(path);
    let library = app.library();
    let mut tags = Tags::new(row.paths.clone(), library.colors.clone());
    let before: Vec<String> = tags.tags.clone();
    let mut options = TagsOptions::new(egui::Id::unique(("finder-tags", path)));
    options.small = true;
    options.icon = true;
    options.tree = true;
    options.fit = true;
    options.field_width = 64.0;
    options.hint_text = "Chercher ou créer un tag".into();
    options.list_height = 360.0;
    options.on_hover = true;
    options.dropdown = true;
    options.suggestions = library.paths.clone();
    options.muted = row.inherited.iter().cloned().collect();
    options.editable = app.tags.is_some();
    if tags_edit(ui, &mut tags, &options) {
        let added = tags.tags.iter().filter(|t| !before.contains(t)).cloned().collect();
        let removed = before.iter().filter(|t| !tags.tags.contains(t)).cloned().collect();
        let colors = tags.colors.iter().filter(|(p, c)| library.colors.get(*p) != Some(*c)).map(|(p, c)| (p.clone(), *c)).collect();
        app.tag_edits.borrow_mut().push(TagEdit { path: path.to_path_buf(), added, removed, colors });
    }
}

/// Writes what the cells changed to the store
pub fn apply_edits(app: &mut FinderApp) {
    let edits = std::mem::take(&mut *app.tag_edits.borrow_mut());
    let Some(store) = app.tags.as_mut() else { return };
    for edit in edits {
        for added in &edit.added {
            if let Some(id) = store.create_path(&TagStore::split_path(added)) {
                store.tag(&edit.path, id);
            }
        }
        for removed in &edit.removed {
            if let Some(id) = store.find_path(&TagStore::split_path(removed)) {
                store.untag(&edit.path, id);
            }
        }
        for (path, color) in &edit.colors {
            if let Some(id) = store.find_path(&TagStore::split_path(path)) {
                store.recolor(id, &to_hex(*color));
            }
        }
    }
}

/// The pane of the tags, like the one of examples/finder.py: a field to search or create a tag, then
/// the tree of tags, each with the state for the selected items: on (all of them have it), off
/// (none) or mixed (some have it)
pub fn pane(app: &mut FinderApp, ui: &mut egui::Ui) {
    if app.tags.is_none() {
        ui.weak("Base des étiquettes indisponible");
        return;
    }
    let selection = app.selected_paths();
    let edit = ui.add(egui::TextEdit::singleline(&mut app.tags_query).hint_text("Chercher ou créer un tag").desired_width(f32::INFINITY));
    let parts = TagStore::split_path(&app.tags_query);
    let searching = !parts.is_empty();
    let (shown, creating, own, inherited) = {
        let store = app.tags.as_ref().unwrap();
        let shown: Vec<Tag> = if searching { store.matching(&parts) } else { store.all() };
        let own: Vec<std::collections::HashSet<i64>> = selection.iter().map(|p| store.own(p)).collect();
        // the tags a single selected item has through its folders or its parents
        let inherited: std::collections::HashSet<i64> = if selection.len() == 1 {
            store.effective(&selection[0]).into_iter().filter(|(_, how)| *how != How::Own).map(|(id, _)| id).collect()
        } else {
            Default::default()
        };
        (shown, store.can_create(&parts), own, inherited)
    };
    ui.weak(match selection.len() {
        0 => "Sélectionne des éléments pour les taguer".to_string(),
        1 => selection[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        n => format!("{n} éléments sélectionnés"),
    });
    ui.add_space(4.0);

    let mut toggle: Option<i64> = None;
    let mut create = false;
    // Enter in the field: the first tag found goes on the selection, or the tag is created
    if searching && edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        if let Some(first) = shown.first() {
            if !selection.is_empty() {
                toggle = Some(first.id);
            }
        } else if creating {
            create = true;
        }
        edit.request_focus();
    }
    let accent = ui.visuals().selection.bg_fill;
    let text = ui.visuals().text_color();
    // not searching: the tree of the sidebar (fold, move with the mouse, trash) with a check box per tag; searching: a flat list
    if !searching {
        app.tag_tree(ui, crate::app::TagTreeMode::Pick, &shown);
    }
    for tag in shown.iter().filter(|_| searching) {
        let on = own.iter().filter(|set| set.contains(&tag.id)).count();
        let state = if selection.is_empty() || on == 0 { "off" } else if on == selection.len() { "on" } else { "mixed" };
        ui.horizontal(|ui| {
            ui.add_space(if searching { 0.0 } else { tag.depth as f32 * 14.0 });
            let tint = if state == "off" { text } else { accent };
            let check = ui.add(egui::Button::image(crate::glyphs::image(crate::glyphs::by_name(&format!("checkbox-{state}")), 16.0, tint)).frame(false));
            if !selection.is_empty() && check.clicked() {
                toggle = Some(tag.id);
            }
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            pyegui_core::tags::paint_tag_icon(ui, rect.center(), 14.0, parse_color(&tag.color));
            let name = if searching { app.tags.as_ref().map(|s| s.tag_path(tag.id).join(" / ")).unwrap_or_default() } else { tag.name.clone() };
            if ui.selectable_label(false, name).clicked() && !selection.is_empty() {
                toggle = Some(tag.id);
            }
            if inherited.contains(&tag.id) {
                ui.weak("hérité");
            }
        });
    }
    if shown.is_empty() && !creating {
        ui.weak(if searching { "Aucun tag trouvé" } else { "Aucun tag : tape un nom pour en créer un" });
    }
    if creating && ui.button(format!("+ Créer le tag « {} »", parts.join(" / "))).clicked() {
        create = true;
    }

    if let Some(id) = toggle {
        let all_have = !selection.is_empty() && own.iter().all(|set| set.contains(&id));
        if let Some(store) = app.tags.as_mut() {
            for path in &selection {
                if all_have { store.untag(path, id) } else { store.tag(path, id) }
            }
        }
    }
    if create {
        if let Some(store) = app.tags.as_mut() {
            if let Some(leaf) = store.create_path(&parts) {
                for path in &selection {
                    store.tag(path, leaf);
                }
            }
        }
        app.tags_query.clear();
    }
}

/// How many items the Récents pane loads at first, and with each « Charger plus »
pub const RECENT_PAGE: usize = 30;

/// The items opened last, the latest first, as cards; the last card loads more
pub fn recent_pane(app: &mut FinderApp, ui: &mut egui::Ui) {
    let limit = app.recent_limit;
    let recent = app.tags.as_ref().map(|t| t.recently_opened(limit)).unwrap_or_default();
    if recent.is_empty() {
        ui.weak("Rien d'ouvert pour l'instant");
    }
    let more = recent.len() >= limit; // the base may hold older ones
    let now = crate::cards::now();
    let mut open: Option<PathBuf> = None;
    for (path, when) in recent {
        let path = PathBuf::from(path);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string_lossy().into_owned());
        let parent = path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let parent = match std::env::var("HOME") {
            Ok(home) if parent.starts_with(&home) => parent.replacen(&home, "~", 1),
            _ => parent,
        };
        let card = crate::cards::Card { path: &path, is_dir: path.is_dir(), name: &name, date: crate::cards::human_date(when, now), lines: vec![parent] };
        let response = crate::cards::show(ui, &card, app.selected.contains(&path), None).on_hover_text(path.to_string_lossy());
        if response.clicked() {
            open = Some(path.clone());
        }
    }
    if more && crate::cards::empty(ui, "Charger plus").clicked() {
        app.recent_limit += RECENT_PAGE;
    }
    if let Some(path) = open {
        app.reveal_path(&path);
    }
}

