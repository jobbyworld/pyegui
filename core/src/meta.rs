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
//! | `formula` | nothing: computed from `config.formula` when shown |
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
    /// computed from a template, never stored (see `eval_formula`)
    Formula,
}

impl MetaType {
    pub const ALL: [MetaType; 7] = [MetaType::Text, MetaType::LongText, MetaType::Number, MetaType::Bool, MetaType::Date, MetaType::Choice, MetaType::Formula];

    /// The name kept in the base (`meta_defs.type`)
    pub fn name(self) -> &'static str {
        match self {
            MetaType::Text => "text",
            MetaType::LongText => "longtext",
            MetaType::Number => "number",
            MetaType::Bool => "bool",
            MetaType::Date => "date",
            MetaType::Choice => "choice",
            MetaType::Formula => "formula",
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
            MetaType::Formula => "Formule",
        }
    }

    /// How a column of this type sorts and filters
    pub fn data_type(self) -> DataType {
        match self {
            MetaType::Text | MetaType::LongText | MetaType::Formula => DataType::Text,
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
    // formula
    pub formula: String,
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
        MetaType::Text | MetaType::LongText | MetaType::Formula => true,
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
        MetaType::Text | MetaType::LongText | MetaType::Formula => text.to_owned(),
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
        MetaType::Text | MetaType::LongText | MetaType::Formula => Value::Text(text.to_owned()),
        MetaType::Number | MetaType::Date => text.trim().parse().map(Value::Num).unwrap_or(Value::None),
        MetaType::Bool => Value::Text(display(def, text)),
        MetaType::Choice => Value::Text(display(def, text)),
    }
}

// inheritance and summary

use std::collections::{BTreeMap, BTreeSet};

/// What an item has for a property once the folders around it are taken into account
#[derive(Clone, PartialEq, Debug)]
pub enum Resolved {
    /// a value put on the item itself
    Own(String),
    /// the value of the nearest folder that has one
    Inherited { value: String, from: String },
    /// a folder with no value of its own and none inherited: what the items below it hold
    Summary(Summary),
    None,
}

/// The values found below a folder: each different stored value with how many items hold it
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Summary {
    pub values: Vec<(String, usize)>,
    /// how many items below hold a value
    pub items: usize,
}

/// The folder that holds a path, `None` at the root
fn parent_of(path: &str, sep: char) -> Option<&str> {
    let at = path.trim_end_matches(sep).rfind(sep)?;
    (at > 0).then(|| &path[..at])
}

/// The value of a property for a path. `values` holds the own values of the property by stored path, `sep` is the
/// separator of the paths. The path itself wins, then its nearest folder; a folder that has neither gets the summary
/// of what is below it (`is_dir`).
pub fn resolve(values: &BTreeMap<String, String>, path: &str, sep: char, is_dir: impl FnOnce() -> bool) -> Resolved {
    resolve_with(values, path, sep, is_dir, &|_, _| None)
}

/// `resolve` where a folder that has no stored value may still get one from the rules of the indexation
/// (`rule_value`), so that a folder the index does not know yet is read like the others. `virt(path, is_self)`
/// is asked for the path itself (`is_self`) and for each folder above it, after the stored values of that level.
pub fn resolve_with(values: &BTreeMap<String, String>, path: &str, sep: char, is_dir: impl FnOnce() -> bool, virt: &dyn Fn(&str, bool) -> Option<String>) -> Resolved {
    if let Some(v) = values.get(path).cloned().or_else(|| virt(path, true)) {
        return Resolved::Own(v);
    }
    let mut at = path;
    while let Some(parent) = parent_of(at, sep) {
        if let Some(v) = values.get(parent).cloned().or_else(|| virt(parent, false)) {
            return Resolved::Inherited { value: v, from: parent.to_owned() };
        }
        at = parent;
    }
    if !is_dir() {
        return Resolved::None;
    }
    let from = format!("{}{sep}", path.trim_end_matches(sep));
    // every path that starts with `folder/`: the characters after the separator sort right after it
    let until = format!("{}{}", path.trim_end_matches(sep), char::from_u32(sep as u32 + 1).unwrap_or('\u{10ffff}'));
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut items = 0;
    for (_, v) in values.range(from..until) {
        *counts.entry(v.as_str()).or_default() += 1;
        items += 1;
    }
    if items == 0 {
        return Resolved::None;
    }
    Resolved::Summary(Summary { values: counts.into_iter().map(|(v, n)| (v.to_owned(), n)).collect(), items })
}

/// The text of a summary. `date` writes a day from its seconds.
pub fn summary_text(def: &MetaDef, summary: &Summary, date: &dyn Fn(f64) -> String) -> String {
    let numbers = || -> Vec<f64> { summary.values.iter().filter_map(|(v, _)| v.trim().parse().ok()).collect() };
    match def.ty {
        MetaType::Number => {
            let n = numbers();
            let (lo, hi) = (n.iter().cloned().fold(f64::INFINITY, f64::min), n.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
            let (a, b) = (display(def, &lo.to_string()), display(def, &hi.to_string()));
            if lo == hi { a } else { format!("{a} – {b}") }
        }
        MetaType::Date => {
            let n = numbers();
            let (lo, hi) = (n.iter().cloned().fold(f64::INFINITY, f64::min), n.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
            if lo == hi { date(lo) } else { format!("{} – {}", date(lo), date(hi)) }
        }
        MetaType::Bool => {
            let yes: usize = summary.values.iter().filter(|(v, _)| v.trim() == "1").map(|(_, n)| n).sum();
            format!("{yes}/{} oui", summary.items)
        }
        MetaType::Choice => {
            // the labels of the options below, in the order of the list
            let used: BTreeSet<i64> = summary.values.iter().flat_map(|(v, _)| decode_ids(v)).collect();
            let labels: Vec<&str> = def.config.options.iter().filter(|o| used.contains(&o.id)).map(|o| o.label.as_str()).collect();
            more(&labels)
        }
        MetaType::Text | MetaType::LongText | MetaType::Formula => {
            let texts: Vec<&str> = summary.values.iter().map(|(v, _)| v.lines().next().unwrap_or("")).collect();
            more(&texts)
        }
    }
}

/// "a, b, c (+2)": the first three
fn more(items: &[&str]) -> String {
    let shown = items.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
    if items.len() > 3 { format!("{shown} (+{})", items.len() - 3) } else { shown }
}

/// The text of a resolved value (empty for none); a date is written by `date`
pub fn resolved_text(def: &MetaDef, resolved: &Resolved, date: &dyn Fn(f64) -> String) -> String {
    let one = |v: &str| if def.ty == MetaType::Date { date_seconds(v).map(date).unwrap_or_default() } else { display(def, v) };
    match resolved {
        Resolved::Own(v) | Resolved::Inherited { value: v, .. } => one(v),
        Resolved::Summary(s) => summary_text(def, s, date),
        Resolved::None => String::new(),
    }
}

/// The value of a resolved property in a column: a summary of numbers or dates sorts by its lowest
pub fn resolved_column_value(def: &MetaDef, resolved: &Resolved) -> Value {
    match resolved {
        Resolved::Own(v) | Resolved::Inherited { value: v, .. } => column_value(def, Some(v)),
        Resolved::Summary(s) if matches!(def.ty, MetaType::Number | MetaType::Date) => {
            s.values.iter().filter_map(|(v, _)| v.trim().parse::<f64>().ok()).fold(None, |lo: Option<f64>, n| Some(lo.map_or(n, |l| l.min(n)))).map_or(Value::None, Value::Num)
        }
        Resolved::Summary(s) => Value::Text(summary_text(def, s, &|d| d.to_string())),
        Resolved::None => Value::None,
    }
}

// formulas

/// The text of a template where `{name}` is replaced by the value of `name`, like a Python f-string:
/// `{filename} {Statut}`. `{name:spec}` formats it: `upper`, `lower`, `title`, `trim`, `.2f` (decimals),
/// `03d` (zeros in front). `{{` and `}}` are a brace. A name that has no value gives nothing.
pub fn eval_formula(template: &str, var: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let mut expr = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    expr.push(c);
                }
                if !closed {
                    out.push('{');
                    out.push_str(&expr);
                    break;
                }
                let (name, spec) = expr.split_once(':').unwrap_or((&expr, ""));
                out.push_str(&apply_spec(var(name.trim()).unwrap_or_default(), spec.trim()));
            }
            c => out.push(c),
        }
    }
    out
}

fn apply_spec(value: String, spec: &str) -> String {
    match spec {
        "" => value,
        "upper" => value.to_uppercase(),
        "lower" => value.to_lowercase(),
        "trim" => value.trim().to_owned(),
        "title" => value
            .split(' ')
            .map(|w| {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()).unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => {
            let number = value.trim().replace(',', ".").parse::<f64>().ok();
            if let (Some(n), Some(digits)) = (number, spec.strip_prefix('.').and_then(|d| d.strip_suffix('f')).and_then(|d| d.parse::<usize>().ok())) {
                format!("{n:.digits$}")
            } else if let (Some(n), Some(width)) = (number, spec.strip_suffix('d').and_then(|w| w.parse::<usize>().ok())) {
                if spec.starts_with('0') { format!("{:0width$}", n as i64) } else { format!("{:width$}", n as i64) }
            } else {
                value
            }
        }
    }
}

/// The variables every item has: `filename` (`name`), `stem`, `ext`, `parent` (`folder`: the name of the folder that
/// holds it), `path`, `size` (bytes) and `modified` (`jj/mm/aaaa`). None for another name.
pub fn file_var(path: &std::path::Path, name: &str) -> Option<String> {
    let text = |o: Option<&std::ffi::OsStr>| o.map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Some(match name.to_lowercase().as_str() {
        "filename" | "name" | "file" => text(path.file_name()),
        "stem" => text(path.file_stem()),
        "ext" => text(path.extension()),
        "parent" | "folder" => text(path.parent().and_then(|p| p.file_name())),
        "path" => path.to_string_lossy().into_owned(),
        "size" => std::fs::metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len().to_string()).unwrap_or_default(),
        "modified" => std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
            .map(|t| t.format("%d/%m/%Y").to_string())
            .unwrap_or_default(),
        _ => return None,
    })
}

// rules of the indexation

/// A folder structure that names properties: `{project}/{entreprise}/{chapter}`. The first folder under the root
/// is the project, the one under it the entreprise, and so on. `*` or `_` is a level that gives nothing and
/// accepts any folder; any other text (`00 - DOE Livrés`) is a level that gives nothing and only accepts a
/// folder of that name (case ignored): the pattern does not apply under a folder that has another name.
/// A level may also set values: `[Statut=Terminé]` after it gives that value to the folder of that level
/// (`00 - DOE Livrés [Statut=Terminé]`), several are separated by `;`. `{projet!00 - DOE Livrés|Archives}` is a
/// name that refuses the folders called so (and what is below them).
#[derive(Clone, PartialEq, Debug)]
pub struct Pattern {
    levels: Vec<Level>,
}

#[derive(Clone, PartialEq, Debug)]
struct Level {
    kind: Kind,
    /// (property, text) set on the folder of this level
    sets: Vec<(String, String)>,
    /// folder names (lowercase) that a `{name!...}` level refuses
    except: Vec<String>,
}

#[derive(Clone, PartialEq, Debug)]
enum Kind {
    Any,
    Fixed(String),
    Name(String),
}

impl Pattern {
    /// None when the text names no property at all (no `{name}` and no `[name=value]`)
    pub fn parse(text: &str) -> Option<Self> {
        let levels: Vec<Level> = text
            .split(['/', '\\'])
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| {
                let (head, sets) = match part.strip_suffix(']').and_then(|p| p.rsplit_once('[')) {
                    Some((head, inside)) => {
                        let sets = inside
                            .split(';')
                            .filter_map(|one| one.split_once('='))
                            .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
                            .filter(|(n, v)| !n.is_empty() && !v.is_empty())
                            .collect();
                        (head.trim(), sets)
                    }
                    None => (part, Vec::new()),
                };
                let inside = head.strip_prefix('{').and_then(|p| p.strip_suffix('}')).map(|n| n.trim());
                let (inside, except) = match inside.and_then(|n| n.split_once('!')) {
                    Some((name, list)) => (Some(name.trim()), list.split('|').map(|x| x.trim().to_lowercase()).filter(|x| !x.is_empty()).collect()),
                    None => (inside, Vec::new()),
                };
                let kind = match inside {
                    Some(name) if !name.is_empty() => Kind::Name(name.to_owned()),
                    Some(_) => Kind::Any,
                    None if head.is_empty() || head == "*" || head == "_" => Kind::Any,
                    None => Kind::Fixed(head.to_owned()),
                };
                Level { kind, sets, except }
            })
            .collect();
        levels.iter().any(|l| matches!(l.kind, Kind::Name(_)) || !l.sets.is_empty()).then_some(Pattern { levels })
    }

    /// The names of the properties the pattern fills or sets, in order of level
    pub fn names(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for l in &self.levels {
            if let Kind::Name(n) = &l.kind {
                out.push(n.as_str());
            }
            out.extend(l.sets.iter().map(|(n, _)| n.as_str()));
        }
        out
    }

    /// How many levels under the root the pattern reads
    pub fn depth(&self) -> usize {
        self.levels.len()
    }

    /// How many levels name a folder: of two rules that give the same folder a value, the more precise wins
    pub fn specificity(&self) -> usize {
        self.levels.iter().filter(|l| matches!(l.kind, Kind::Fixed(_))).count()
    }

    /// What a folder gives: the (property, text) pairs, if it is under `root` and the folders above it have the
    /// names the pattern asks: the name of the level, filled with the name of the folder, and the values the
    /// level sets. Empty when the folder gives nothing. `root` and `folder` are stored paths, `sep` is their
    /// separator.
    pub fn assign(&self, root: &str, folder: &str, sep: char) -> Vec<(String, String)> {
        let Some(rest) = folder.strip_prefix(root.trim_end_matches(sep)).and_then(|r| r.strip_prefix(sep)) else { return vec![] };
        let parts: Vec<&str> = rest.split(sep).filter(|p| !p.is_empty()).collect();
        let Some(at) = parts.len().checked_sub(1) else { return vec![] };
        let Some(level) = self.levels.get(at) else { return vec![] };
        for (l, part) in self.levels.iter().zip(&parts) {
            if let Kind::Fixed(text) = &l.kind {
                if text.to_lowercase() != part.to_lowercase() {
                    return vec![];
                }
            }
            if l.except.contains(&part.to_lowercase()) {
                return vec![];
            }
        }
        let mut out = Vec::new();
        if let Kind::Name(name) = &level.kind {
            out.push((name.clone(), parts[at].to_string()));
        }
        out.extend(level.sets.iter().cloned());
        out
    }
}

/// A rule of the indexation: its root (a stored path) and its pattern
pub type RuleSet = Vec<(String, Pattern)>;

/// What the rules give a folder for a property, without the index: the value of the first rule (they come most
/// precise first) whose pattern names the property at the level of the folder. None for a list that does not
/// have the label yet (indexing adds it).
pub fn rule_value(rules: &RuleSet, def: &MetaDef, folder: &str, sep: char, today: chrono::NaiveDate) -> Option<String> {
    for (root, pattern) in rules {
        for (name, text) in pattern.assign(root, folder, sep) {
            if name.to_lowercase() != def.name.to_lowercase() {
                continue;
            }
            let mut copy = def.clone();
            let value = parse_input(&mut copy, &text, today)?;
            return (copy.config == def.config).then_some(value);
        }
    }
    None
}

/// The stored value of a text for a property (a folder name read by a rule, say). For an open list a new label
/// joins the list, so `def` may change: the caller saves it if `def.config` differs. None when the text is not a
/// value of the type (a closed list that does not have it, a number that is not one...).
pub fn parse_input(def: &mut MetaDef, text: &str, today: chrono::NaiveDate) -> Option<String> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    match def.ty {
        MetaType::Text | MetaType::LongText => Some(text.to_owned()),
        MetaType::Number => t.replace(',', ".").parse::<f64>().ok().filter(|n| n.is_finite()).map(|n| n.to_string()),
        MetaType::Date => crate::date_input::parse_date(t, today).map(|d| d.and_time(chrono::NaiveTime::MIN).and_utc().timestamp().to_string()),
        MetaType::Bool => match t.to_lowercase().as_str() {
            "1" | "oui" | "yes" | "true" | "vrai" => Some("1".into()),
            "0" | "non" | "no" | "false" | "faux" => Some("0".into()),
            _ => None,
        },
        MetaType::Choice => {
            let id = match def.config.find(t) {
                Some(id) => id,
                None if def.config.mode == ListMode::Open => def.config.add_option(t)?,
                None => return None,
            };
            Some(encode_ids(&[id], def.config.multiple))
        }
        MetaType::Formula => None,
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

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn an_item_has_its_own_value_then_the_one_of_the_nearest_folder() {
        let v = map(&[("/p", "1"), ("/p/a", "2"), ("/p/a/f.txt", "3")]);
        assert_eq!(resolve(&v, "/p/a/f.txt", '/', || false), Resolved::Own("3".into()));
        assert_eq!(resolve(&v, "/p/a/g.txt", '/', || false), Resolved::Inherited { value: "2".into(), from: "/p/a".into() });
        assert_eq!(resolve(&v, "/p/b/g.txt", '/', || false), Resolved::Inherited { value: "1".into(), from: "/p".into() });
        assert_eq!(resolve(&v, "/q/g.txt", '/', || false), Resolved::None);
        assert_eq!(resolve(&v, "/pp/g.txt", '/', || false), Resolved::None, "/pp is not below /p");
    }

    #[test]
    fn a_folder_without_value_sums_up_what_is_below_it() {
        let v = map(&[("/p/a/1", "5"), ("/p/a/2", "9"), ("/p/b/1", "5"), ("/pp/x", "100"), ("/p0", "100")]);
        let Resolved::Summary(s) = resolve(&v, "/p", '/', || true) else { panic!("a summary") };
        assert_eq!(s.items, 3, "/pp and /p0 are not below /p");
        assert_eq!(s.values, vec![("5".to_owned(), 2), ("9".to_owned(), 1)]);
        assert_eq!(resolve(&v, "/p", '/', || false), Resolved::None, "a file has no summary");
        assert_eq!(resolve(&v, "/empty", '/', || true), Resolved::None);
    }

    #[test]
    fn summaries_read_by_type() {
        let num = MetaDef { id: 1, name: "n".into(), ty: MetaType::Number, description: String::new(), config: Config::default() };
        let s = Summary { values: vec![("5".into(), 2), ("9".into(), 1)], items: 3 };
        assert_eq!(summary_text(&num, &s, &|d| d.to_string()), "5 – 9");
        assert_eq!(resolved_column_value(&num, &Resolved::Summary(s.clone())), Value::Num(5.0));
        let one = Summary { values: vec![("5".into(), 3)], items: 3 };
        assert_eq!(summary_text(&num, &one, &|d| d.to_string()), "5");
        let boolean = MetaDef { ty: MetaType::Bool, ..num.clone() };
        let b = Summary { values: vec![("1".into(), 2), ("0".into(), 1)], items: 3 };
        assert_eq!(summary_text(&boolean, &b, &|d| d.to_string()), "2/3 oui");
        let mut list = choice(true);
        list.config.add_option("Archivé");
        let c = Summary { values: vec![("[3,1]".into(), 1), ("2".into(), 1)], items: 2 };
        assert_eq!(summary_text(&list, &c, &|d| d.to_string()), "À faire, En cours, Fini", "in the order of the list");
        let many = Summary { values: vec![("[1,2,3,4]".into(), 1)], items: 1 };
        assert_eq!(summary_text(&list, &many, &|d| d.to_string()), "À faire, En cours, Fini (+1)");
    }

    #[test]
    fn a_resolved_value_is_written_with_its_type() {
        let day = MetaDef { id: 1, name: "d".into(), ty: MetaType::Date, description: String::new(), config: Config::default() };
        let r = Resolved::Inherited { value: "86400".into(), from: "/p".into() };
        assert_eq!(resolved_text(&day, &r, &|d| format!("jour {d}")), "jour 86400");
        assert_eq!(resolved_text(&day, &Resolved::None, &|d| d.to_string()), "");
    }

    #[test]
    fn a_formula_fills_its_names_like_an_f_string() {
        let var = |n: &str| match n {
            "filename" => Some("rapport.pdf".to_owned()),
            "Statut" => Some("en cours".to_owned()),
            "prix" => Some("3.14159".to_owned()),
            "n" => Some("7".to_owned()),
            _ => None,
        };
        assert_eq!(eval_formula("{filename} {Statut}", &var), "rapport.pdf en cours");
        assert_eq!(eval_formula("{ Statut :upper}", &var), "EN COURS");
        assert_eq!(eval_formula("{Statut:title}", &var), "En Cours");
        assert_eq!(eval_formula("{prix:.2f} €", &var), "3.14 €");
        assert_eq!(eval_formula("n°{n:03d}", &var), "n°007");
        assert_eq!(eval_formula("{{filename}} = {filename}", &var), "{filename} = rapport.pdf");
        assert_eq!(eval_formula("[{unknown}]", &var), "[]");
        assert_eq!(eval_formula("open {Statut", &var), "open {Statut", "an unclosed brace stays as it is");
        assert_eq!(eval_formula("{prix:wat}", &var), "3.14159", "a spec that is not known leaves the value");
    }

    #[test]
    fn items_have_file_variables() {
        let p = std::path::Path::new("/tmp/Projet/rapport.final.pdf");
        assert_eq!(file_var(p, "filename").as_deref(), Some("rapport.final.pdf"));
        assert_eq!(file_var(p, "STEM").as_deref(), Some("rapport.final"));
        assert_eq!(file_var(p, "ext").as_deref(), Some("pdf"));
        assert_eq!(file_var(p, "folder").as_deref(), Some("Projet"));
        assert_eq!(file_var(p, "size").as_deref(), Some(""), "a file that is not there has no size");
        assert_eq!(file_var(p, "other"), None);
    }

    #[test]
    fn a_pattern_names_the_folders_under_a_root() {
        let p = Pattern::parse("{project}/{entreprise}/*/{chapter}").unwrap();
        assert_eq!(p.names(), vec!["project", "entreprise", "chapter"]);
        assert_eq!(p.depth(), 4);
        assert_eq!(p.assign("/data", "/data/Alpha", '/'), vec![("project".into(), "Alpha".into())]);
        assert_eq!(p.assign("/data/", "/data/Alpha/ACME", '/'), vec![("entreprise".into(), "ACME".into())], "a slash at the end of the root is fine");
        assert_eq!(p.assign("/data", "/data/Alpha/ACME/misc", '/'), Vec::<(String, String)>::new(), "a level that is not named");
        assert_eq!(p.assign("/data", "/data/Alpha/ACME/misc/Ch1", '/'), vec![("chapter".into(), "Ch1".into())]);
        assert_eq!(p.assign("/data", "/data/Alpha/ACME/misc/Ch1/deeper", '/'), Vec::<(String, String)>::new(), "below the pattern");
        assert_eq!(p.assign("/data", "/data", '/'), Vec::<(String, String)>::new(), "the root gives nothing");
        assert_eq!(p.assign("/data", "/database/x", '/'), Vec::<(String, String)>::new(), "/database is not under /data");
        assert_eq!(Pattern::parse("a/b"), None);
        assert_eq!(Pattern::parse("{}"), None);
        // a text is a filter: the folder at that level must have this name
        let q = Pattern::parse("{client}/00 - DOE Livrés/{projet}").unwrap();
        assert_eq!(q.specificity(), 1);
        assert_eq!(q.assign("/d", "/d/Acme", '/'), vec![("client".into(), "Acme".into())]);
        assert_eq!(q.assign("/d", "/d/Acme/00 - DOE Livrés", '/'), Vec::<(String, String)>::new(), "the folder of the text gives nothing");
        assert_eq!(q.assign("/d", "/d/Acme/00 - doe livrés/Tour A", '/'), vec![("projet".into(), "Tour A".into())]);
        assert_eq!(q.assign("/d", "/d/Acme/Autre/Tour A", '/'), Vec::<(String, String)>::new(), "another name at that level");
        assert!(Pattern::parse("{a}\\{b}").is_some(), "both separators");
    }

    #[test]
    fn a_level_can_set_a_value() {
        let q = Pattern::parse("{client}/00 - DOE Livrés [Statut=Terminé]/{projet}").unwrap();
        assert_eq!(q.names(), vec!["client", "Statut", "projet"]);
        assert_eq!(q.specificity(), 1);
        assert_eq!(q.assign("/d", "/d/Acme/00 - doe livrés", '/'), vec![("Statut".into(), "Terminé".into())]);
        assert_eq!(q.assign("/d", "/d/Acme/00 - doe livrés/Tour A", '/'), vec![("projet".into(), "Tour A".into())]);
        assert_eq!(q.assign("/d", "/d/Acme/Autre", '/'), Vec::<(String, String)>::new());
        assert!(Pattern::parse("Livrés [Statut=Terminé]").is_some(), "a rule may only set values");
        assert!(Pattern::parse("a [Statut=]").is_none());
        let not = Pattern::parse("{client}/{projet!00 - DOE Livrés|Archives} [Statut=En cours]/{Étape}").unwrap();
        assert_eq!(not.names(), vec!["client", "projet", "Statut", "Étape"]);
        assert_eq!(not.assign("/d", "/d/Acme/Tour", '/'), vec![("projet".into(), "Tour".into()), ("Statut".into(), "En cours".into())]);
        assert!(not.assign("/d", "/d/Acme/00 - doe livrés", '/').is_empty(), "refused name");
        assert!(not.assign("/d", "/d/Acme/archives/01 - x", '/').is_empty(), "and what is below it");
        assert_eq!(not.assign("/d", "/d/Acme/Tour/01 - x", '/'), vec![("Étape".into(), "01 - x".into())]);
        let two = Pattern::parse("{a}[X=1; Y=2]").unwrap();
        assert_eq!(two.assign("/d", "/d/q", '/'), vec![("a".into(), "q".into()), ("X".into(), "1".into()), ("Y".into(), "2".into())]);
    }

    #[test]
    fn rules_give_a_value_to_a_folder_the_index_does_not_know() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let def = MetaDef { id: 1, name: "projet".into(), ty: MetaType::Text, description: String::new(), config: Config::default() };
        let rules: RuleSet = vec![("/d".into(), Pattern::parse("{client}/00 - DOE Livrés/{projet}").unwrap())];
        let values = BTreeMap::new();
        let virt = |p: &str, is_self: bool| if is_self && p.ends_with(".pdf") { None } else { rule_value(&rules, &def, p, '/', today) };
        let found = resolve_with(&values, "/d/Acme/00 - DOE Livrés/Tour A/x.pdf", '/', || false, &virt);
        assert_eq!(found, Resolved::Inherited { value: "Tour A".into(), from: "/d/Acme/00 - DOE Livrés/Tour A".into() });
        assert_eq!(resolve_with(&values, "/d/Acme/00 - DOE Livrés", '/', || false, &virt), Resolved::None, "the folder of the text gives nothing");
    }

    #[test]
    fn a_text_becomes_a_value_of_the_type() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let mut d = MetaDef { id: 1, name: "x".into(), ty: MetaType::Number, description: String::new(), config: Config::default() };
        assert_eq!(parse_input(&mut d, " 3,5 ", today).as_deref(), Some("3.5"));
        assert_eq!(parse_input(&mut d, "abc", today), None);
        d.ty = MetaType::Bool;
        assert_eq!(parse_input(&mut d, "Oui", today).as_deref(), Some("1"));
        assert_eq!(parse_input(&mut d, "peut-être", today), None);
        d.ty = MetaType::Date;
        assert_eq!(parse_input(&mut d, "02/01/1970", today).as_deref(), Some("86400"));
        let mut list = choice(false);
        assert_eq!(parse_input(&mut list, "en cours", today).as_deref(), Some("2"));
        assert_eq!(parse_input(&mut list, "Nouveau", today), None, "a closed list does not grow");
        list.config.mode = ListMode::Open;
        assert_eq!(parse_input(&mut list, "Nouveau", today).as_deref(), Some("4"));
        assert_eq!(list.config.label_of(4), Some("Nouveau"));
        list.config.multiple = true;
        assert_eq!(parse_input(&mut list, "Fini", today).as_deref(), Some("[3]"));
        d.ty = MetaType::Formula;
        assert_eq!(parse_input(&mut d, "x", today), None);
    }
}
