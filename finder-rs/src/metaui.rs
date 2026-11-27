//! The meta properties on screen: the pane "Propriétés" (see and edit the values of the selected item) and
//! the window that manages the definitions (name, type, description, and the options of a list).

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, NaiveTime};
use eframe::egui::{self, Color32, RichText};
use pyegui_core::columns::Value;
use pyegui_core::date_input::parse_date;
use pyegui_core::meta::{resolve_with, rule_value, RuleSet, eval_formula, file_var, decode_ids, encode_ids, resolved_column_value, resolved_text, ListMode, MetaDef, MetaType, Resolved};
use std::collections::BTreeMap;
use std::rc::Rc;

use pyegui_core::settings::{settings_window, SettingsEvent, SettingsItem, SettingsOptions, SettingsState};
use pyegui_core::property_set::{Change, Kind, PropOption, Property, PropertySetOptions};

use crate::app::FinderApp;
use crate::glyphs;
use crate::tagsui::{parse_color, to_hex};

/// What the pane and the window keep between two frames
#[derive(Default)]
pub struct MetaUi {
    pub manager: Option<Manager>,
    /// the window of the rules of the indexation, with the text of the rule being added
    pub rules: Option<RulesUi>,
    /// the item whose values the pane shows, and the text being typed for each property
    path: Option<PathBuf>,
    /// the look of the property set: by group or by name, the groups folded, the filter
    set: pyegui_core::property_set::PropertySetState,
}

#[derive(Default)]
pub struct Manager {
    selected: Option<i64>,
    /// the property being edited, as read when it was chosen
    draft: Option<MetaDef>,
    win: SettingsState,
    /// the panel shows a new property to create
    adding: bool,
    new_fresh: bool,
    new_group: String,
    new_description: String,
    new_name: String,
    new_type: usize,
    new_option: String,
    /// an option waiting for the answer to "delete it?" (with the number of items that hold it)
    confirm_option: Option<i64>,
    /// the trash was clicked this frame: the same click does not cancel
    confirm_fresh: bool,
    /// the option being moved by its grip
    drag_option: Option<i64>,
    confirm_property: bool,
}

/// The properties of the store, the own values of those that are shown, and what each row resolves to
/// (own, inherited or summary): read once per version of the store, then looked up while the rows are drawn
#[derive(Default)]
pub struct MetaCache {
    version: u64,
    defs: Rc<Vec<MetaDef>>,
    /// the enabled rules, the most precise first
    rules: Rc<RuleSet>,
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
            *cache = MetaCache {
                version,
                defs: Rc::new(self.tags.as_ref().map(|s| s.meta_defs()).unwrap_or_default()),
                rules: Rc::new(self.tags.as_ref().map(|s| s.ordered_rules()).unwrap_or_default()),
                ..Default::default()
            };
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
        let (rules, today) = (cache.rules.clone(), chrono::Local::now().date_naive());
        let virt = |folder: &str, is_self: bool| if is_self && !path.is_dir() { None } else { rule_value(&rules, def, folder, std::path::MAIN_SEPARATOR, today) };
        let resolved = Rc::new(resolve_with(&values, &key, std::path::MAIN_SEPARATOR, || path.is_dir(), &virt));
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

/// The names the columns of the views already have
pub const RESERVED: &[&str] = &["Nom", "Modifié", "Taille", "Type", "Étiquettes", "Dernière ouverture"];

/// The key of the column of a property in the views: its name, told apart when a column has it already
pub fn column_key(def: &MetaDef) -> String {
    if RESERVED.contains(&def.name.as_str()) { format!("{} (propriété)", def.name) } else { def.name.clone() }
}

/// The columns of the properties: a title (the name of the property) and how a row fills it
pub fn extras<'a>(app: &'a FinderApp, reserved: &[&str]) -> Vec<pyegui_core::files::Extra<'a>> {
    let defs = app.meta_defs();
    defs.iter()
        .map(|d| {
            let title = if reserved.contains(&d.name.as_str()) { format!("{} (propriété)", d.name) } else { d.name.clone() };
            let (for_cell, for_value) = (d.clone(), d.clone());
            if d.ty == MetaType::Formula {
                return pyegui_core::files::Extra {
                    title,
                    icon: Some(crate::glyphs::column_icon(crate::glyphs::PROPERTY)),
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
                icon: Some(crate::glyphs::column_icon(crate::glyphs::PROPERTY)),
                width: 130.0,
                cell: Box::new(move |ui, path| edit_cell(ui, app, &for_cell, path)),
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

/// A change made in a cell of a property column, applied once the view is drawn
pub enum MetaEdit {
    Set { path: PathBuf, def: i64, value: Option<String> },
    /// the ids a list property ends with, and the labels to add to an open list first
    Choice { path: PathBuf, def: i64, ids: Vec<i64>, new_labels: Vec<String> },
}

/// Writes what the cells of the property columns changed to the store
pub fn apply_edits(app: &mut FinderApp) {
    let edits = std::mem::take(&mut *app.meta_edits.borrow_mut());
    let Some(store) = app.tags.as_mut() else { return };
    for edit in edits {
        match edit {
            MetaEdit::Set { path, def, value } => store.set_meta_value(&path, def, value.as_deref()),
            MetaEdit::Choice { path, def, mut ids, new_labels } => {
                for label in new_labels {
                    if let Some(id) = store.add_choice_option(def, &label) {
                        ids.push(id);
                    }
                }
                let multiple = store.meta_def(def).is_some_and(|d| d.config.multiple);
                if !multiple {
                    ids.truncate(1);
                }
                let text = encode_ids(&ids, multiple);
                store.set_meta_value(&path, def, (!text.is_empty()).then_some(text).as_deref());
            }
        }
    }
}

/// The cell of a row with the editor of the type: a list as chips in the colors of its options, a checkbox, a field
/// for a text, a number or a date. A formula and a long text are only shown.
fn edit_cell(ui: &mut egui::Ui, app: &FinderApp, def: &MetaDef, path: &std::path::Path) {
    if app.tags.is_none() {
        return cell(ui, app, def, path);
    }
    let resolved = app.meta_resolved(def, path);
    match def.ty {
        MetaType::Choice => choice_cell(ui, app, def, path, &resolved),
        MetaType::Bool => {
            let on = matches!(&*resolved, Resolved::Own(v) | Resolved::Inherited { value: v, .. } if v.trim() == "1");
            let mut now = on;
            let muted = !matches!(&*resolved, Resolved::Own(_));
            let response = ui.add(egui::Checkbox::new(&mut now, ""));
            if muted {
                response.clone().on_hover_text("Valeur héritée ou absente : cliquer en met une sur cet élément");
            }
            if now != on {
                app.meta_edits.borrow_mut().push(MetaEdit::Set { path: path.to_path_buf(), def: def.id, value: Some(if now { "1" } else { "0" }.into()) });
            }
        }
        MetaType::Text | MetaType::Number | MetaType::Date => {
            let stored = match &*resolved {
                Resolved::Own(v) => editable(def, v),
                _ => String::new(),
            };
            let hint = match &*resolved {
                Resolved::Inherited { .. } | Resolved::Summary(_) => {
                    let text = resolved_text(def, &resolved, &date_text);
                    if matches!(&*resolved, Resolved::Summary(_)) { format!("≈ {text}") } else { text }
                }
                _ => String::new(),
            };
            let id = egui::Id::unique(("meta-cell", def.id, path));
            let mut buffer = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| stored.clone());
            let response = ui.add(egui::TextEdit::singleline(&mut buffer).frame(egui::Frame::NONE).hint_text(RichText::new(hint).italics()).desired_width(f32::INFINITY));
            if response.lost_focus() {
                if buffer != stored {
                    let value = match parse_typed(def, &buffer) {
                        Typed::Value(v) => Some(Some(v)),
                        Typed::Empty => Some(None),
                        Typed::Invalid => None,
                    };
                    if let Some(value) = value {
                        app.meta_edits.borrow_mut().push(MetaEdit::Set { path: path.to_path_buf(), def: def.id, value });
                    }
                }
                ui.data_mut(|d| d.remove_temp::<String>(id));
            } else if response.has_focus() {
                ui.data_mut(|d| d.insert_temp(id, buffer));
            } else {
                ui.data_mut(|d| d.remove_temp::<String>(id));
            }
        }
        MetaType::LongText | MetaType::Formula => cell(ui, app, def, path),
    }
}

/// A list property in a cell: the editor of the properties pane, flat (a check box per option, a round for one choice)
fn choice_cell(ui: &mut egui::Ui, app: &FinderApp, def: &MetaDef, path: &std::path::Path, resolved: &Resolved) {
    let order = |id: &i64| def.config.options.iter().position(|o| o.id == *id);
    let shown: Vec<i64> = match resolved {
        Resolved::Own(v) | Resolved::Inherited { value: v, .. } => decode_ids(v),
        Resolved::Summary(s) => {
            let mut all: Vec<i64> = s.values.iter().flat_map(|(v, _)| decode_ids(v)).collect();
            all.sort_by_key(order);
            all.dedup();
            all
        }
        Resolved::None => vec![],
    };
    {
        // the editor of the properties pane: a combo box with a check box (or a round, for one choice) per option
        use pyegui_core::property_set::{choice_combo, ChoiceChange, ChoiceSpec, PropOption};
        let options: Vec<PropOption> = def.config.options.iter().map(|o| PropOption { id: o.id.to_string(), label: o.label.clone(), color: (!o.color.is_empty()).then(|| parse_color(&o.color)) }).collect();
        let selected: Vec<String> = shown.iter().map(|i| i.to_string()).collect();
        let salt = egui::Id::unique(("meta-multi-cell", def.id, path));
        let mut typed: String = ui.data(|d| d.get_temp(salt.with("new"))).unwrap_or_default();
        let spec = ChoiceSpec { salt, options: &options, selected: &selected, multiple: def.config.multiple, open: def.config.mode == ListMode::Open, editable: true, width: ui.available_width(), add_hint: "Nouvelle valeur", note: None, flat: true, muted: !matches!(resolved, Resolved::Own(_)) };
        let change = choice_combo(ui, &spec, &mut typed);
        ui.data_mut(|d| d.insert_temp(salt.with("new"), typed));
        let edit = match change {
            Some(ChoiceChange::Choose(ids)) => Some((ids.iter().filter_map(|i| i.parse().ok()).collect::<Vec<i64>>(), vec![])),
            Some(ChoiceChange::Clear) => Some((vec![], vec![])),
            Some(ChoiceChange::AddOption(label)) => Some((if def.config.multiple { shown.clone() } else { vec![] }, vec![label])),
            None => None,
        };
        if let Some((ids, new_labels)) = edit {
            app.meta_edits.borrow_mut().push(MetaEdit::Choice { path: path.to_path_buf(), def: def.id, ids, new_labels });
        }
        return;
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

/// The row of the property set for a property: its editor, its value, and what the item has without a value of its own
fn property_of(app: &FinderApp, def: &MetaDef, resolved: &Resolved, stored: Option<&str>, path: &std::path::Path) -> Property {
    let kind = match def.ty {
        MetaType::Text => Kind::Text,
        MetaType::LongText => Kind::LongText,
        MetaType::Number => Kind::Number,
        MetaType::Date => Kind::Date,
        MetaType::Bool => Kind::Bool,
        MetaType::Formula => Kind::Readonly,
        MetaType::Choice => Kind::Choice {
            options: def.config.options.iter().map(|o| PropOption { id: o.id.to_string(), label: o.label.clone(), color: (!o.color.is_empty()).then(|| parse_color(&o.color)) }).collect(),
            multiple: def.config.multiple,
            open: def.config.mode == ListMode::Open,
        },
    };
    let mut p = Property::new(&def.id.to_string(), &def.name, kind);
    p.group = def.config.group.clone();
    p.description = def.description.clone();
    p.hint = match def.ty {
        MetaType::Date => "jj/mm/aaaa, demain, 12 mars…".into(),
        MetaType::Number => "nombre".into(),
        _ => String::new(),
    };
    if def.ty == MetaType::Number {
        p.unit = def.config.unit.clone();
    }
    match (def.ty, stored) {
        (MetaType::Formula, _) => p.value = app.meta_formula(def, path).to_string(),
        (MetaType::Choice, Some(v)) => p.selected = decode_ids(v).iter().map(|i| i.to_string()).collect(),
        (_, Some(v)) => p.value = editable(def, v),
        _ => {}
    }
    if stored.is_none() && def.ty != MetaType::Formula {
        p.note = match resolved {
            Resolved::Inherited { from, .. } => {
                let name = std::path::Path::new(from).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| from.clone());
                format!("↳ {} · hérité de {name}", resolved_text(def, resolved, &date_text))
            }
            Resolved::Summary(s) => format!("≈ {} · résumé de {} élément(s)", resolved_text(def, resolved, &date_text), s.items),
            _ => String::new(),
        };
    }
    p
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
    ui.horizontal(|ui| {
        if ui.button("Gérer les propriétés…").clicked() {
            app.meta.manager = Some(Manager::default());
        }
        if ui.button("Règles d'indexation…").clicked() {
            app.meta.rules = Some(RulesUi::new(app.folder().to_string_lossy().into_owned()));
        }
    });
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
    }
    let values = app.tags.as_ref().map(|s| s.meta_values_of(&path)).unwrap_or_default();
    let resolved: Vec<Rc<Resolved>> = defs.iter().map(|d| if d.ty == MetaType::Formula { Rc::new(Resolved::None) } else { app.meta_resolved(d, &path) }).collect();
    let rows: Vec<Property> = defs.iter().zip(&resolved).map(|(def, resolved)| property_of(app, def, resolved, values.get(&def.id).map(String::as_str), &path)).collect();
    let mut options = PropertySetOptions::new(egui::Id::unique("meta-set"));
    options.key = path.to_string_lossy().into_owned();
    options.default_group = "Divers".into();
    options.grouped_label = "Par groupe".into();
    options.alphabetical_label = "A–Z".into();
    options.filter_hint = "Filtrer".into();
    options.clear_tip = "Effacer la valeur".into();
    options.add_hint = "Ajouter une option".into();
    options.empty_text = "Aucune propriété".into();
    let changes = pyegui_core::property_set::property_set(ui, &mut app.meta.set, &rows, &options);
    let mut acts: Vec<Act> = vec![];
    for change in changes {
        let def_of = |id: &str| id.parse::<i64>().ok().and_then(|id| defs.iter().find(|d| d.id == id));
        match change {
            Change::Clear { id } => {
                if let Some(def) = def_of(&id) {
                    acts.push(Act::Set(def.id, None));
                }
            }
            Change::Set { id, value } => {
                let Some(def) = def_of(&id) else { continue };
                if def.ty == MetaType::Bool {
                    acts.push(Act::Set(def.id, Some(value)));
                } else {
                    match parse_typed(def, &value) {
                        Typed::Value(v) => acts.push(Act::Set(def.id, Some(v))),
                        Typed::Empty => acts.push(Act::Set(def.id, None)),
                        Typed::Invalid => {}
                    }
                }
            }
            Change::Choose { id, ids } => {
                let Some(def) = def_of(&id) else { continue };
                let ids: Vec<i64> = ids.iter().filter_map(|i| i.parse().ok()).collect();
                let text = encode_ids(&ids, def.config.multiple);
                acts.push(Act::Set(def.id, (!text.is_empty()).then_some(text)));
            }
            Change::AddOption { id, label } => {
                if let Some(def) = def_of(&id) {
                    acts.push(Act::AddAndPick(def.id, label));
                }
            }
        }
    }
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
    let mut created: Option<i64> = None;
    let items: Vec<SettingsItem> = defs.iter().map(|d| SettingsItem { id: d.id.to_string(), label: d.name.clone(), hint: d.ty.label().to_owned() }).collect();
    let selected = manager.selected.map(|id| id.to_string());
    let adding = manager.adding;
    let mut options = SettingsOptions::new(egui::Id::unique("meta-manager"));
    options.title = "Propriétés".into();
    options.subtitle = "Un nom, un type et une description. Les valeurs se mettent sur les fichiers et dossiers depuis le panneau Propriétés.".into();
    options.list_title = "Propriétés".into();
    options.add_label = "Nouvelle propriété".into();
    options.empty_text = "Aucune propriété".into();
    options.width = 780.0;
    options.height = 430.0;
    options.adding = adding;
    options.frame = Some(crate::glyphs::popup_frame(ctx));
    // the state of the list is taken out while the panel borrows the rest of the manager
    let mut win = std::mem::take(&mut manager.win);
    let events = settings_window(
        ctx,
        &mut win,
        &options,
        &items,
        selected.as_deref(),
        |ui| {
            crate::glyphs::modal_visuals(ui);
            if adding {
                new_property_form(ui, &mut manager, store, &mut created);
                return;
            }
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
            ui.label("Groupe");
            if ui.add(egui::TextEdit::singleline(&mut draft.config.group).hint_text("Divers").desired_width(280.0)).lost_focus() {
                save = true;
            }
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
            let mut rows: Vec<(i64, egui::Rect)> = vec![];
            ui.scope(|ui| {
                let labels: Vec<(i64, String)> = draft.config.options.iter().map(|o| (o.id, o.label.clone())).collect();
                for id in &ids {
                    let used = store.choice_usage(draft.id, *id);
                    let Some(at) = draft.config.options.iter().position(|o| o.id == *id) else { continue };
                    let row = ui.horizontal(|ui| {
                        // the grip: drag the option to another place
                        let (grip, grip_response) = ui.allocate_exact_size(egui::vec2(14.0, 22.0), egui::Sense::drag());
                        if grip_response.drag_started() {
                            manager.drag_option = Some(*id);
                        }
                        if grip_response.hovered() || manager.drag_option == Some(*id) {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                        }
                        let dots = ui.visuals().text_color().gamma_multiply(0.5);
                        for dx in [-2.5f32, 2.5] {
                            for dy in [-4.0f32, 0.0, 4.0] {
                                ui.painter().circle_filled(grip.center() + egui::vec2(dx, dy), 1.2, dots);
                            }
                        }
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
                        // the trash: a first click asks, a second one deletes
                        let armed = manager.confirm_option == Some(*id);
                        // no text: it would move the icon, the tip says what a second click does
                        let tip = if armed { if used > 0 { format!("Cliquer encore pour retirer de {used} élément(s) et supprimer") } else { "Cliquer encore pour supprimer".to_owned() } } else { "Supprimer l'option".to_owned() };
                        let trash = glyphs::button(ui, glyphs::TRASH, 13.0, armed, &tip);
                        if trash.clicked() {
                            if armed { remove = Some(*id) } else { manager.confirm_option = Some(*id); manager.confirm_fresh = true; }
                        }
                        if !armed && labels.len() > 1 {
                            ui.menu_button("Fusionner dans…", |ui| {
                                for (other, name) in &labels {
                                    if other != id && ui.button(name).clicked() {
                                        merge = Some((*id, *other));
                                        ui.close();
                                    }
                                }
                            });
                        }
                        if armed && !manager.confirm_fresh && !trash.hovered() && ui.input(|i| i.pointer.any_pressed() || i.key_pressed(egui::Key::Escape)) {
                            manager.confirm_option = None;
                        }
                    });
                    rows.push((*id, row.response.rect));
                }
                if ids.is_empty() {
                    ui.weak("Aucune option");
                }
            });
            // the drag: the option follows the row under the mouse
            if let Some(moving) = manager.drag_option {
                if ui.input(|i| i.pointer.any_down()) {
                    if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                        let to = if p.y < rows.first().map_or(0.0, |r| r.1.top()) {
                            rows.first().map(|r| r.0)
                        } else if p.y > rows.last().map_or(0.0, |r| r.1.bottom()) {
                            rows.last().map(|r| r.0)
                        } else {
                            rows.iter().find(|r| r.1.y_range().contains(p.y)).map(|r| r.0)
                        };
                        if let (Some(to), Some(from)) = (to.and_then(|t| ids.iter().position(|i| *i == t)), ids.iter().position(|i| *i == moving)) {
                            if to != from {
                                draft.config.move_option(moving, to as isize - from as isize);
                                save = true;
                            }
                        }
                    }
                    ui.ctx().request_repaint();
                } else {
                    manager.drag_option = None;
                }
            }
            manager.confirm_fresh = false;
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
        },
        |ui| {
            crate::glyphs::modal_visuals(ui);
            ui.weak(format!("{} propriété(s)", items.len()));
        },
    );
    manager.win = win;
    for event in events {
        match event {
            SettingsEvent::Select(id) => {
                manager.selected = id.parse().ok();
                manager.adding = false;
            }
            SettingsEvent::Move { from, to } => {
                if let Some(d) = defs.get(from) {
                    store.move_meta(d.id, to as isize - from as isize);
                }
            }
            SettingsEvent::Add => {
                manager.adding = true;
                manager.new_fresh = true;
            }
            SettingsEvent::Close => close = true,
        }
    }
    if let Some(id) = created {
        manager.selected = Some(id);
        manager.adding = false;
        manager.draft = None;
    }
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


/// The panel of a new property: its name, type, group and description, then "Créer"
fn new_property_form(ui: &mut egui::Ui, manager: &mut Manager, store: &mut crate::tagstore::TagStore, created: &mut Option<i64>) {
    ui.label(RichText::new("Nouvelle propriété").strong().size(15.0));
    ui.add_space(6.0);
    egui::Grid::new("meta-new-fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Nom");
        let name = ui.add(egui::TextEdit::singleline(&mut manager.new_name).desired_width(280.0));
        if manager.new_fresh {
            name.request_focus();
            manager.new_fresh = false;
        }
        ui.end_row();
        ui.label("Type");
        egui::ComboBox::from_id_salt("meta-new-type").selected_text(MetaType::ALL[manager.new_type].label()).width(280.0).show_ui(ui, |ui| {
            for (i, ty) in MetaType::ALL.iter().enumerate() {
                ui.selectable_value(&mut manager.new_type, i, ty.label());
            }
        });
        ui.end_row();
        ui.label("Groupe");
        ui.add(egui::TextEdit::singleline(&mut manager.new_group).hint_text("Divers").desired_width(280.0));
        ui.end_row();
        ui.label("Description");
        ui.add(egui::TextEdit::multiline(&mut manager.new_description).desired_rows(2).desired_width(280.0));
        ui.end_row();
    });
    ui.add_space(8.0);
    let taken = store.meta_defs().iter().any(|d| d.name.to_lowercase() == manager.new_name.trim().to_lowercase());
    if taken && !manager.new_name.trim().is_empty() {
        ui.colored_label(Color32::from_rgb(220, 90, 90), "Une propriété porte déjà ce nom");
    }
    ui.horizontal(|ui| {
        let ok = !manager.new_name.trim().is_empty() && !taken;
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter)) && ok;
        if ui.add_enabled(ok, egui::Button::new("Créer").min_size(egui::vec2(88.0, 26.0))).clicked() || enter {
            if let Some(id) = store.create_meta(&manager.new_name, MetaType::ALL[manager.new_type]) {
                if let Some(mut def) = store.meta_def(id) {
                    def.config.group = manager.new_group.trim().to_owned();
                    def.description = manager.new_description.trim().to_owned();
                    store.update_meta(&def);
                }
                manager.new_name.clear();
                manager.new_group.clear();
                manager.new_description.clear();
                *created = Some(id);
            }
        }
        if ui.button("Annuler").clicked() {
            manager.adding = false;
        }
    });
}

/// What the window of the rules keeps between two frames
#[derive(Default)]
pub struct RulesUi {
    /// the folder and the pattern of the rule being added
    root: String,
    pattern: String,
    name: String,
    selected: Option<i64>,
    adding: bool,
    confirm_delete: bool,
    win: SettingsState,
}

impl RulesUi {
    pub fn new(folder: String) -> Self {
        RulesUi { root: folder, ..Default::default() }
    }
}

const RULES_HELP: &str = "Sous un dossier, chaque niveau de sous-dossiers remplit une propriété : {project}/{entreprise}/{chapter}. Les valeurs se mettent sur les dossiers (leur contenu en hérite) à la fin d'une indexation ou avec « Appliquer ». Une valeur tapée à la main n'est jamais remplacée. * ignore un niveau ; un nom de dossier fixe (00 - DOE Livrés) ne s'applique qu'à ce dossier ; {nom!a|b} refuse les dossiers a et b ; [Statut=Terminé] après un niveau donne cette valeur à son dossier.";

fn rule_check(ui: &mut egui::Ui, pattern: &str, names: &[String]) {
    match pyegui_core::meta::Pattern::parse(pattern) {
        None => {
            ui.colored_label(Color32::from_rgb(220, 90, 90), "Aucun {nom} dans le motif");
        }
        Some(p) => {
            let missing: Vec<&str> = p.names().into_iter().filter(|n| !names.contains(&n.to_lowercase())).collect();
            if !missing.is_empty() {
                ui.colored_label(Color32::from_rgb(220, 90, 90), format!("Propriété inconnue : {}", missing.join(", ")));
            }
        }
    }
}

/// The window of the rules of the indexation: a folder, and the pattern its sub-folders are read with
pub fn rules_modal(app: &mut FinderApp, ctx: &egui::Context) {
    let Some(mut ui_state) = app.meta.rules.take() else { return };
    let folder = app.folder().to_string_lossy().into_owned();
    let Some(store) = app.tags.as_mut() else { return };
    let rules = store.meta_rules();
    let names: Vec<String> = store.meta_defs().iter().map(|d| d.name.to_lowercase()).collect();
    if ui_state.selected.is_some_and(|id| !rules.iter().any(|r| r.id == id)) {
        ui_state.selected = None;
    }
    if ui_state.selected.is_none() && !ui_state.adding {
        ui_state.selected = rules.first().map(|r| r.id);
    }
    let (mut close, mut apply) = (false, false);
    let mut change: Option<crate::metastore::Rule> = None;
    let mut delete: Option<i64> = None;
    let mut add = false;
    let items: Vec<SettingsItem> = rules
        .iter()
        .map(|r| SettingsItem {
            id: r.id.to_string(),
            label: r.title().to_owned(),
            hint: if r.enabled { String::new() } else { "désactivée".to_owned() },
        })
        .collect();
    let selected = ui_state.selected.map(|id| id.to_string());
    let adding = ui_state.adding;
    let mut options = SettingsOptions::new(egui::Id::unique("meta-rules"));
    options.title = "Règles d'indexation".into();
    options.subtitle = "Les dossiers d'une arborescence remplissent des propriétés.".into();
    options.list_title = "Règles".into();
    options.add_label = "Nouvelle règle".into();
    options.empty_text = "Aucune règle".into();
    options.width = 780.0;
    options.height = 430.0;
    options.reorderable = false;
    options.adding = adding;
    options.frame = Some(crate::glyphs::popup_frame(ctx));
    let mut win = std::mem::take(&mut ui_state.win);
    let events = settings_window(
        ctx,
        &mut win,
        &options,
        &items,
        selected.as_deref(),
        |ui| {
            crate::glyphs::modal_visuals(ui);
            if adding {
                ui.label(RichText::new("Nouvelle règle").strong().size(15.0));
                ui.add_space(6.0);
                egui::Grid::new("rule-new").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("Nom");
                    ui.add(egui::TextEdit::singleline(&mut ui_state.name).hint_text("Facultatif").desired_width(380.0));
                    ui.end_row();
                    ui.label("Dossier");
                    ui.vertical(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut ui_state.root).desired_width(380.0));
                        if ui.small_button("Dossier affiché").clicked() {
                            ui_state.root = folder.clone();
                        }
                    });
                    ui.end_row();
                    ui.label("Motif");
                    ui.add(egui::TextEdit::singleline(&mut ui_state.pattern).hint_text("{project}/{entreprise}").desired_width(380.0));
                    ui.end_row();
                });
                if !ui_state.pattern.trim().is_empty() {
                    rule_check(ui, &ui_state.pattern, &names);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let ok = !ui_state.root.trim().is_empty() && pyegui_core::meta::Pattern::parse(&ui_state.pattern).is_some();
                    if ui.add_enabled(ok, egui::Button::new("Ajouter").min_size(egui::vec2(88.0, 26.0))).clicked() {
                        add = true;
                    }
                    if ui.button("Annuler").clicked() {
                        ui_state.adding = false;
                    }
                });
            } else if let Some(rule) = rules.iter().find(|r| Some(r.id) == ui_state.selected) {
                let mut r = rule.clone();
                ui.checkbox(&mut r.enabled, "Règle active");
                ui.add_space(4.0);
                egui::Grid::new("rule-edit").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("Nom");
                    ui.add(egui::TextEdit::singleline(&mut r.name).hint_text(r.pattern.as_str()).desired_width(380.0));
                    ui.end_row();
                    ui.label("Dossier");
                    ui.add(egui::TextEdit::singleline(&mut r.root).desired_width(380.0));
                    ui.end_row();
                    ui.label("Motif");
                    ui.add(egui::TextEdit::singleline(&mut r.pattern).desired_width(380.0));
                    ui.end_row();
                });
                rule_check(ui, &r.pattern, &names);
                if r != *rule {
                    change = Some(r);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui_state.confirm_delete {
                        ui.label("Supprimer la règle et les valeurs qu'elle a mises ?");
                        if ui.button("Supprimer").clicked() {
                            delete = Some(rule.id);
                            ui_state.confirm_delete = false;
                        }
                        if ui.button("Annuler").clicked() {
                            ui_state.confirm_delete = false;
                        }
                    } else if ui.button("Supprimer la règle").clicked() {
                        ui_state.confirm_delete = true;
                    }
                });
            } else {
                ui.weak("Choisis une règle ou crées-en une.");
            }
            ui.add_space(10.0);
            egui::CollapsingHeader::new("Syntaxe des motifs").default_open(rules.is_empty()).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(RULES_HELP).weak().small()).wrap());
            });
        },
        |ui| {
            crate::glyphs::modal_visuals(ui);
            if ui.button("Appliquer").on_hover_text("Relit les dossiers déjà indexés").clicked() {
                apply = true;
            }
        },
    );
    ui_state.win = win;
    for event in events {
        match event {
            SettingsEvent::Select(id) => {
                ui_state.selected = id.parse().ok();
                ui_state.adding = false;
                ui_state.confirm_delete = false;
            }
            SettingsEvent::Move { .. } => {}
            SettingsEvent::Add => ui_state.adding = true,
            SettingsEvent::Close => close = true,
        }
    }
    if let Some(r) = change {
        store.update_meta_rule(&r);
    }
    if let Some(id) = delete {
        store.delete_meta_rule(id);
    }
    if add {
        if let Some(id) = store.add_meta_rule(std::path::Path::new(ui_state.root.trim()), &ui_state.pattern) {
            if !ui_state.name.trim().is_empty() {
                if let Some(mut rule) = store.meta_rules().into_iter().find(|r| r.id == id) {
                    rule.name = ui_state.name.trim().to_owned();
                    store.update_meta_rule(&rule);
                }
            }
            ui_state.name.clear();
            ui_state.pattern.clear();
            ui_state.adding = false;
            ui_state.selected = Some(id);
        }
    }
    if apply {
        app.apply_meta_rules_notice();
    }
    if !close {
        app.meta.rules = Some(ui_state);
    }
}
