//! The meta properties in the base: their definitions (`meta_defs`) and the values put on files and folders
//! (`meta_values`). What a value means is in `pyegui_core::meta`.

use std::collections::HashMap;
use std::path::Path;

use pyegui_core::meta::{decode_ids, encode_ids, parse_input, Config, MetaDef, MetaType, Pattern};
use rusqlite::{params, OptionalExtension};

use crate::tagstore::{key, TagStore};

impl TagStore {
    /// The definitions, in their order
    pub fn meta_defs(&self) -> Vec<MetaDef> {
        let Ok(mut statement) = self.db.prepare("SELECT id, name, type, description, config FROM meta_defs ORDER BY position, id") else { return vec![] };
        statement
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?)))
            .map(|rows| {
                rows.flatten()
                    .filter_map(|(id, name, ty, description, config)| Some(MetaDef { id, name, ty: MetaType::parse(&ty)?, description, config: Config::parse(&config) }))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn meta_def(&self, id: i64) -> Option<MetaDef> {
        self.meta_defs().into_iter().find(|d| d.id == id)
    }

    /// Makes a property; None if the name is empty or taken (case ignored)
    pub fn create_meta(&mut self, name: &str, ty: MetaType) -> Option<i64> {
        let name = name.trim();
        if name.is_empty() || self.meta_defs().iter().any(|d| d.name.to_lowercase() == name.to_lowercase()) {
            return None;
        }
        let position: i64 = self.db.query_row("SELECT COALESCE(MAX(position) + 1, 0) FROM meta_defs", [], |r| r.get(0)).unwrap_or(0);
        self.db.execute("INSERT INTO meta_defs (name, type, config, position) VALUES (?, ?, ?, ?)", params![name, ty.name(), Config::default().to_json(), position]).ok()?;
        let id = self.db.last_insert_rowid();
        self.changed();
        Some(id)
    }

    /// Writes the name, the description and the configuration of a property (its type stays). False if the
    /// name is empty or another property has it.
    pub fn update_meta(&mut self, def: &MetaDef) -> bool {
        let name = def.name.trim();
        if name.is_empty() || self.meta_defs().iter().any(|d| d.id != def.id && d.name.to_lowercase() == name.to_lowercase()) {
            return false;
        }
        let done = self.db.execute("UPDATE meta_defs SET name = ?, description = ?, config = ? WHERE id = ?", params![name, def.description, def.config.to_json(), def.id]).is_ok();
        self.changed();
        done
    }

    /// Deletes a property and, with it, all its values
    pub fn delete_meta(&mut self, id: i64) {
        let _ = self.db.execute("DELETE FROM meta_defs WHERE id = ?", params![id]);
        self.changed();
    }

    /// Moves a property by `delta` places in the list
    pub fn move_meta(&mut self, id: i64, delta: isize) {
        let mut ids: Vec<i64> = self.meta_defs().iter().map(|d| d.id).collect();
        let Some(at) = ids.iter().position(|i| *i == id) else { return };
        let to = (at as isize + delta).clamp(0, ids.len() as isize - 1) as usize;
        let moved = ids.remove(at);
        ids.insert(to, moved);
        for (position, id) in ids.iter().enumerate() {
            let _ = self.db.execute("UPDATE meta_defs SET position = ? WHERE id = ?", params![position as i64, id]);
        }
        self.changed();
    }

    /// The stored value of a property on an item (its own, not an inherited one)
    pub fn meta_value(&self, path: &Path, def: i64) -> Option<String> {
        self.db.query_row("SELECT value FROM meta_values WHERE path = ? AND def_id = ?", params![key(path), def], |r| r.get(0)).optional().ok().flatten()
    }

    /// All the own values of an item, by property
    pub fn meta_values_of(&self, path: &Path) -> HashMap<i64, String> {
        let Ok(mut statement) = self.db.prepare("SELECT def_id, value FROM meta_values WHERE path = ?") else { return HashMap::new() };
        statement.query_map(params![key(path)], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    /// Every own value of a property, by stored path (what inheritance and summaries are computed from)
    pub fn meta_all(&self, def: i64) -> std::collections::BTreeMap<String, String> {
        let Ok(mut statement) = self.db.prepare("SELECT path, value FROM meta_values WHERE def_id = ?") else { return Default::default() };
        statement.query_map(params![def], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    /// Puts a value on an item; `None` or a blank text removes it
    pub fn set_meta_value(&mut self, path: &Path, def: i64, value: Option<&str>) {
        match value.filter(|v| !v.trim().is_empty()) {
            None => {
                let _ = self.db.execute("DELETE FROM meta_values WHERE path = ? AND def_id = ?", params![key(path), def]);
            }
            Some(v) => {
                let _ = self.db.execute(
                    "INSERT INTO meta_values (path, def_id, value, source) VALUES (?, ?, ?, 'user') \
                     ON CONFLICT(path, def_id) DO UPDATE SET value = excluded.value, source = 'user'",
                    params![key(path), def, v],
                );
            }
        }
        self.changed();
    }

    /// How many items hold a value of a property
    pub fn meta_usage(&self, def: i64) -> usize {
        self.db.query_row("SELECT COUNT(*) FROM meta_values WHERE def_id = ?", params![def], |r| r.get::<_, i64>(0)).unwrap_or(0) as usize
    }

    /// Adds an option to a choice (an open list grows as people type); its id, or the one that exists
    pub fn add_choice_option(&mut self, def: i64, label: &str) -> Option<i64> {
        let mut d = self.meta_def(def).filter(|d| d.ty == MetaType::Choice)?;
        let id = d.config.add_option(label)?;
        self.update_meta(&d).then_some(id)
    }

    /// The values that hold an option: how many items would lose it
    pub fn choice_usage(&self, def: i64, option: i64) -> usize {
        self.choice_rows(def).iter().filter(|(_, ids)| ids.contains(&option)).count()
    }

    fn choice_rows(&self, def: i64) -> Vec<(String, Vec<i64>)> {
        let Ok(mut statement) = self.db.prepare("SELECT path, value FROM meta_values WHERE def_id = ?") else { return vec![] };
        statement.query_map(params![def], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map(|rows| rows.flatten().map(|(p, v)| (p, decode_ids(&v))).collect()).unwrap_or_default()
    }

    /// Removes an option from the list and from the values that hold it (a value left empty goes away)
    pub fn remove_choice_option(&mut self, def: i64, option: i64) {
        self.replace_choice_option(def, option, None);
    }

    /// Merges an option into another: the items that held `from` hold `into` (once) and `from` leaves the list
    pub fn merge_choice_option(&mut self, def: i64, from: i64, into: i64) {
        if from != into {
            self.replace_choice_option(def, from, Some(into));
        }
    }

    fn replace_choice_option(&mut self, def: i64, from: i64, into: Option<i64>) {
        let Some(mut d) = self.meta_def(def).filter(|d| d.ty == MetaType::Choice) else { return };
        let multiple = d.config.multiple;
        for (path, mut ids) in self.choice_rows(def) {
            if !ids.contains(&from) {
                continue;
            }
            let at = ids.iter().position(|i| *i == from).unwrap_or(0);
            match into {
                Some(to) if !ids.contains(&to) => ids[at] = to,
                _ => {
                    ids.remove(at);
                }
            }
            let text = encode_ids(&ids, multiple);
            if text.is_empty() {
                let _ = self.db.execute("DELETE FROM meta_values WHERE path = ? AND def_id = ?", params![path, def]);
            } else {
                let _ = self.db.execute("UPDATE meta_values SET value = ? WHERE path = ? AND def_id = ?", params![text, path, def]);
            }
        }
        d.config.remove_option(from);
        self.update_meta(&d);
    }
}

/// A rule of the indexation: under `root`, the folders are read with `pattern` (`{project}/{entreprise}`) and what
/// they are called goes in the properties of those names
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub id: i64,
    pub root: String,
    pub pattern: String,
    pub enabled: bool,
}

impl TagStore {
    /// The rules, the most precise first: more fixed names in the pattern, then a deeper root, then the older
    fn by_precision(rules: &[Rule]) -> Vec<&Rule> {
        let mut order: Vec<&Rule> = rules.iter().collect();
        order.sort_by_key(|r| (std::cmp::Reverse(Pattern::parse(&r.pattern).map_or(0, |p| p.specificity())), std::cmp::Reverse(r.root.len()), r.id));
        order
    }

    /// The enabled rules with a valid pattern, the most precise first: what `rule_value` reads
    pub fn ordered_rules(&self) -> pyegui_core::meta::RuleSet {
        let rules = self.meta_rules();
        Self::by_precision(&rules).into_iter().filter(|r| r.enabled).filter_map(|r| Some((r.root.clone(), Pattern::parse(&r.pattern)?))).collect()
    }

    pub fn meta_rules(&self) -> Vec<Rule> {
        let Ok(mut statement) = self.db.prepare("SELECT id, root, pattern, enabled FROM meta_rules ORDER BY id") else { return vec![] };
        statement
            .query_map([], |r| Ok(Rule { id: r.get(0)?, root: r.get(1)?, pattern: r.get(2)?, enabled: r.get::<_, i64>(3)? != 0 }))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    /// Adds a rule; None if the pattern names nothing
    pub fn add_meta_rule(&mut self, root: &Path, pattern: &str) -> Option<i64> {
        Pattern::parse(pattern)?;
        self.db.execute("INSERT INTO meta_rules (root, pattern, enabled) VALUES (?, ?, 1)", params![key(root), pattern.trim()]).ok()?;
        self.changed();
        Some(self.db.last_insert_rowid())
    }

    pub fn update_meta_rule(&mut self, rule: &Rule) {
        let _ = self.db.execute("UPDATE meta_rules SET root = ?, pattern = ?, enabled = ? WHERE id = ?", params![rule.root, rule.pattern.trim(), rule.enabled as i64, rule.id]);
        self.changed();
    }

    /// Deletes a rule and the values it wrote
    pub fn delete_meta_rule(&mut self, id: i64) {
        let _ = self.db.execute("DELETE FROM meta_values WHERE source = ?", params![format!("auto:{id}")]);
        let _ = self.db.execute("DELETE FROM meta_rules WHERE id = ?", params![id]);
        self.changed();
    }

    /// Reads the folders known to the index again with every rule and writes the values they give, on the folders
    /// themselves (the items below inherit them). A value typed by the person is never replaced: the values a rule
    /// wrote (source `auto:<id>`) are removed first and put again, the others stay. Returns the number of values
    /// written and the names of the properties that a pattern uses and that do not exist.
    pub fn apply_meta_rules(&mut self) -> (usize, Vec<String>) {
        let rules = self.meta_rules();
        let mut defs = self.meta_defs();
        let today = chrono::Local::now().date_naive();
        let sep = std::path::MAIN_SEPARATOR;
        let (mut written, mut unknown) = (0, Vec::new());
        let _ = self.db.execute_batch("BEGIN");
        for rule in &rules {
            let _ = self.db.execute("DELETE FROM meta_values WHERE source = ?", params![format!("auto:{}", rule.id)]);
        }
        // the most precise rule writes first (OR IGNORE below leaves the folder to it)
        for rule in Self::by_precision(&rules) {
            let source = format!("auto:{}", rule.id);
            let Some(pattern) = Pattern::parse(&rule.pattern).filter(|_| rule.enabled) else { continue };
            for name in pattern.names() {
                if !defs.iter().any(|d| d.name.to_lowercase() == name.to_lowercase()) && !unknown.iter().any(|u: &String| u.to_lowercase() == name.to_lowercase()) {
                    unknown.push(name.to_owned());
                }
            }
            let prefix = format!("{}{sep}", rule.root.trim_end_matches(sep));
            let folders: Vec<String> = match self.db.prepare("SELECT path FROM files WHERE is_dir = 1 AND substr(path, 1, ?) = ?") {
                Ok(mut st) => st.query_map(params![prefix.chars().count() as i64, prefix], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default(),
                Err(_) => continue,
            };
            for folder in folders {
                for (name, text) in pattern.assign(&rule.root, &folder, sep) {
                    let Some(def) = defs.iter_mut().find(|d| d.name.to_lowercase() == name.to_lowercase()) else { continue };
                    let before = def.config.clone();
                    let Some(value) = parse_input(def, &text, today) else { continue };
                    if def.config != before {
                        let _ = self.db.execute("UPDATE meta_defs SET config = ? WHERE id = ?", params![def.config.to_json(), def.id]);
                    }
                    // OR IGNORE: a value that is already there (typed, or from another rule) stays
                    written += self.db.execute("INSERT OR IGNORE INTO meta_values (path, def_id, value, source) VALUES (?, ?, ?, ?)", params![folder, def.id, value, source]).unwrap_or(0);
                }
            }
        }
        let _ = self.db.execute_batch("COMMIT");
        self.changed();
        (written, unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> TagStore {
        TagStore::open(Some(Path::new(":memory:"))).unwrap()
    }

    #[test]
    fn a_property_is_made_once_per_name_and_keeps_its_order() {
        let mut s = store();
        let a = s.create_meta("Statut", MetaType::Choice).unwrap();
        let b = s.create_meta("Prix", MetaType::Number).unwrap();
        assert_eq!(s.create_meta("statut", MetaType::Text), None, "name taken, case ignored");
        assert_eq!(s.create_meta("  ", MetaType::Text), None);
        assert_eq!(s.meta_defs().iter().map(|d| d.id).collect::<Vec<_>>(), vec![a, b]);
        s.move_meta(b, -1);
        assert_eq!(s.meta_defs().iter().map(|d| d.id).collect::<Vec<_>>(), vec![b, a]);
    }

    #[test]
    fn a_definition_is_edited_but_not_renamed_onto_another() {
        let mut s = store();
        let a = s.create_meta("Statut", MetaType::Text).unwrap();
        s.create_meta("Prix", MetaType::Number).unwrap();
        let mut d = s.meta_def(a).unwrap();
        d.description = "où en est le dossier".into();
        d.name = "Prix".into();
        assert!(!s.update_meta(&d));
        d.name = "État".into();
        assert!(s.update_meta(&d));
        let back = s.meta_def(a).unwrap();
        assert_eq!((back.name.as_str(), back.description.as_str()), ("État", "où en est le dossier"));
    }

    #[test]
    fn values_are_set_read_and_removed_and_follow_their_property() {
        let mut s = store();
        let def = s.create_meta("Prix", MetaType::Number).unwrap();
        let file = Path::new("/tmp/a.txt");
        assert_eq!(s.meta_value(file, def), None);
        s.set_meta_value(file, def, Some("12"));
        s.set_meta_value(file, def, Some("15"));
        assert_eq!(s.meta_value(file, def).as_deref(), Some("15"));
        assert_eq!(s.meta_values_of(file).get(&def).map(String::as_str), Some("15"));
        assert_eq!(s.meta_usage(def), 1);
        s.set_meta_value(file, def, Some("  "));
        assert_eq!(s.meta_value(file, def), None, "a blank number is no value");
        s.set_meta_value(file, def, Some("1"));
        s.delete_meta(def);
        assert_eq!(s.meta_values_of(file).len(), 0, "the values of a deleted property go with it");
    }

    #[test]
    fn a_choice_grows_and_removing_or_merging_an_option_updates_the_values() {
        let mut s = store();
        let def = s.create_meta("Tags", MetaType::Choice).unwrap();
        let mut d = s.meta_def(def).unwrap();
        d.config.multiple = true;
        assert!(s.update_meta(&d));
        let (a, b, c) = (s.add_choice_option(def, "a").unwrap(), s.add_choice_option(def, "b").unwrap(), s.add_choice_option(def, "c").unwrap());
        assert_eq!(s.add_choice_option(def, "A"), Some(a));
        let (f1, f2, f3) = (Path::new("/tmp/1"), Path::new("/tmp/2"), Path::new("/tmp/3"));
        s.set_meta_value(f1, def, Some(&encode_ids(&[a, b], true)));
        s.set_meta_value(f2, def, Some(&encode_ids(&[b], true)));
        s.set_meta_value(f3, def, Some(&encode_ids(&[a, c], true)));
        assert_eq!(s.choice_usage(def, b), 2);
        s.merge_choice_option(def, b, a);
        assert_eq!(decode_ids(&s.meta_value(f1, def).unwrap()), vec![a], "a was already there: no duplicate");
        assert_eq!(decode_ids(&s.meta_value(f2, def).unwrap()), vec![a]);
        assert!(s.meta_def(def).unwrap().config.option(b).is_none());
        s.remove_choice_option(def, a);
        assert_eq!(s.meta_value(f1, def), None, "an emptied value goes away");
        assert_eq!(decode_ids(&s.meta_value(f3, def).unwrap()), vec![c]);
    }

    #[test]
    fn all_the_values_of_a_property_resolve_through_the_folders() {
        use pyegui_core::meta::{resolve, Resolved};
        let mut s = store();
        let def = s.create_meta("Client", MetaType::Text).unwrap();
        s.set_meta_value(Path::new("/tmp/p"), def, Some("ACME"));
        s.set_meta_value(Path::new("/tmp/q/x"), def, Some("Zed"));
        let all = s.meta_all(def);
        assert_eq!(all.len(), 2);
        let found = resolve(&all, &crate::tagstore::key(Path::new("/tmp/p/a/b.txt")), std::path::MAIN_SEPARATOR, || false);
        assert!(matches!(found, Resolved::Inherited { ref value, .. } if value == "ACME"));
        assert!(matches!(resolve(&all, &crate::tagstore::key(Path::new("/tmp/q")), std::path::MAIN_SEPARATOR, || true), Resolved::Summary(_)));
    }

    #[test]
    fn a_rule_reads_the_folder_names_and_never_replaces_a_typed_value() {
        let mut s = store();
        let project = s.create_meta("Project", MetaType::Choice).unwrap();
        let mut d = s.meta_def(project).unwrap();
        d.config.mode = pyegui_core::meta::ListMode::Open;
        s.update_meta(&d);
        let year = s.create_meta("Year", MetaType::Number).unwrap();
        for (path, dir) in [("/data/Alpha", true), ("/data/Alpha/2024", true), ("/data/Alpha/2024/f.txt", false), ("/data/Beta", true), ("/data/Beta/xx", true), ("/other/Gamma", true)] {
            s.db.execute("INSERT INTO files (path, is_dir) VALUES (?, ?)", params![path, dir as i64]).unwrap();
        }
        let rule = s.add_meta_rule(Path::new("/data"), "{project}/{year}").unwrap();
        assert_eq!(s.add_meta_rule(Path::new("/data"), "no names"), None);
        s.set_meta_value(Path::new("/data/Beta"), project, Some("1"));
        let (written, unknown) = s.apply_meta_rules();
        assert!(unknown.is_empty());
        assert_eq!(written, 2, "Alpha and Alpha/2024; Beta is typed and Beta/xx is not a number");
        let label = |p: &str| s.meta_value(Path::new(p), project).and_then(|v| pyegui_core::meta::decode_ids(&v).first().copied()).and_then(|id| s.meta_def(project).unwrap().config.label_of(id).map(str::to_owned));
        assert_eq!(label("/data/Alpha").as_deref(), Some("Alpha"));
        assert_eq!(s.meta_value(Path::new("/data/Alpha/2024"), year).as_deref(), Some("2024"));
        assert_eq!(s.meta_value(Path::new("/other/Gamma"), project), None, "outside the root");
        assert_eq!(s.meta_value(Path::new("/data/Beta"), project).as_deref(), Some("1"), "typed value kept");
        // applying again changes nothing; disabling a rule takes its values away
        assert_eq!(s.apply_meta_rules().0, 2);
        let mut r = s.meta_rules()[0].clone();
        r.enabled = false;
        s.update_meta_rule(&r);
        s.apply_meta_rules();
        assert_eq!(s.meta_value(Path::new("/data/Alpha/2024"), year), None);
        assert_eq!(s.meta_value(Path::new("/data/Beta"), project).as_deref(), Some("1"));
        s.delete_meta_rule(rule);
        assert!(s.meta_rules().is_empty());
    }

    #[test]
    fn a_pattern_that_names_a_missing_property_is_reported() {
        let mut s = store();
        s.add_meta_rule(Path::new("/data"), "{Client}/{x}").unwrap();
        s.create_meta("client", MetaType::Text).unwrap();
        assert_eq!(s.apply_meta_rules(), (0, vec!["x".to_owned()]));
    }

    #[test]
    fn two_rules_with_a_fixed_folder_set_a_value_on_it_and_name_the_repos() {
        let mut s = store();
        s.create_meta("Repo", MetaType::Text).unwrap();
        let privacy = s.create_meta("privacy", MetaType::Choice).unwrap();
        let mut d = s.meta_def(privacy).unwrap();
        d.config.mode = pyegui_core::meta::ListMode::Open;
        let (perso, pro) = (d.config.add_option("perso").unwrap(), d.config.add_option("pro").unwrap());
        s.update_meta(&d);
        for (path, dir) in [("/V", true), ("/V/Github", true), ("/V/Github/a", true), ("/V/Github/a/x", false), ("/V/Github-pro", true), ("/V/Github-pro/b", true)] {
            s.db.execute("INSERT INTO files (path, is_dir) VALUES (?, ?)", params![path, dir as i64]).unwrap();
        }
        s.add_meta_rule(Path::new("/V"), "github[privacy=perso]/{repo}").unwrap();
        s.add_meta_rule(Path::new("/V"), "github-pro[privacy=pro]/{repo}").unwrap();
        let (_, unknown) = s.apply_meta_rules();
        assert!(unknown.is_empty());
        let get = |p: &str, def| s.meta_value(Path::new(p), def);
        assert_eq!(get("/V/Github", privacy), Some(perso.to_string()));
        assert_eq!(get("/V/Github-pro", privacy), Some(pro.to_string()));
        assert_eq!(get("/V/Github/a", 1).as_deref(), Some("a"));
        assert_eq!(get("/V/Github-pro/b", 1).as_deref(), Some("b"));
    }
}
