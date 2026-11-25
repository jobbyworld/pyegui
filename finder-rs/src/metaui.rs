//! The meta properties on screen: the pane "Propriétés" (see and edit the values of the selected item) and
//! the window that manages the definitions (name, type, description, and the options of a list).

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, NaiveTime};
use eframe::egui::{self, Color32, RichText};
use pyegui_core::columns::Value;
use pyegui_core::date_input::parse_date;
use pyegui_core::human_date::human_full_date;
use pyegui_core::meta::{eval_formula, file_var, decode_ids, display, encode_ids, resolve, resolved_column_value, resolved_text, ListMode, MetaDef, MetaType, Resolved};
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::app::FinderApp;
use crate::glyphs;
use crate::tagsui::{parse_color, to_hex};

/// What the pane and the window keep between two frames
#[derive(Default)]
pub struct MetaUi {
    pub manager: Option<Manager>,
    /// the item whose values the pane shows, and the text being typed for each property
    path: Option<PathBuf>,
    buffers: HashMap<i64, String>,
    new_option: HashMap<i64, String>,
}

#[derive(Default)]
pub struct Manager {
    selected: Option<i64>,
    /// the property being edited, as read when it was chosen
    draft: Option<MetaDef>,
    new_name: String,
    new_type: usize,
    new_option: String,
    /// an option waiting for the answer to "delete it?" (with the number of items that hold it)
    confirm_option: Option<i64>,
    confirm_property: bool,
}

/// The properties of the store, the own values of those that are shown, and what each row resolves to
/// (own, inherited or summary): read once per version of the store, then looked up while the rows are drawn
#[derive(Default)]
pub struct MetaCache {
    version: u64,
    defs: Rc<Vec<MetaDef>>,
    values: HashMap<i64, Rc<BTreeMap<String, String>>>,
    resolved: HashMap<(i64, PathBuf), Rc<Resolved>>,
    computed: HashMap<(i64, PathBuf), Rc<String>>,
}

/// A day as the columns write it
pub fn date_text(seconds: f64) -> String {
    seconds_day(seconds).map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default()
}

impl FinderApp {
    fn meta_refresh(&self) -> std::cell::RefMut<'_, MetaCache> {
        let mut cache = self.meta_cache.borrow_mut();
        let version = self.tags.as_ref().map_or(0, |s| s.version) + 1;
        if cache.version != version {
            *cache = MetaCache { version, defs: Rc::new(self.tags.as_ref().map(|s| s.meta_defs()).unwrap_or_default()), ..Default::default() };
        }
        cache
    }

    /// The properties, in their order
    pub fn meta_defs(&self) -> Rc<Vec<MetaDef>> {
        self.meta_refresh().defs.clone()
    }

    /// What a property is for a path: its own value, the one of a folder around it, or the summary of a folder
    pub fn meta_resolved(&self, def: &MetaDef, path: &std::path::Path) -> Rc<Resolved> {
        let mut cache = self.meta_refresh();
        if let Some(found) = cache.resolved.get(&(def.id, path.to_path_buf())) {
            return found.clone();
        }
        let Some(store) = &self.tags else { return Rc::new(Resolved::None) };
        let values = cache.values.entry(def.id).or_insert_with(|| Rc::new(store.meta_all(def.id))).clone();
        let key = crate::tagstore::key(path);
        let resolved = Rc::new(resolve(&values, &key, std::path::MAIN_SEPARATOR, || path.is_dir()));
        cache.resolved.insert((def.id, path.to_path_buf()), resolved.clone());
        resolved
    }
}

impl FinderApp {
    /// The text of a formula property for a path: the template with the variables of the item and the
    /// values of the other properties (a formula may use another one, down to a few levels)
    pub fn meta_formula(&self, def: &MetaDef, path: &std::path::Path) -> Rc<String> {
        self.formula_at(def, path, 0)
    }

    fn formula_at(&self, def: &MetaDef, path: &std::path::Path, depth: u8) -> Rc<String> {
        if depth > 4 {
            return Rc::new(String::new());
        }
        if let Some(found) = self.meta_refresh().computed.get(&(def.id, path.to_path_buf())) {
            if depth == 0 {
                return found.clone();
            }
        }
        let defs = self.meta_defs();
        let text = eval_formula(&def.config.formula, &|name| {
            if let Some(v) = file_var(path, name) {
                return Some(v);
            }
            let other = defs.iter().find(|d| d.id != def.id && d.name.to_lowercase() == name.to_lowercase())?;
            Some(if other.ty == MetaType::Formula {
                (*self.formula_at(other, path, depth + 1)).clone()
            } else {
                resolved_text(other, &self.meta_resolved(other, path), &date_text)
            })
        });
        let text = Rc::new(text);
        if depth == 0 {
            self.meta_refresh().computed.insert((def.id, path.to_path_buf()), text.clone());
        }
        text
    }
}

/// The columns of the properties shown: a title (the name of the property) and how a row fills it
pub fn extras<'a>(app: &'a FinderApp, reserved: &[&str]) -> Vec<pyegui_core::files::Extra<'a>> {
    let defs = app.meta_defs();
    defs.iter()
        .filter(|d| app.meta_columns.contains(&d.id))
        .map(|d| {
            let title = if reserved.contains(&d.name.as_str()) { format!("{} (propriété)", d.name) } else { d.name.clone() };
            let (for_cell, for_value) = (d.clone(), d.clone());
            if d.ty == MetaType::Formula {
                return pyegui_core::files::Extra {
                    title,
                    width: 160.0,
                    cell: Box::new(move |ui, path| {
                        ui.add(egui::Label::new(&*app.meta_formula(&for_cell, path)).truncate().selectable(false));
                    }),
                    ty: d.ty.data_type(),
                    value: Some(Box::new(move |path| {
                        let text = app.meta_formula(&for_value, path);
                        if text.is_empty() { Value::None } else { Value::Text((*text).clone()) }
                    })),
                };
            }
            pyegui_core::files::Extra {
                title,
                width: 130.0,
                cell: Box::new(move |ui, path| cell(ui, app, &for_cell, path)),
                ty: d.ty.data_type(),
                value: Some(Box::new(move |path| resolved_column_value(&for_value, &app.meta_resolved(&for_value, path)))),
            }
        })
        .collect()
}

/// The cell of a row: the own value as it is, an inherited one or a summary in weak italics
fn cell(ui: &mut egui::Ui, app: &FinderApp, def: &MetaDef, path: &std::path::Path) {
    let resolved = app.meta_resolved(def, path);
    let text = resolved_text(def, &resolved, &date_text);
    if text.is_empty() {
        return;
    }
    let (rich, hint) = match &*resolved {
        Resolved::Own(_) => (RichText::new(text), None),
        Resolved::Inherited { from, .. } => (RichText::new(text).italics().weak(), Some(format!("Hérité de {from}"))),
        Resolved::Summary(s) => (RichText::new(format!("≈ {text}")).italics().weak(), Some(format!("Résumé de {} élément(s) dans ce dossier", s.items))),
        Resolved::None => return,
    };
    let label = ui.add(egui::Label::new(rich).truncate().selectable(false));
    if let Some(hint) = hint {
        label.on_hover_text(hint);
    }
}

/// The seconds since 1970 of a day (midnight UTC: a date has no time zone)
fn day_seconds(day: NaiveDate) -> f64 {
    day.and_time(NaiveTime::MIN).and_utc().timestamp() as f64
}

fn seconds_day(seconds: f64) -> Option<NaiveDate> {
    DateTime::from_timestamp(seconds as i64, 0).map(|t| t.date_naive())
}

/// The text a value is edited as
fn editable(def: &MetaDef, stored: &str) -> String {
    match def.ty {
        MetaType::Date => stored.trim().parse().ok().and_then(seconds_day).map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default(),
        _ => stored.to_owned(),
    }
}

/// The stored text of what was typed, None when it is empty or not a value of the type
enum Typed {
    Value(String),
    Empty,
    Invalid,
}

fn parse_typed(def: &MetaDef, text: &str) -> Typed {
    let t = text.trim();
    if t.is_empty() {
        return Typed::Empty;
    }
    match def.ty {
        MetaType::Number => match t.replace(',', ".").parse::<f64>() {
            Ok(n) if n.is_finite() => Typed::Value(n.to_string()),
            _ => Typed::Invalid,
        },
        MetaType::Date => match parse_date(t, chrono::Local::now().date_naive()) {
            Some(day) => Typed::Value(day_seconds(day).to_string()),
            None => Typed::Invalid,
        },
        _ => Typed::Value(text.to_owned()),
    }
}

enum Act {
    Set(i64, Option<String>),
    /// a new option of an open list, which is then put on the item
    AddAndPick(i64, String),
}

/// The item the pane is about: the one selected, or the folder shown
fn target(app: &FinderApp) -> (PathBuf, bool) {
    match app.selected_paths().as_slice() {
        [one] => (one.clone(), true),
        _ => (app.viewer_path().map(std::path::Path::to_path_buf).unwrap_or_else(|| app.folder().to_path_buf()), false),
    }
}

/// The pane of the properties: for the selected item (or the folder shown), one editor per property
pub fn pane(app: &mut FinderApp, ui: &mut egui::Ui) {
    if app.tags.is_none() {
        ui.weak("Base indisponible");
        return;
    }
    let many = app.selected_paths().len() > 1;
    let (path, selected) = target(app);
    let defs = app.tags.as_ref().map(|s| s.meta_defs()).unwrap_or_default();
    ui.horizontal(|ui| {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string_lossy().into_owned());
        ui.label(RichText::new(name).strong());
        if !selected {
            ui.weak("(dossier affiché)");
        }
    });
    if ui.button("Gérer les propriétés…").clicked() {
        app.meta.manager = Some(Manager::default());
    }
    ui.separator();
    if many {
        ui.weak("Plusieurs éléments sélectionnés : choisis-en un seul pour éditer ses propriétés.");
        return;
    }
    if defs.is_empty() {
        ui.weak("Aucune propriété : crée-en une avec « Gérer les propriétés… ».");
        return;
    }
    if app.meta.path.as_ref() != Some(&path) {
        app.meta.path = Some(path.clone());
        app.meta.buffers.clear();
        app.meta.new_option.clear();
    }
    let values = app.tags.as_ref().map(|s| s.meta_values_of(&path)).unwrap_or_default();
    let resolved: Vec<Rc<Resolved>> = defs.iter().map(|d| if d.ty == MetaType::Formula { Rc::new(Resolved::None) } else { app.meta_resolved(d, &path) }).collect();
    egui::CollapsingHeader::new("Colonnes des vues").id_salt("meta-columns").show(ui, |ui| {
        for def in &defs {
            let mut on = app.meta_columns.contains(&def.id);
            if ui.checkbox(&mut on, &def.name).changed() {
                app.meta_columns.retain(|id| *id != def.id);
                if on {
                    app.meta_columns.push(def.id);
                }
            }
        }
    });
    let mut acts: Vec<Act> = vec![];
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (def, resolved) in defs.iter().zip(&resolved) {
            let stored = values.get(&def.id).cloned();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let label = ui.label(RichText::new(&def.name).strong());
                if !def.description.is_empty() {
                    label.on_hover_text(&def.description);
                }
                ui.weak(def.ty.label());
                if stored.is_some() && glyphs::button(ui, glyphs::CLOSE, 12.0, false, "Effacer la valeur").clicked() {
                    acts.push(Act::Set(def.id, None));
                    app.meta.buffers.remove(&def.id);
                }
            });
            // what the item has without a value of its own: the one of a folder around it, or what is below it
            if def.ty == MetaType::Formula {
                let text = app.meta_formula(def, &path);
                ui.label(if text.is_empty() { RichText::new("—").weak() } else { RichText::new(&*text) });
                continue;
            }
            match &**resolved {
                Resolved::Inherited { from, .. } => {
                    let name = std::path::Path::new(from).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| from.clone());
                    ui.label(RichText::new(format!("↳ {} · hérité de {name}", resolved_text(def, resolved, &date_text))).italics().weak());
                }
                Resolved::Summary(s) => {
                    ui.label(RichText::new(format!("≈ {} · résumé de {} élément(s)", resolved_text(def, resolved, &date_text), s.items)).italics().weak());
                }
                _ => {}
            }
            match def.ty {
                MetaType::Bool => {
                    let mut on = stored.as_deref() == Some("1");
                    let words = if on { "Oui" } else { "Non" };
                    if ui.checkbox(&mut on, words).changed() {
                        acts.push(Act::Set(def.id, Some(if on { "1" } else { "0" }.into())));
                    }
                }
                MetaType::Choice => {
                    let ids = stored.as_deref().map(decode_ids).unwrap_or_default();
                    let shown = if ids.is_empty() { "—".to_owned() } else { display(def, stored.as_deref().unwrap_or("")) };
                    let new_option = app.meta.new_option.entry(def.id).or_default();
                    egui::ComboBox::from_id_salt(("meta-choice", def.id)).selected_text(shown).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
                        for option in &def.config.options {
                            let swatch = if option.color.is_empty() { None } else { Some(parse_color(&option.color)) };
                            ui.horizontal(|ui| {
                                if let Some(color) = swatch {
                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                                    ui.painter().circle_filled(rect.center(), 5.0, color);
                                }
                                if def.config.multiple {
                                    let mut on = ids.contains(&option.id);
                                    if ui.checkbox(&mut on, &option.label).changed() {
                                        let mut next = ids.clone();
                                        if on { next.push(option.id) } else { next.retain(|i| *i != option.id) }
                                        // the order of the list, not the order of the clicks
                                        next.sort_by_key(|id| def.config.options.iter().position(|o| o.id == *id));
                                        let text = encode_ids(&next, true);
                                        acts.push(Act::Set(def.id, (!text.is_empty()).then_some(text)));
                                    }
                                } else if ui.selectable_label(ids == [option.id], &option.label).clicked() {
                                    acts.push(Act::Set(def.id, Some(option.id.to_string())));
                                    ui.close();
                                }
                            });
                        }
                        if def.config.options.is_empty() {
                            ui.weak("Aucune option");
                        }
                        if def.config.mode == ListMode::Open {
                            ui.separator();
                            let edit = ui.add(egui::TextEdit::singleline(new_option).hint_text("Ajouter une option").desired_width(160.0));
                            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !new_option.trim().is_empty() {
                                acts.push(Act::AddAndPick(def.id, std::mem::take(new_option)));
                                edit.request_focus();
                            }
                        }
                    });
                }
                _ => {
                    let current = stored.as_deref().map(|v| editable(def, v)).unwrap_or_default();
                    let buffer = app.meta.buffers.entry(def.id).or_insert_with(|| current.clone());
                    let edit = if def.ty == MetaType::LongText {
                        ui.add(egui::TextEdit::multiline(buffer).desired_rows(3).desired_width(f32::INFINITY))
                    } else {
                        let hint = match def.ty {
                            MetaType::Date => "jj/mm/aaaa, demain, 12 mars…",
                            MetaType::Number => "nombre",
                            _ => "",
                        };
                        ui.add(egui::TextEdit::singleline(buffer).hint_text(hint).desired_width(f32::INFINITY))
                    };
                    if edit.lost_focus() && *buffer != current {
                        match parse_typed(def, buffer) {
                            Typed::Value(v) => acts.push(Act::Set(def.id, Some(v))),
                            Typed::Empty => acts.push(Act::Set(def.id, None)),
                            Typed::Invalid => *buffer = current.clone(),
                        }
                    } else if !edit.has_focus() && *buffer != current {
                        // changed elsewhere (a cleared value, another window of the app)
                        *buffer = current.clone();
                    }
                    if def.ty == MetaType::Date {
                        if let Some(day) = stored.as_deref().and_then(|v| v.trim().parse().ok()).and_then(seconds_day) {
                            ui.weak(human_full_date(day));
                        }
                    }
                    if def.ty == MetaType::Number && !def.config.unit.is_empty() && stored.is_some() {
                        ui.weak(display(def, stored.as_deref().unwrap_or("")));
                    }
                }
            }
        }
    });
    if acts.is_empty() {
        return;
    }
    let Some(store) = app.tags.as_mut() else { return };
    for act in acts {
        match act {
            Act::Set(def, value) => store.set_meta_value(&path, def, value.as_deref()),
            Act::AddAndPick(def, label) => {
                let Some(option) = store.add_choice_option(def, &label) else { continue };
                let multiple = store.meta_def(def).is_some_and(|d| d.config.multiple);
                let mut ids = if multiple { store.meta_value(&path, def).map(|v| decode_ids(&v)).unwrap_or_default() } else { vec![] };
                if !ids.contains(&option) {
                    ids.push(option);
                }
                store.set_meta_value(&path, def, Some(&encode_ids(&ids, multiple)));
            }
        }
    }
    app.meta.buffers.clear();
}

/// The window that manages the definitions
pub fn modal(app: &mut FinderApp, ctx: &egui::Context) {
    let Some(mut manager) = app.meta.manager.take() else { return };
    let Some(store) = app.tags.as_mut() else { return };
    let defs = store.meta_defs();
    // the draft follows the property chosen; a deleted one is forgotten
    if manager.selected.is_some_and(|id| !defs.iter().any(|d| d.id == id)) {
        manager.selected = None;
    }
    if manager.selected.is_none() {
        manager.selected = defs.first().map(|d| d.id);
    }
    if manager.draft.as_ref().map(|d| d.id) != manager.selected {
        manager.draft = manager.selected.and_then(|id| defs.iter().find(|d| d.id == id).cloned());
        manager.confirm_option = None;
        manager.confirm_property = false;
    }
    let mut close = false;
    let mut save = false;
    egui::Modal::new(egui::Id::unique("meta-manager")).show(ctx, |ui| {
        // a fixed size: nothing inside may ask for more, or the window grows at every frame
        ui.set_width(660.0);
        ui.set_max_width(660.0);
        ui.set_max_height(560.0);
        ui.heading("Propriétés");
        ui.weak("Un nom, un type et une description. Les valeurs se mettent sur les fichiers et dossiers depuis le panneau Propriétés.");
        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(190.0);
                egui::ScrollArea::vertical().id_salt("meta-list").max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                    for def in &defs {
                        if ui.selectable_label(manager.selected == Some(def.id), format!("{}  ·  {}", def.name, def.ty.label())).clicked() {
                            manager.selected = Some(def.id);
                        }
                    }
                    if defs.is_empty() {
                        ui.weak("Aucune propriété");
                    }
                });
                if let Some(id) = manager.selected {
                    ui.horizontal(|ui| {
                        if ui.small_button("▲").clicked() {
                            store.move_meta(id, -1);
                        }
                        if ui.small_button("▼").clicked() {
                            store.move_meta(id, 1);
                        }
                    });
                }
                ui.separator();
                ui.add(egui::TextEdit::singleline(&mut manager.new_name).hint_text("Nouvelle propriété").desired_width(180.0));
                egui::ComboBox::from_id_salt("meta-new-type").selected_text(MetaType::ALL[manager.new_type].label()).width(180.0).show_ui(ui, |ui| {
                    for (i, ty) in MetaType::ALL.iter().enumerate() {
                        ui.selectable_value(&mut manager.new_type, i, ty.label());
                    }
                });
                if ui.add_enabled(!manager.new_name.trim().is_empty(), egui::Button::new("Créer")).clicked() {
                    if let Some(id) = store.create_meta(&manager.new_name, MetaType::ALL[manager.new_type]) {
                        manager.selected = Some(id);
                        manager.new_name.clear();
                    }
                }
            });
            // not `ui.separator()`: in a row it takes all the height it is given, and the window grows at every frame
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.set_width(430.0);
                let Some(draft) = manager.draft.as_mut() else {
                    ui.weak("Choisis une propriété ou crées-en une.");
                    return;
                };
                let usage = store.meta_usage(draft.id);
                egui::Grid::new("meta-fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("Nom");
                    let name = ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(280.0));
                    if name.lost_focus() {
                        save = true;
                    }
                    ui.end_row();
                    ui.label("Type");
                    ui.label(draft.ty.label());
                    ui.end_row();
                    ui.label("Description");
                    let description = ui.add(egui::TextEdit::multiline(&mut draft.description).desired_rows(2).desired_width(280.0));
                    if description.lost_focus() {
                        save = true;
                    }
                    ui.end_row();
                    if draft.ty == MetaType::Number {
                        ui.label("Unité");
                        if ui.add(egui::TextEdit::singleline(&mut draft.config.unit).desired_width(80.0)).lost_focus() {
                            save = true;
                        }
                        ui.end_row();
                    }
                    if draft.ty == MetaType::Formula {
                        ui.label("Formule");
                        ui.vertical(|ui| {
                            let edit = ui.add(egui::TextEdit::multiline(&mut draft.config.formula).desired_rows(2).desired_width(280.0).hint_text("{filename} {Statut}"));
                            if edit.lost_focus() {
                                save = true;
                            }
                            ui.weak("{nom} : filename, stem, ext, parent, path, size, modified, ou une autre propriété. {nom:upper}, :lower, :title, :.2f, :03d.");
                        });
                        ui.end_row();
                    }
                    if draft.ty == MetaType::Choice {
                        ui.label("Liste");
                        ui.vertical(|ui| {
                            if ui.radio_value(&mut draft.config.mode, ListMode::Closed, "Fermée : seules les options ci-dessous").changed() {
                                save = true;
                            }
                            if ui.radio_value(&mut draft.config.mode, ListMode::Open, "Ouverte : une nouvelle valeur s'ajoute à la liste").changed() {
                                save = true;
                            }
                            if ui.checkbox(&mut draft.config.multiple, "Plusieurs valeurs à la fois").changed() {
                                save = true;
                            }
                        });
                        ui.end_row();
                    }
                });
                if draft.ty == MetaType::Choice {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Options").strong());
                    let ids: Vec<i64> = draft.config.options.iter().map(|o| o.id).collect();
                    let mut remove: Option<i64> = None;
                    let mut merge: Option<(i64, i64)> = None;
                    egui::ScrollArea::vertical().id_salt("meta-options").max_height(170.0).auto_shrink([false, true]).show(ui, |ui| {
                        let labels: Vec<(i64, String)> = draft.config.options.iter().map(|o| (o.id, o.label.clone())).collect();
                        for id in &ids {
                            let used = store.choice_usage(draft.id, *id);
                            let Some(at) = draft.config.options.iter().position(|o| o.id == *id) else { continue };
                            ui.horizontal(|ui| {
                                let option = &mut draft.config.options[at];
                                let mut color = if option.color.is_empty() { Color32::from_gray(150) } else { parse_color(&option.color) };
                                if ui.color_edit_button_srgba(&mut color).changed() {
                                    option.color = to_hex(color);
                                    save = true;
                                }
                                let mut label = option.label.clone();
                                let edit = ui.add(egui::TextEdit::singleline(&mut label).desired_width(160.0));
                                if edit.changed() {
                                    // refused when empty or taken: the text comes back
                                    if draft.config.rename_option(*id, &label) {
                                        save = true;
                                    }
                                }
                                ui.weak(format!("{used}"));
                                if ui.small_button("▲").clicked() {
                                    draft.config.move_option(*id, -1);
                                    save = true;
                                }
                                if ui.small_button("▼").clicked() {
                                    draft.config.move_option(*id, 1);
                                    save = true;
                                }
                                if manager.confirm_option == Some(*id) {
                                    ui.label(if used > 0 { format!("Retirer de {used} élément(s) ?") } else { "Supprimer ?".to_owned() });
                                    if ui.button("Oui").clicked() {
                                        remove = Some(*id);
                                    }
                                    if ui.button("Non").clicked() {
                                        manager.confirm_option = None;
                                    }
                                } else {
                                    if glyphs::button(ui, glyphs::CLOSE, 12.0, false, "Supprimer l'option").clicked() {
                                        if used == 0 { remove = Some(*id) } else { manager.confirm_option = Some(*id) }
                                    }
                                    if labels.len() > 1 {
                                        ui.menu_button("Fusionner dans…", |ui| {
                                            for (other, name) in &labels {
                                                if other != id && ui.button(name).clicked() {
                                                    merge = Some((*id, *other));
                                                    ui.close();
                                                }
                                            }
                                        });
                                    }
                                }
                            });
                        }
                        if ids.is_empty() {
                            ui.weak("Aucune option");
                        }
                    });
                    ui.horizontal(|ui| {
                        let edit = ui.add(egui::TextEdit::singleline(&mut manager.new_option).hint_text("Ajouter une option").desired_width(200.0));
                        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (ui.button("Ajouter").clicked() || enter) && draft.config.add_option(&manager.new_option).is_some() {
                            manager.new_option.clear();
                            save = true;
                            edit.request_focus();
                        }
                    });
                    // what changes the values goes to the base at once, then the draft is read again
                    if let Some(id) = remove {
                        store.update_meta(draft);
                        store.remove_choice_option(draft.id, id);
                        manager.confirm_option = None;
                        manager.draft = None;
                    } else if let Some((from, into)) = merge {
                        store.update_meta(draft);
                        store.merge_choice_option(draft.id, from, into);
                        manager.draft = None;
                    }
                }
                let Some((draft_id, draft_name)) = manager.draft.as_ref().map(|d| (d.id, d.name.clone())) else { return };
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if manager.confirm_property {
                        ui.label(format!("Supprimer « {draft_name} » et ses {usage} valeur(s) ?"));
                        if ui.button("Supprimer").clicked() {
                            store.delete_meta(draft_id);
                            manager.confirm_property = false;
                            manager.selected = None;
                            manager.draft = None;
                        }
                        if ui.button("Annuler").clicked() {
                            manager.confirm_property = false;
                        }
                    } else if ui.button("Supprimer la propriété").clicked() {
                        manager.confirm_property = true;
                    }
                });
            });
        });
        ui.add_space(8.0);
        if ui.button("Fermer").clicked() {
            close = true;
        }
    });
    if save {
        if let Some(draft) = manager.draft.as_ref() {
            if !store.update_meta(draft) {
                // the name was refused (empty, or another property has it): the stored one comes back
                manager.draft = store.meta_def(draft.id).map(|stored| MetaDef { config: draft.config.clone(), description: draft.description.clone(), ..stored });
                if let Some(d) = manager.draft.as_ref() {
                    store.update_meta(d);
                }
            }
        }
    }
    if !close && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.meta.manager = Some(manager);
    }
}
