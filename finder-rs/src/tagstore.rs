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

pub(crate) const SCHEMA: &str = "
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
CREATE TABLE IF NOT EXISTS pdf_info (
    path TEXT PRIMARY KEY,
    size INTEGER, mtime REAL,  -- of the file when it was read: another one means read again
    pages INTEGER, encrypted INTEGER, needs_password INTEGER,
    can_print INTEGER, can_copy INTEGER, can_modify INTEGER,
    can_annotate INTEGER, can_fill_forms INTEGER, can_assemble INTEGER,
    title TEXT, author TEXT, subject TEXT, keywords TEXT, creator TEXT, producer TEXT,
    created REAL, modified REAL, pdf_version TEXT, error TEXT, read_at REAL
);
CREATE TABLE IF NOT EXISTS favorites (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    label TEXT NOT NULL,
    icon TEXT NOT NULL DEFAULT 'folder',
    position INTEGER NOT NULL DEFAULT 0
);
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

/// A favorite of the sidebar: a folder with the name and the icon it is shown with
#[derive(Clone, Debug, PartialEq)]
pub struct Favorite {
    pub id: i64,
    pub path: String,
    pub label: String,
    /// the name of a glyph (see `glyphs::by_name`)
    pub icon: String,
}

/// The favorites a new database starts with: the home folder and its usual folders that exist
fn default_favorites() -> Vec<(String, String, String)> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
    let mut out = vec![(home.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Départ".into()), "home".to_owned(), home.to_string_lossy().into_owned())];
    for (label, icon, name) in [
        ("Bureau", "desktop", "Desktop"),
        ("Documents", "document", "Documents"),
        ("Téléchargements", "download", "Downloads"),
        ("Images", "image", "Pictures"),
        ("Musique", "folder", "Music"),
        ("Vidéos", "folder", if cfg!(target_os = "macos") { "Movies" } else { "Videos" }),
    ] {
        let path = home.join(name);
        if path.is_dir() {
            out.push((label.to_owned(), icon.to_owned(), path.to_string_lossy().into_owned()));
        }
    }
    if cfg!(target_os = "macos") {
        out.push(("Applications".to_owned(), "folder".to_owned(), "/Applications".to_owned()));
    }
    out
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

/// The columns of `files` filled by the index (the same as `INDEX_COLUMNS` of finder.py), added to the bases made before it
const INDEX_COLUMNS: [(&str, &str); 7] = [
    ("name", "TEXT"),
    ("parent", "TEXT"),
    ("is_dir", "INTEGER"),
    ("size", "INTEGER"),
    ("modified", "REAL"),
    ("last_seen", "REAL"),
    ("last_full_scan", "REAL"),
];

pub(crate) fn add_index_columns(db: &Connection) {
    let have: HashSet<String> = db
        .prepare("PRAGMA table_info(files)")
        .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(1)).map(|rows| rows.flatten().collect()))
        .unwrap_or_default();
    for (column, kind) in INDEX_COLUMNS {
        if !have.contains(column) {
            let _ = db.execute_batch(&format!("ALTER TABLE files ADD COLUMN {column} {kind}"));
        }
    }
}

fn secs(time: std::io::Result<SystemTime>) -> f64 {
    time.ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// Records a folder and its direct entries (no walk below) in the table `files`, what navigation does
/// when a folder is shown; the entries that vanished from it are dropped. It opens its own connection,
/// so it is meant for a thread. `None` when the folder cannot be read or the base is busy: the next visit tries again.
pub fn index_children(db_path: Option<&Path>, folder: &Path) -> Option<usize> {
    let mut db = open_index(db_path)?;
    record_folder(&mut db, folder, None).map(|(n, _)| n)
}

/// Reads the metadata (page count first) of the PDF among `rows` that are new or changed since they were
/// last read (their size or date is not the one of `pdf_info`), into the table `pdf_info` of finder.py.
#[cfg(feature = "pdf")]
fn read_pdfs(db: &mut Connection, rows: &[(String, String, bool, i64, f64)]) {
    use pyegui_core::pdf::{info_of_path, PERMISSIONS};
    for (path, name, is_dir, size, modified) in rows {
        if *is_dir || !name.to_lowercase().ends_with(".pdf") {
            continue;
        }
        let up_to_date = db
            .query_row("SELECT size, mtime FROM pdf_info WHERE path = ?", params![path], |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<f64>>(1)?)))
            .optional()
            .ok()
            .flatten()
            .is_some_and(|(s, m)| s == Some(*size) && m == Some(*modified));
        if up_to_date {
            continue;
        }
        let info = info_of_path(path);
        // a file that could not be read tells nothing about its encryption and its rights
        let known = info.error.is_none();
        let flag = |v: bool| known.then_some(i64::from(v));
        let allowed = |bit: u32| flag(info.permissions.is_none_or(|p| p & (1 << (bit - 1)) != 0));
        let rights: Vec<Option<i64>> = PERMISSIONS.iter().map(|(_, bit)| allowed(*bit)).collect();
        pyegui_core::dlog!("sqlite", "pdf_info: {path} recorded ({} pages{})", info.pages.map_or("?".to_string(), |n| n.to_string()), info.error.as_ref().map_or(String::new(), |e| format!(", error: {e}")));
        let _ = db.execute(
            "INSERT OR REPLACE INTO pdf_info (path, size, mtime, pages, encrypted, needs_password, can_print, can_copy, can_modify,
                 can_annotate, can_fill_forms, can_assemble, title, author, subject, keywords, creator, producer,
                 created, modified, pdf_version, error, read_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                path, size, modified, info.pages.map(|n| n as i64), flag(info.encrypted), flag(info.needs_password),
                rights[0], rights[1], rights[2], rights[3], rights[4], rights[5],
                info.title, info.author, info.subject, info.keywords, info.creator, info.producer,
                info.created, info.modified, info.pdf_version, info.error, now()
            ],
        );
    }
}

fn open_index(db_path: Option<&Path>) -> Option<Connection> {
    let path = db_path.map(Path::to_path_buf).unwrap_or_else(default_path);
    let db = Connection::open(path).ok()?;
    db.busy_timeout(std::time::Duration::from_secs(2)).ok()?;
    add_index_columns(&db);
    Some(db)
}

/// What a walk of `index_tree` has done, read by the window while it runs
#[derive(Default)]
pub struct IndexProgress {
    /// the entries recorded so far
    pub entries: std::sync::atomic::AtomicUsize,
    /// the folders read, and the folders known so far (read or waiting): the share of the work done is
    /// an estimate, the total grows as the walk meets sub-folders
    pub folders_done: std::sync::atomic::AtomicUsize,
    pub folders_known: std::sync::atomic::AtomicUsize,
    /// the folder being read
    pub current: std::sync::Mutex<String>,
    /// set by the window to stop the walk
    pub cancel: std::sync::atomic::AtomicBool,
}

impl IndexProgress {
    /// The share of the folders read, 0.0 to 1.0
    pub fn fraction(&self) -> f32 {
        use std::sync::atomic::Ordering::Relaxed;
        let known = self.folders_known.load(Relaxed);
        if known == 0 {
            0.0
        } else {
            (self.folders_done.load(Relaxed) as f32 / known as f32).min(1.0)
        }
    }
}

/// Records everything under `root` (the action "Indexer le dossier"): one connection, one transaction
/// per folder, symbolic links listed but not followed, what `ignore` says left out. Returns the number
/// of files recorded (the folders are counted in `progress.entries` with them), `None` if `progress.cancel` was set.
pub fn index_tree(db_path: Option<&Path>, root: &Path, ignore: &crate::indexignore::IgnoreRules, progress: &IndexProgress) -> Option<usize> {
    use std::sync::atomic::Ordering::Relaxed;
    let mut db = open_index(db_path)?;
    let mut todo = vec![root.to_path_buf()];
    let (mut total, mut files) = (0, 0);
    progress.folders_known.store(1, Relaxed);
    while let Some(folder) = todo.pop() {
        if progress.cancel.load(Relaxed) {
            return None;
        }
        *progress.current.lock().unwrap_or_else(|e| e.into_inner()) = folder.to_string_lossy().into_owned();
        if let Some((n, subfolders)) = record_folder(&mut db, &folder, Some(ignore)) {
            total += n;
            // the entries are files and sub-folders (the ones that are not left out)
            files += n - subfolders.len();
            progress.entries.store(total, Relaxed);
            progress.folders_known.fetch_add(subfolders.len(), Relaxed);
            todo.extend(subfolders);
        }
        progress.folders_done.fetch_add(1, Relaxed);
    }
    Some(files)
}

fn record_folder(db: &mut Connection, folder: &Path, ignore: Option<&crate::indexignore::IgnoreRules>) -> Option<(usize, Vec<PathBuf>)> {
    let started = now();
    let parent = key(folder);
    let mut rows = Vec::new();
    let mut subfolders = Vec::new();
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        let Ok(info) = std::fs::symlink_metadata(entry.path()) else { continue };
        let is_dir = info.is_dir();
        // a walk leaves out what the rules say: nothing is recorded of it, and a folder is not entered
        if let Some(rules) = ignore {
            let name = entry.file_name().to_string_lossy().into_owned();
            if rules.ignores(&entry.path(), &name, is_dir) {
                pyegui_core::dlog!("sqlite", "files: ignored {}", entry.path().display());
                continue;
            }
        }
        if is_dir {
            subfolders.push(entry.path());
        }
        rows.push((key(&entry.path()), entry.file_name().to_string_lossy().into_owned(), is_dir, if is_dir { 0 } else { info.len() as i64 }, secs(info.modified())));
    }
    let tx = db.transaction().ok()?;
    // what was recorded of the folder before this visit, to log what is new and what changed
    let before: Option<std::collections::HashMap<String, (i64, f64)>> = pyegui_core::debuglog::enabled().then(|| {
        let mut statement = tx.prepare("SELECT path, COALESCE(size, 0), COALESCE(modified, 0) FROM files WHERE parent = ? AND path != ?").ok()?;
        let all = statement.query_map(params![parent, parent], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)?, r.get::<_, f64>(2)?)))).ok()?;
        Some(all.flatten().collect())
    }).flatten();
    let folder_modified = secs(std::fs::metadata(folder).and_then(|m| m.modified()));
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| folder.to_string_lossy().into_owned());
    let up = folder.parent().map(key).unwrap_or_else(|| parent.clone());
    tx.execute(
        "INSERT INTO files (path, name, parent, is_dir, size, modified, last_full_scan) VALUES (?, ?, ?, 1, 0, ?, ?)
         ON CONFLICT(path) DO UPDATE SET name = excluded.name, parent = excluded.parent, is_dir = 1,
             modified = excluded.modified, last_full_scan = excluded.last_full_scan",
        params![parent, name, up, folder_modified, started],
    )
    .ok()?;
    for (path, name, is_dir, size, modified) in &rows {
        tx.execute(
            "INSERT INTO files (path, name, parent, is_dir, size, modified, last_seen) VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(path) DO UPDATE SET name = excluded.name, parent = excluded.parent, is_dir = excluded.is_dir,
                 size = excluded.size, modified = excluded.modified, last_seen = excluded.last_seen",
            params![path, name, parent, is_dir, size, modified, if *is_dir { None } else { Some(started) }],
        )
        .ok()?;
    }
    let found: HashSet<&String> = rows.iter().map(|r| &r.0).collect();
    let known: Vec<(String, bool)> = {
        let mut statement = tx.prepare("SELECT path, COALESCE(is_dir, 0) FROM files WHERE parent = ? AND path != ?").ok()?;
        let all = statement.query_map(params![parent, parent], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0))).ok()?;
        all.flatten().collect()
    };
    let (mut added, mut updated, mut removed) = (0usize, 0usize, 0usize);
    if let Some(before) = &before {
        const SHOWN: usize = 15;
        for (path, name, is_dir, size, modified) in &rows {
            let kind = if *is_dir { "folder" } else { "file" };
            match before.get(path) {
                None => {
                    added += 1;
                    if added <= SHOWN {
                        pyegui_core::dlog!("sqlite", "files: + {kind} {path}");
                    }
                }
                Some((s, m)) if s != size || (m - modified).abs() > 1e-3 => {
                    updated += 1;
                    if updated <= SHOWN {
                        pyegui_core::dlog!("sqlite", "files: ~ {kind} {name} changed ({s} -> {size} bytes) in {parent}");
                    }
                }
                Some(_) => {}
            }
        }
        if added > SHOWN || updated > SHOWN {
            pyegui_core::dlog!("sqlite", "files: ... only the first {SHOWN} added and the first {SHOWN} updated are listed");
        }
    }
    for (path, is_dir) in known {
        if found.contains(&path) {
            continue;
        }
        removed += 1;
        pyegui_core::dlog!("sqlite", "files: - {} {path} is gone", if is_dir { "folder" } else { "file" });
        let _ = tx.execute("DELETE FROM files WHERE path = ?", params![path]);
        if is_dir {
            // a vanished folder takes what was recorded below it
            let escaped = path.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            let _ = tx.execute("DELETE FROM files WHERE path LIKE ? ESCAPE '\\'", params![format!("{escaped}{}%", std::path::MAIN_SEPARATOR)]);
        }
    }
    tx.commit().ok()?;
    pyegui_core::dlog!(
        "sqlite",
        "folder {parent} recorded: {} entries, {added} new, {updated} changed, {removed} removed",
        rows.len()
    );
    #[cfg(feature = "pdf")]
    read_pdfs(db, &rows);
    Some((rows.len(), subfolders))
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
        // a database without the table of the favorites (new, or older) starts with the usual ones
        let had_favorites = db
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'favorites'", [], |r| r.get::<_, i64>(0))
            .map(|n| n > 0)
            .unwrap_or(true);
        db.execute_batch(SCHEMA)?;
        add_index_columns(&db);
        let store = TagStore { db, version: 0 };
        if !had_favorites && path.is_none_or(|p| p.as_os_str() != ":memory:") {
            for (label, icon, path) in default_favorites() {
                let position = store.next_favorite_position();
                let _ = store.db.execute("INSERT OR IGNORE INTO favorites (path, label, icon, position) VALUES (?, ?, ?, ?)", params![path, label, icon, position]);
            }
        }
        Ok(store)
    }

    // the favorites

    fn next_favorite_position(&self) -> i64 {
        self.db.query_row("SELECT COALESCE(MAX(position) + 1, 0) FROM favorites", [], |r| r.get(0)).unwrap_or(0)
    }

    /// The favorites in the order of the sidebar
    pub fn favorites(&self) -> Vec<Favorite> {
        let Ok(mut statement) = self.db.prepare("SELECT id, path, label, icon FROM favorites ORDER BY position, id") else { return vec![] };
        statement
            .query_map([], |r| Ok(Favorite { id: r.get(0)?, path: r.get(1)?, label: r.get(2)?, icon: r.get(3)? }))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    /// A folder or a file added last, named after it; None if it already is a favorite
    pub fn add_favorite(&mut self, path: &Path) -> Option<i64> {
        let text = path.to_string_lossy().into_owned();
        let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| text.clone());
        let position = self.next_favorite_position();
        let icon = if path.is_dir() { "folder" } else { "document" };
        let added = self.db.execute("INSERT OR IGNORE INTO favorites (path, label, icon, position) VALUES (?, ?, ?, ?)", params![text, label, icon, position]).ok()?;
        if added == 0 {
            return None;
        }
        pyegui_core::dlog!("sqlite", "favorites: {text} added");
        self.changed();
        Some(self.db.last_insert_rowid())
    }

    pub fn remove_favorite(&mut self, id: i64) {
        let _ = self.db.execute("DELETE FROM favorites WHERE id = ?", params![id]);
        pyegui_core::dlog!("sqlite", "favorites: #{id} removed");
        self.changed();
    }

    /// Puts the favorites in this order; the ones that are not named follow, in their order
    pub fn set_favorite_order(&mut self, ids: &[i64]) {
        let mut order: Vec<i64> = ids.to_vec();
        for favorite in self.favorites() {
            if !order.contains(&favorite.id) {
                order.push(favorite.id);
            }
        }
        for (position, id) in order.iter().enumerate() {
            let _ = self.db.execute("UPDATE favorites SET position = ? WHERE id = ?", params![position as i64, id]);
        }
        self.changed();
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
        pyegui_core::dlog!("sqlite", "tags: tag {name:?} created (color {color}, parent {parent:?})");
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

    /// Puts a tag under a parent (None: at the root), before one of its children (None: last). The siblings are
    /// numbered again. False if that would make a loop
    pub fn move_tag_at(&mut self, tag: i64, parent: Option<i64>, before: Option<i64>) -> bool {
        if parent.is_some_and(|p| self.subtree(tag).contains(&p)) || before == Some(tag) {
            return false;
        }
        let mut siblings: Vec<i64> = match self.db.prepare("SELECT id FROM tags WHERE parent_id IS ? AND id != ? ORDER BY position, id") {
            Ok(mut statement) => statement.query_map(params![parent, tag], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default(),
            Err(_) => return false,
        };
        let at = before.and_then(|b| siblings.iter().position(|id| *id == b)).unwrap_or(siblings.len());
        siblings.insert(at, tag);
        let _ = self.db.execute("UPDATE tags SET parent_id = ? WHERE id = ?", params![parent, tag]);
        for (position, id) in siblings.iter().enumerate() {
            let _ = self.db.execute("UPDATE tags SET position = ? WHERE id = ?", params![position as i64, id]);
        }
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
        pyegui_core::dlog!("sqlite", "items: tag {tag} put on {}", key(path));
        self.changed();
    }

    pub fn untag(&mut self, path: &Path, tag: i64) {
        let _ = self.db.execute("DELETE FROM items WHERE path = ? AND tag_id = ?", params![key(path), tag]);
        pyegui_core::dlog!("sqlite", "items: tag {tag} removed from {}", key(path));
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
        pyegui_core::dlog!("sqlite", "files: {} opened", key(path));
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
    fn a_tag_moves_under_another_or_among_its_siblings_but_never_under_itself() {
        let mut s = store();
        let a = s.create("A", "#111111", None).unwrap();
        let b = s.create("B", "#222222", None).unwrap();
        let c = s.create("C", "#333333", None).unwrap();
        let a1 = s.create("A1", "#444444", Some(a)).unwrap();
        let names = |s: &TagStore| s.all().iter().map(|t| (t.name.clone(), t.depth)).collect::<Vec<_>>();
        // C before A
        assert!(s.move_tag_at(c, None, Some(a)));
        assert_eq!(names(&s), [("C".into(), 0), ("A".into(), 0), ("A1".into(), 1), ("B".into(), 0)]);
        // B under A, before A1
        assert!(s.move_tag_at(b, Some(a), Some(a1)));
        assert_eq!(names(&s), [("C".into(), 0), ("A".into(), 0), ("B".into(), 1), ("A1".into(), 1)]);
        // A under its own child: refused
        assert!(!s.move_tag_at(a, Some(a1), None));
        // A1 to the root, last
        assert!(s.move_tag_at(a1, None, None));
        assert_eq!(names(&s).last(), Some(&("A1".to_string(), 0)));
    }

    #[test]
    fn favorites_are_added_once_reordered_and_removed() {
        let mut s = store();
        assert!(s.favorites().is_empty()); // the in-memory base is not seeded
        let a = s.add_favorite(Path::new("/tmp/a")).unwrap();
        let b = s.add_favorite(Path::new("/tmp/b")).unwrap();
        let c = s.add_favorite(Path::new("/tmp/c")).unwrap();
        assert!(s.add_favorite(Path::new("/tmp/a")).is_none());
        assert_eq!(s.favorites().iter().map(|f| f.label.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        s.set_favorite_order(&[c, a]);
        assert_eq!(s.favorites().iter().map(|f| f.id).collect::<Vec<_>>(), [c, a, b]);
        s.remove_favorite(a);
        assert_eq!(s.favorites().iter().map(|f| f.id).collect::<Vec<_>>(), [c, b]);
    }

    #[test]
    fn a_new_database_starts_with_the_usual_favorites_and_keeps_their_removal() {
        let dir = std::env::temp_dir().join(format!("finder-fav-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("tags.db");
        let first = {
            let mut s = TagStore::open(Some(&path)).unwrap();
            let all = s.favorites();
            assert!(!all.is_empty());
            s.remove_favorite(all[0].id);
            all.len()
        };
        // opened again: not filled again
        let s = TagStore::open(Some(&path)).unwrap();
        assert_eq!(s.favorites().len(), first - 1);
        let _ = std::fs::remove_dir_all(&dir);
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

#[cfg(test)]
mod index_children_tests {
    use super::*;

    #[test]
    fn records_and_drops_direct_entries() {
        let dir = std::env::temp_dir().join(format!("idxkids{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("x.txt"), "hi").unwrap();
        std::fs::write(dir.join("y.txt"), "").unwrap();
        let db = std::env::temp_dir().join(format!("idxkids{}.db", std::process::id()));
        TagStore::open(Some(&db)).unwrap();
        assert_eq!(index_children(Some(&db), &dir), Some(3));
        #[cfg(feature = "pdf")]
        {
            std::fs::write(dir.join("bad.pdf"), "not a pdf").unwrap();
            index_children(Some(&db), &dir);
            let c = Connection::open(&db).unwrap();
            let n: i64 = c.query_row("SELECT COUNT(*) FROM pdf_info WHERE path = ?", params![key(&dir.join("bad.pdf"))], |r| r.get(0)).unwrap();
            assert_eq!(n, 1, "a PDF is read into pdf_info (here with an error)");
            std::fs::remove_file(dir.join("bad.pdf")).unwrap();
            index_children(Some(&db), &dir);
        }
        std::fs::remove_file(dir.join("y.txt")).unwrap();
        std::fs::remove_dir(dir.join("sub")).unwrap();
        assert_eq!(index_children(Some(&db), &dir), Some(1));
        let c = Connection::open(&db).unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM files WHERE parent = ?", params![key(&dir)], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_walk_leaves_out_what_the_rules_ignore() {
        let dir = std::env::temp_dir().join(format!("idxignore{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["keep/deep", ".git/objects", "node_modules/pkg", "Local Settings/cache"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
            std::fs::write(dir.join(sub).join("f.txt"), "x").unwrap();
        }
        let db = std::env::temp_dir().join(format!("idxignore{}.db", std::process::id()));
        TagStore::open(Some(&db)).unwrap();
        let rules = crate::indexignore::IgnoreRules { hidden: true, names: vec!["node_modules".into(), "Local Settings".into()] };
        let progress = IndexProgress::default();
        let total = index_tree(Some(&db), &dir, &rules, &progress);
        assert_eq!(progress.folders_done.load(std::sync::atomic::Ordering::Relaxed), 3, "the root, keep and keep/deep");
        assert!((progress.fraction() - 1.0).abs() < 1e-6);
        assert_eq!(total, Some(1), "keep/deep/f.txt is the only file");
        assert_eq!(progress.entries.load(std::sync::atomic::Ordering::Relaxed), 3, "keep, keep/deep and keep/deep/f.txt");
        let c = Connection::open(&db).unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM files WHERE path LIKE ?", params![format!("%{}%", ".git")], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
