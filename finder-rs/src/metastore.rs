//! The meta properties in the base: their definitions (`meta_defs`) and the values put on files and folders
//! (`meta_values`). What a value means is in `pyegui_core::meta`.

use std::collections::HashMap;
use std::path::Path;

use pyegui_core::meta::{decode_ids, encode_ids, Config, MetaDef, MetaType};
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
}
