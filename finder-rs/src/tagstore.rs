//! The tags of the Finder, and the files it knows, kept in a SQLite database. The schema is the one of
//! `TagStore` of `examples/finder.py`: the Python and the Rust Finders can share one `tags.db`.
//!
//! A tag has a name, a color and an optional parent tag, so the tags form a tree. A tag is put on files
//! and folders. Two rules of inheritance, both computed when asked, never copied in the base:
//!
//! * an item has the tags of the folders that contain it;
//! * an item with a tag also has all the parent tags of that tag.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    color TEXT NOT NULL,
    parent_id INTEGER REFERENCES tags(id) ON DELETE SET NULL,
    position INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS items (
    path TEXT NOT NULL,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (path, tag_id)
);
CREATE INDEX IF NOT EXISTS items_tag ON items(tag_id);
CREATE TABLE IF NOT EXISTS files (
    path TEXT PRIMARY KEY,
    last_opened REAL,
    open_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS files_last_opened ON files(last_opened);
CREATE TABLE IF NOT EXISTS notes (
    path TEXT PRIMARY KEY,
    text TEXT NOT NULL,
    updated REAL NOT NULL
);
";

/// The colors new tags start from, one after the other (the same as `NEW_TAG_COLORS` of finder.py)
pub const NEW_TAG_COLORS: [&str; 8] = ["#ef4444", "#f97316", "#eab308", "#22c55e", "#06b6d4", "#3b82f6", "#8b5cf6", "#ec4899"];

fn find_in(all: &[Tag], parts: &[String]) -> Option<i64> {
    let mut parent = None;
    for part in parts {
        parent = Some(all.iter().find(|t| t.parent == parent && t.name.to_lowercase() == part.to_lowercase())?.id);
    }
    parent
}

fn path_of(all: &[Tag], tag: i64) -> Vec<String> {
    let mut names = Vec::new();
    let mut at = Some(tag);
    while let Some(t) = at.and_then(|id| all.iter().find(|t| t.id == id)) {
        names.push(t.name.clone());
        at = t.parent;
        if names.len() > 64 {
            break;
        }
    }
    names.reverse();
    names
}

/// Where a tag of an item comes from, strongest first
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum How {
    Own,
    Folder,
    Parent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub parent: Option<i64>,
    pub depth: usize,
}

pub struct TagStore {
    db: Connection,
    /// grows at every change, so a view can tell its cache is stale
    pub version: u64,
}

/// tags.db in the folder of the user data of the OS; PYEGUI_FINDER_TAGS overrides it
pub fn default_path() -> PathBuf {
    if let Some(p) = std::env::var_os("PYEGUI_FINDER_TAGS") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default();
    let base = if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Roaming"))
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local").join("share"))
    };
    base.join("Finder").join("tags.db")
}

/// How a path is stored: absolute, and lower case on Windows (the comparison of the OS)
pub fn key(path: &Path) -> String {
    let absolute = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(path) };
    let text = absolute.to_string_lossy().into_owned();
    if cfg!(windows) { text.to_lowercase() } else { text }
}

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

impl TagStore {
    pub fn open(path: Option<&Path>) -> rusqlite::Result<Self> {
        let db = match path {
            Some(p) if p.as_os_str() == ":memory:" => Connection::open_in_memory()?,
            other => {
                let path = other.map(Path::to_path_buf).unwrap_or_else(default_path);
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let db = Connection::open(path)?;
                db.busy_timeout(std::time::Duration::from_secs(2))?;
                // write-ahead log, like the Python store: a reader does not block a writer
                let _: String = db.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0)).unwrap_or_default();
                db
            }
        };
        db.execute_batch("PRAGMA foreign_keys = ON;")?;
        db.execute_batch(SCHEMA)?;
        Ok(TagStore { db, version: 0 })
    }

    fn changed(&mut self) {
        self.version += 1;
    }

    fn next_position(&self, parent: Option<i64>) -> i64 {
        self.db
            .query_row("SELECT COALESCE(MAX(position) + 1, 0) FROM tags WHERE parent_id IS ?", params![parent], |r| r.get(0))
            .unwrap_or(0)
    }

    // the tags

    /// A new tag, last among its siblings; returns its id
    pub fn create(&mut self, name: &str, color: &str, parent: Option<i64>) -> Option<i64> {
        let position = self.next_position(parent);
        self.db
            .execute("INSERT INTO tags (name, color, parent_id, position) VALUES (?, ?, ?, ?)", params![name, color, parent, position])
            .ok()?;
        self.changed();
        Some(self.db.last_insert_rowid())
    }

    pub fn rename(&mut self, tag: i64, name: &str) {
        let _ = self.db.execute("UPDATE tags SET name = ? WHERE id = ?", params![name, tag]);
        self.changed();
    }

    pub fn recolor(&mut self, tag: i64, color: &str) {
        let _ = self.db.execute("UPDATE tags SET color = ? WHERE id = ?", params![color, tag]);
        self.changed();
    }

    /// Puts a tag under another one (None: at the root). False if that would make a loop
    #[allow(dead_code)]
    pub fn move_tag(&mut self, tag: i64, parent: Option<i64>) -> bool {
        if parent.is_some_and(|p| self.subtree(tag).contains(&p)) {
            return false;
        }
        let position = self.next_position(parent);
        let _ = self.db.execute("UPDATE tags SET parent_id = ?, position = ? WHERE id = ?", params![parent, position, tag]);
        self.changed();
        true
    }

    /// Removes a tag and what was tagged with it; its children go up to its parent
    pub fn delete(&mut self, tag: i64) {
        let parent: Option<i64> = self.db.query_row("SELECT parent_id FROM tags WHERE id = ?", params![tag], |r| r.get(0)).ok().flatten();
        let _ = self.db.execute("UPDATE tags SET parent_id = ? WHERE parent_id = ?", params![parent, tag]);
        let _ = self.db.execute("DELETE FROM tags WHERE id = ?", params![tag]);
        self.changed();
    }

    /// Every tag, in the order of the tree
    pub fn all(&self) -> Vec<Tag> {
        let Ok(mut statement) = self.db.prepare("SELECT id, name, color, parent_id FROM tags ORDER BY position, id") else { return vec![] };
        let rows: Vec<(i64, String, String, Option<i64>)> = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default();
        let mut children: HashMap<Option<i64>, Vec<(i64, String, String)>> = HashMap::new();
        for (id, name, color, parent) in rows {
            children.entry(parent).or_default().push((id, name, color));
        }
        fn walk(children: &HashMap<Option<i64>, Vec<(i64, String, String)>>, parent: Option<i64>, depth: usize, out: &mut Vec<Tag>) {
            for (id, name, color) in children.get(&parent).into_iter().flatten() {
                out.push(Tag { id: *id, name: name.clone(), color: color.clone(), parent, depth });
                if depth < 64 {
                    walk(children, Some(*id), depth + 1, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&children, None, 0, &mut out);
        out
    }

    /// A tag and all the tags under it
    pub fn subtree(&self, tag: i64) -> HashSet<i64> {
        let Ok(mut statement) = self.db.prepare(
            "WITH RECURSIVE sub(id) AS (SELECT ? UNION SELECT tags.id FROM tags JOIN sub ON tags.parent_id = sub.id) SELECT id FROM sub",
        ) else {
            return HashSet::new();
        };
        statement.query_map(params![tag], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    /// The parent tags of a tag, from its parent up to the root
    pub fn lineage(&self, tag: i64) -> Vec<i64> {
        let Ok(mut statement) = self.db.prepare(
            "WITH RECURSIVE up(id, parent_id, level) AS (SELECT id, parent_id, 0 FROM tags WHERE id = ? \
             UNION SELECT tags.id, tags.parent_id, level + 1 FROM tags JOIN up ON tags.id = up.parent_id) \
             SELECT id FROM up WHERE level > 0 ORDER BY level",
        ) else {
            return vec![];
        };
        statement.query_map(params![tag], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    // tags by path: "Work/Invoice" is the tag Invoice under the tag Work

    /// "Work/ Invoice" -> ["Work", "Invoice"]: the "/" separate the levels of the tree of tags
    pub fn split_path(text: &str) -> Vec<String> {
        text.split('/').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect()
    }

    /// The names of a tag and of its parents, from the top level down
    pub fn tag_path(&self, tag: i64) -> Vec<String> {
        path_of(&self.all(), tag)
    }

    /// The tag at this path from the top level (names compared without case), or None
    pub fn find_path(&self, parts: &[String]) -> Option<i64> {
        let all = self.all();
        find_in(&all, parts)
    }

    /// The tag at this path, made with the levels above it that do not exist yet; a new tag takes the
    /// next color of `NEW_TAG_COLORS`. None for an empty path
    pub fn create_path(&mut self, parts: &[String]) -> Option<i64> {
        let mut parent = None;
        for i in 0..parts.len() {
            let existing = match self.find_path(&parts[..=i]) {
                Some(id) => id,
                None => {
                    let color = NEW_TAG_COLORS[self.all().len() % NEW_TAG_COLORS.len()];
                    self.create(&parts[i], color, parent)?
                }
            };
            parent = Some(existing);
        }
        parent
    }

    /// The tags found by what was typed: each typed level is a part of the name of the matching level,
    /// the last typed one against the tag itself ("w/inv" finds Work/Invoice)
    pub fn matching(&self, parts: &[String]) -> Vec<Tag> {
        let all = self.all();
        all.iter()
            .filter(|tag| {
                let names: Vec<String> = path_of(&all, tag.id).iter().map(|n| n.to_lowercase()).collect();
                parts.len() <= names.len()
                    && parts.iter().zip(&names[names.len() - parts.len()..]).all(|(p, n)| n.contains(&p.to_lowercase()))
            })
            .cloned()
            .collect()
    }

    /// A tag is offered for creation when the typed path does not exist; a single name that already
    /// exists somewhere in the tree is found, not offered again
    pub fn can_create(&self, parts: &[String]) -> bool {
        if parts.is_empty() || self.find_path(parts).is_some() {
            return false;
        }
        parts.len() > 1 || !self.all().iter().any(|t| t.name.to_lowercase() == parts[0].to_lowercase())
    }

    // the items

    pub fn tag(&mut self, path: &Path, tag: i64) {
        let _ = self.db.execute("INSERT OR IGNORE INTO items (path, tag_id) VALUES (?, ?)", params![key(path), tag]);
        self.changed();
    }

    pub fn untag(&mut self, path: &Path, tag: i64) {
        let _ = self.db.execute("DELETE FROM items WHERE path = ? AND tag_id = ?", params![key(path), tag]);
        self.changed();
    }

    /// The ids of the tags put on this very item
    pub fn own(&self, path: &Path) -> HashSet<i64> {
        let Ok(mut statement) = self.db.prepare("SELECT tag_id FROM items WHERE path = ?") else { return HashSet::new() };
        statement.query_map(params![key(path)], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    /// Every tag of an item with where it comes from: put on the item, on a folder that contains it, or
    /// the parent of another tag
    pub fn effective(&self, path: &Path) -> HashMap<i64, How> {
        let mut found: HashMap<i64, How> = self.own(path).into_iter().map(|t| (t, How::Own)).collect();
        let folders: Vec<String> = path.ancestors().skip(1).filter(|p| !p.as_os_str().is_empty()).map(key).collect();
        if !folders.is_empty() {
            let marks = vec!["?"; folders.len()].join(",");
            if let Ok(mut statement) = self.db.prepare(&format!("SELECT DISTINCT tag_id FROM items WHERE path IN ({marks})")) {
                if let Ok(rows) = statement.query_map(rusqlite::params_from_iter(folders.iter()), |r| r.get::<_, i64>(0)) {
                    for tag in rows.flatten() {
                        found.entry(tag).or_insert(How::Folder);
                    }
                }
            }
        }
        for tag in found.keys().copied().collect::<Vec<_>>() {
            for parent in self.lineage(tag) {
                found.entry(parent).or_insert(How::Parent);
            }
        }
        found
    }

    /// The paths that have a tag, or a tag under it, put on them: the roots of what the tag covers
    pub fn tagged_with(&self, tag: i64) -> Vec<String> {
        let ids: Vec<i64> = self.subtree(tag).into_iter().collect();
        let marks = vec!["?"; ids.len()].join(",");
        let Ok(mut statement) = self.db.prepare(&format!("SELECT DISTINCT path FROM items WHERE tag_id IN ({marks}) ORDER BY path")) else { return vec![] };
        statement.query_map(rusqlite::params_from_iter(ids.iter()), |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    // the files: any file or folder can be referenced, to remember when it was last opened

    /// The item was opened now
    pub fn opened(&mut self, path: &Path) {
        let _ = self.db.execute(
            "INSERT INTO files (path, last_opened, open_count) VALUES (?, ?, 1) \
             ON CONFLICT(path) DO UPDATE SET last_opened = excluded.last_opened, open_count = open_count + 1",
            params![key(path), now()],
        );
        self.changed();
    }

    /// When every item opened was last opened, by stored path
    pub fn opened_dates(&self) -> HashMap<String, f64> {
        let Ok(mut statement) = self.db.prepare("SELECT path, last_opened FROM files WHERE last_opened IS NOT NULL") else { return HashMap::new() };
        statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    /// The paths last opened, the latest first
    pub fn recently_opened(&self, limit: usize) -> Vec<(String, f64)> {
        let Ok(mut statement) = self.db.prepare("SELECT path, last_opened FROM files WHERE last_opened IS NOT NULL ORDER BY last_opened DESC LIMIT ?") else { return vec![] };
        statement.query_map(params![limit as i64], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    // the notes

    #[allow(dead_code)]
    pub fn note(&self, path: &Path) -> String {
        self.db.query_row("SELECT text FROM notes WHERE path = ?", params![key(path)], |r| r.get(0)).optional().ok().flatten().unwrap_or_default()
    }

    /// Writes the note of an item; an empty text removes it
    #[allow(dead_code)]
    pub fn set_note(&mut self, path: &Path, text: &str) {
        if text.trim().is_empty() {
            let _ = self.db.execute("DELETE FROM notes WHERE path = ?", params![key(path)]);
        } else {
            let _ = self.db.execute(
                "INSERT INTO notes (path, text, updated) VALUES (?, ?, ?) \
                 ON CONFLICT(path) DO UPDATE SET text = excluded.text, updated = excluded.updated",
                params![key(path), text, now()],
            );
        }
        self.changed();
    }

    /// The stored paths that have a note
    #[allow(dead_code)]
    pub fn noted(&self) -> HashSet<String> {
        let Ok(mut statement) = self.db.prepare("SELECT path FROM notes") else { return HashSet::new() };
        statement.query_map([], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> TagStore {
        TagStore::open(Some(Path::new(":memory:"))).unwrap()
    }

    #[test]
    fn a_tag_is_inherited_from_its_folders_and_its_children() {
        let mut s = store();
        let work = s.create("Work", "#3b82f6", None).unwrap();
        let invoice = s.create("Invoice", "#ef4444", Some(work)).unwrap();
        s.tag(Path::new("/docs/2026"), invoice);
        let found = s.effective(Path::new("/docs/2026/march.pdf"));
        assert_eq!(found.get(&invoice), Some(&How::Folder));
        assert_eq!(found.get(&work), Some(&How::Parent));
        assert_eq!(s.effective(Path::new("/docs/2026")).get(&invoice), Some(&How::Own));
        assert_eq!(s.tagged_with(work), vec![key(Path::new("/docs/2026"))]);
        assert!(s.effective(Path::new("/other")).is_empty());
    }

    #[test]
    fn the_tree_cannot_loop_and_deleting_lifts_the_children() {
        let mut s = store();
        let a = s.create("A", "#000", None).unwrap();
        let b = s.create("B", "#000", Some(a)).unwrap();
        let c = s.create("C", "#000", Some(b)).unwrap();
        assert!(!s.move_tag(a, Some(c)), "a loop");
        s.delete(b);
        let all = s.all();
        assert_eq!(all.iter().map(|t| (t.name.as_str(), t.depth)).collect::<Vec<_>>(), vec![("A", 0), ("C", 1)]);
        assert_eq!(all[1].parent, Some(a));
    }

    #[test]
    fn opened_files_come_back_latest_first_and_notes_can_be_removed() {
        let mut s = store();
        s.opened(Path::new("/a"));
        std::thread::sleep(std::time::Duration::from_millis(10));
        s.opened(Path::new("/b"));
        assert_eq!(s.recently_opened(10)[0].0, key(Path::new("/b")));
        s.set_note(Path::new("/a"), "hello");
        assert_eq!(s.note(Path::new("/a")), "hello");
        assert_eq!(s.noted().len(), 1);
        s.set_note(Path::new("/a"), "  ");
        assert!(s.noted().is_empty());
    }

    fn p(text: &str) -> Vec<String> {
        TagStore::split_path(text)
    }

    #[test]
    fn a_path_makes_the_missing_levels_and_finds_the_others() {
        let mut s = store();
        assert_eq!(p(" Work/ Invoice //"), vec!["Work", "Invoice"]);
        let invoice = s.create_path(&p("Work/Invoice")).unwrap();
        assert_eq!(s.all().len(), 2);
        assert_eq!(s.tag_path(invoice), vec!["Work", "Invoice"]);
        // again, in another case: nothing new
        assert_eq!(s.create_path(&p("work/INVOICE")), Some(invoice));
        assert_eq!(s.all().len(), 2);
        // a deeper path reuses the levels that exist and takes the next color
        let march = s.create_path(&p("Work/Invoice/March")).unwrap();
        assert_eq!(s.all().len(), 3);
        assert_eq!(s.all().iter().find(|t| t.id == march).unwrap().color, NEW_TAG_COLORS[2]);
        assert_eq!(s.find_path(&p("Work/Nope")), None);
        assert_eq!(s.create_path(&[]), None);
    }

    #[test]
    fn typing_finds_by_level_and_offers_to_create() {
        let mut s = store();
        s.create_path(&p("Work/Invoice"));
        s.create_path(&p("Home/Invoice"));
        let found: Vec<String> = s.matching(&p("w/inv")).iter().map(|t| t.name.clone()).collect();
        assert_eq!(found, vec!["Invoice"]);
        assert_eq!(s.matching(&p("inv")).len(), 2);
        assert!(s.matching(&p("a/b/c/d")).is_empty());
        assert!(!s.can_create(&p("Work/Invoice")));
        assert!(!s.can_create(&p("invoice")), "a name that exists somewhere is found, not offered");
        assert!(s.can_create(&p("Work/Tax")));
        assert!(s.can_create(&p("Tax")));
        assert!(!s.can_create(&[]));
    }

    #[test]
    fn rename_and_recolor_keep_the_same_row() {
        let mut s = store();
        let id = s.create_path(&p("A/B")).unwrap();
        s.rename(id, "C");
        s.recolor(id, "#123456");
        let t = s.all().into_iter().find(|t| t.id == id).unwrap();
        assert_eq!((t.name.as_str(), t.color.as_str()), ("C", "#123456"));
        assert_eq!(s.find_path(&p("a/c")), Some(id));
    }
}
