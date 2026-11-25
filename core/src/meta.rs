//! Meta properties: the definition of a property (a name, a type, a description, a configuration) and the
//! way a value of each type is stored and read. No database here: the stores (`finder-rs`, `finder.py`) keep
//! the values as text in `meta_values.value`, and this module says what that text means.
//!
//! Stored forms (the same in both Finders):
//!
//! | type | text |
//! |---|---|
//! | `text`, `longtext` | as typed |
//! | `number` | a decimal number (`12`, `3.5`) |
//! | `bool` | `1` or `0` |
//! | `date` | seconds since 1970 (a decimal number) |
//! | `choice` | the id of an option (`3`), or with `multiple` a JSON list of ids (`[3,5]`) |
//!
//! A choice stores the **id** of an option, never its label, so renaming or merging options does not touch
//! the values.

use serde::{Deserialize, Serialize};

use crate::columns::{DataType, Value};

/// What a property holds
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetaType {
    Text,
    LongText,
    Number,
    Bool,
    Date,
    Choice,
}

impl MetaType {
    pub const ALL: [MetaType; 6] = [MetaType::Text, MetaType::LongText, MetaType::Number, MetaType::Bool, MetaType::Date, MetaType::Choice];

    /// The name kept in the base (`meta_defs.type`)
    pub fn name(self) -> &'static str {
        match self {
            MetaType::Text => "text",
            MetaType::LongText => "longtext",
            MetaType::Number => "number",
            MetaType::Bool => "bool",
            MetaType::Date => "date",
            MetaType::Choice => "choice",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    /// The name shown to the person
    pub fn label(self) -> &'static str {
        match self {
            MetaType::Text => "Texte",
            MetaType::LongText => "Texte long",
            MetaType::Number => "Nombre",
            MetaType::Bool => "Oui / non",
            MetaType::Date => "Date",
            MetaType::Choice => "Liste de choix",
        }
    }

    /// How a column of this type sorts and filters
    pub fn data_type(self) -> DataType {
        match self {
            MetaType::Text | MetaType::LongText => DataType::Text,
            MetaType::Number => DataType::Number,
            MetaType::Date => DataType::Date,
            MetaType::Bool | MetaType::Choice => DataType::Choice,
        }
    }
}

/// A closed list only offers its options; an open one also takes a new value, which joins the list
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListMode {
    #[default]
    Closed,
    Open,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub id: i64,
    pub label: String,
    /// `#rrggbb`, or empty for none
    #[serde(default)]
    pub color: String,
}

/// The configuration of a property (`meta_defs.config`, JSON). Only what a type needs is read.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // choice
    pub mode: ListMode,
    pub multiple: bool,
    pub options: Vec<ChoiceOption>,
    /// the id the next option gets (ids are never reused)
    pub next_id: i64,
    // number
    pub unit: String,
    pub decimals: Option<u8>,
}

impl Config {
    pub fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn option(&self, id: i64) -> Option<&ChoiceOption> {
        self.options.iter().find(|o| o.id == id)
    }

    pub fn label_of(&self, id: i64) -> Option<&str> {
        self.option(id).map(|o| o.label.as_str())
    }

    pub fn find(&self, label: &str) -> Option<i64> {
        let wanted = label.trim().to_lowercase();
        self.options.iter().find(|o| o.label.to_lowercase() == wanted).map(|o| o.id)
    }

    /// Adds an option (the id of the one already there if its label exists, case ignored). None for an empty label.
    pub fn add_option(&mut self, label: &str) -> Option<i64> {
        let label = label.trim();
        if label.is_empty() {
            return None;
        }
        if let Some(id) = self.find(label) {
            return Some(id);
        }
        let floor = self.options.iter().map(|o| o.id).max().unwrap_or(0);
        let id = self.next_id.max(floor + 1);
        self.next_id = id + 1;
        self.options.push(ChoiceOption { id, label: label.to_owned(), color: String::new() });
        Some(id)
    }

    /// Renames an option; false if the label is empty or another option has it
    pub fn rename_option(&mut self, id: i64, label: &str) -> bool {
        let label = label.trim();
        if label.is_empty() || self.find(label).is_some_and(|other| other != id) {
            return false;
        }
        match self.options.iter_mut().find(|o| o.id == id) {
            Some(o) => {
                o.label = label.to_owned();
                true
            }
            None => false,
        }
    }

    pub fn remove_option(&mut self, id: i64) {
        self.options.retain(|o| o.id != id);
    }

    /// Moves an option by `delta` places in the list (the order of the list is the order of the sort)
    pub fn move_option(&mut self, id: i64, delta: isize) {
        let Some(at) = self.options.iter().position(|o| o.id == id) else { return };
        let to = (at as isize + delta).clamp(0, self.options.len() as isize - 1) as usize;
        let option = self.options.remove(at);
        self.options.insert(to, option);
    }
}

/// One property of the base
#[derive(Clone, PartialEq, Debug)]
pub struct MetaDef {
    pub id: i64,
    pub name: String,
    pub ty: MetaType,
    pub description: String,
    pub config: Config,
}

impl MetaDef {
    pub fn is_multiple(&self) -> bool {
        self.ty == MetaType::Choice && self.config.multiple
    }
}

// the values

/// The ids a stored choice holds: one id (`3`) or a list (`[3,5]`)
pub fn decode_ids(text: &str) -> Vec<i64> {
    let t = text.trim();
    if t.starts_with('[') {
        serde_json::from_str(t).unwrap_or_default()
    } else {
        t.parse().map(|id| vec![id]).unwrap_or_default()
    }
}

/// The stored form of the ids of a choice: none gives an empty text (no value)
pub fn encode_ids(ids: &[i64], multiple: bool) -> String {
    match (ids, multiple) {
        ([], _) => String::new(),
        (_, true) => serde_json::to_string(ids).unwrap_or_default(),
        (ids, false) => ids[0].to_string(),
    }
}

/// Whether a text is a value the type accepts
pub fn is_valid(def: &MetaDef, text: &str) -> bool {
    let t = text.trim();
    match def.ty {
        MetaType::Text | MetaType::LongText => true,
        MetaType::Number | MetaType::Date => t.parse::<f64>().is_ok_and(f64::is_finite),
        MetaType::Bool => matches!(t, "0" | "1"),
        MetaType::Choice => {
            let ids = decode_ids(t);
            !ids.is_empty() && ids.iter().all(|id| def.config.option(*id).is_some())
        }
    }
}

/// The value as the person reads it. A date needs the day it is said, so it is formatted by the caller
/// (`date_text`); the others are here.
pub fn display(def: &MetaDef, text: &str) -> String {
    match def.ty {
        MetaType::Text | MetaType::LongText => text.to_owned(),
        MetaType::Number => match text.trim().parse::<f64>() {
            Ok(n) => {
                let digits = def.config.decimals.map(usize::from).unwrap_or(if n.fract() == 0.0 { 0 } else { 2 });
                let body = format!("{n:.digits$}");
                if def.config.unit.is_empty() { body } else { format!("{body} {}", def.config.unit) }
            }
            Err(_) => text.to_owned(),
        },
        MetaType::Bool => if text.trim() == "1" { "Oui" } else { "Non" }.to_owned(),
        MetaType::Date => text.to_owned(),
        MetaType::Choice => decode_ids(text).iter().map(|id| def.config.label_of(*id).unwrap_or("?")).collect::<Vec<_>>().join(", "),
    }
}

/// The seconds since 1970 of a stored date
pub fn date_seconds(text: &str) -> Option<f64> {
    text.trim().parse().ok()
}

/// The value of a row in a column, to sort and to filter. A choice sorts by the place of its option in the list
/// (the first of them for a multiple one) and filters on its labels.
pub fn column_value(def: &MetaDef, text: Option<&str>) -> Value {
    let Some(text) = text else { return Value::None };
    match def.ty {
        MetaType::Text | MetaType::LongText => Value::Text(text.to_owned()),
        MetaType::Number | MetaType::Date => text.trim().parse().map(Value::Num).unwrap_or(Value::None),
        MetaType::Bool => Value::Text(display(def, text)),
        MetaType::Choice => Value::Text(display(def, text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(multiple: bool) -> MetaDef {
        let mut config = Config { multiple, ..Config::default() };
        config.add_option("À faire");
        config.add_option("En cours");
        config.add_option("Fini");
        MetaDef { id: 1, name: "Statut".into(), ty: MetaType::Choice, description: String::new(), config }
    }

    #[test]
    fn types_round_trip_by_name() {
        for t in MetaType::ALL {
            assert_eq!(MetaType::parse(t.name()), Some(t));
        }
        assert_eq!(MetaType::parse("daterange"), None);
    }

    #[test]
    fn options_get_ids_that_are_never_reused() {
        let mut def = choice(false);
        let ids: Vec<i64> = def.config.options.iter().map(|o| o.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
        assert_eq!(def.config.add_option("  en COURS "), Some(2), "same label, case ignored: the same option");
        def.config.remove_option(3);
        assert_eq!(def.config.add_option("Archivé"), Some(4), "the id of a removed option is not given again");
        assert_eq!(def.config.add_option("  "), None);
    }

    #[test]
    fn renaming_keeps_the_id_and_refuses_a_duplicate() {
        let mut def = choice(false);
        assert!(def.config.rename_option(2, "Démarré"));
        assert_eq!(def.config.label_of(2), Some("Démarré"));
        assert!(!def.config.rename_option(2, "fini"), "another option has it");
        assert!(!def.config.rename_option(2, " "));
        assert!(def.config.rename_option(2, "démarré"), "its own label, other case");
    }

    #[test]
    fn the_order_of_the_list_can_change() {
        let mut def = choice(false);
        def.config.move_option(3, -2);
        let labels: Vec<&str> = def.config.options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(labels, vec!["Fini", "À faire", "En cours"]);
        def.config.move_option(3, -5);
        assert_eq!(def.config.options[0].id, 3, "clamped at the start");
    }

    #[test]
    fn config_is_kept_as_json_and_a_bad_one_gives_the_default() {
        let mut def = choice(true);
        def.config.mode = ListMode::Open;
        def.config.options[0].color = "#ff0000".into();
        let back = Config::parse(&def.config.to_json());
        assert_eq!(back, def.config);
        assert_eq!(Config::parse("not json"), Config::default());
        assert_eq!(Config::parse("{}"), Config::default());
    }

    #[test]
    fn choice_values_hold_ids_one_or_many() {
        assert_eq!(decode_ids("3"), vec![3]);
        assert_eq!(decode_ids("[3,5]"), vec![3, 5]);
        assert_eq!(decode_ids("x"), Vec::<i64>::new());
        assert_eq!(encode_ids(&[3], false), "3");
        assert_eq!(encode_ids(&[3, 5], true), "[3,5]");
        assert_eq!(encode_ids(&[3], true), "[3]");
        assert_eq!(encode_ids(&[], true), "");
    }

    #[test]
    fn a_choice_shows_labels_and_follows_a_rename() {
        let mut def = choice(true);
        assert_eq!(display(&def, "[1,3]"), "À faire, Fini");
        def.config.rename_option(3, "Terminé");
        assert_eq!(display(&def, "[1,3]"), "À faire, Terminé");
        def.config.remove_option(1);
        assert_eq!(display(&def, "[1,3]"), "?, Terminé");
        assert!(!is_valid(&def, "[1,3]"));
        assert!(is_valid(&def, "[3]"));
    }

    #[test]
    fn values_are_checked_by_type() {
        let def = |ty| MetaDef { id: 1, name: "x".into(), ty, description: String::new(), config: Config::default() };
        assert!(is_valid(&def(MetaType::Number), "3.5"));
        assert!(!is_valid(&def(MetaType::Number), "abc"));
        assert!(!is_valid(&def(MetaType::Number), "inf"));
        assert!(is_valid(&def(MetaType::Bool), "1"));
        assert!(!is_valid(&def(MetaType::Bool), "oui"));
        assert!(is_valid(&def(MetaType::Date), "1700000000"));
        assert!(is_valid(&def(MetaType::Text), ""));
    }

    #[test]
    fn numbers_show_their_unit_and_decimals() {
        let mut d = MetaDef { id: 1, name: "Prix".into(), ty: MetaType::Number, description: String::new(), config: Config::default() };
        assert_eq!(display(&d, "12"), "12");
        assert_eq!(display(&d, "3.5"), "3.50");
        d.config.unit = "€".into();
        d.config.decimals = Some(1);
        assert_eq!(display(&d, "3.5"), "3.5 €");
    }

    #[test]
    fn column_values_sort_by_kind() {
        let mut d = MetaDef { id: 1, name: "n".into(), ty: MetaType::Number, description: String::new(), config: Config::default() };
        assert_eq!(column_value(&d, Some("2")), Value::Num(2.0));
        assert_eq!(column_value(&d, None), Value::None);
        d.ty = MetaType::Bool;
        assert_eq!(column_value(&d, Some("1")), Value::Text("Oui".into()));
        assert_eq!(d.ty.data_type(), DataType::Choice);
    }
}
