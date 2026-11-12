"""The tags of the Finder example, and the files it knows, kept in a SQLite database.

A tag has a name, a color and an optional parent tag, so the tags form a tree. A tag is put on
files and folders. Two rules of inheritance, both computed when asked, never copied in the base:

* an item has the tags of the folders that contain it (every file of a tagged folder has its tag);
* an item with a tag also has all the parent tags of that tag (what is "Invoice" is also "Work").

    tags = TagStore()                       # the default database, see default_path()
    work = tags.create("Work", "#3b82f6")
    invoice = tags.create("Invoice", "#ef4444", parent=work)
    tags.tag("/docs/2026", invoice)
    tags.effective("/docs/2026/march.pdf")  # {invoice: "folder", work: "parent"}
    tags.tagged_with(work)                  # ["/docs/2026"]

Any file or folder can also be referenced in the base (table files), to remember when it was last
opened: reference() adds it, opened() stamps it, last_opened() and recently_opened() read it. An item can also have a note,
a free text: set_note(), note() and noted().
"""

import os
import sqlite3
import sys
import time
from pathlib import Path

HOW = ("own", "folder", "parent")  # where an effective tag comes from, strongest first

SCHEMA = """
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
"""


def default_path():
    """tags.db in the folder of the user data of the OS; PYEGUI_FINDER_TAGS overrides it"""
    if os.environ.get("PYEGUI_FINDER_TAGS"):
        return Path(os.environ["PYEGUI_FINDER_TAGS"])
    if sys.platform == "darwin":
        base = Path.home() / "Library" / "Application Support"
    elif sys.platform == "win32":
        base = Path(os.environ.get("APPDATA", Path.home() / "AppData" / "Roaming"))
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share"))
    return base / "Finder" / "tags.db"


def key(path):
    """how a path is stored: absolute, and in the case of the OS for the comparison (Windows)"""
    return os.path.normcase(os.path.abspath(str(path)))


class TagStore:
    def __init__(self, path=None):
        self.path = ":memory:" if path == ":memory:" else Path(path or default_path())
        if self.path != ":memory:":
            self.path.parent.mkdir(parents=True, exist_ok=True)
        self.db = sqlite3.connect(str(self.path))
        self.db.execute("PRAGMA foreign_keys = ON")
        self.db.executescript(SCHEMA)
        self.db.commit()
        self.version = 0  # grows at every change, so a view can tell its cache is stale

    def changed(self):
        self.db.commit()
        self.version += 1

    # the tags

    def create(self, name, color, parent=None):
        """a new tag, last among its siblings; returns its id"""
        last = self.db.execute(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM tags WHERE parent_id IS ?", (parent,)
        ).fetchone()[0]
        cursor = self.db.execute(
            "INSERT INTO tags (name, color, parent_id, position) VALUES (?, ?, ?, ?)",
            (name, color, parent, last),
        )
        self.changed()
        return cursor.lastrowid

    def rename(self, tag, name):
        self.db.execute("UPDATE tags SET name = ? WHERE id = ?", (name, tag))
        self.changed()

    def recolor(self, tag, color):
        self.db.execute("UPDATE tags SET color = ? WHERE id = ?", (color, tag))
        self.changed()

    def move(self, tag, parent):
        """puts a tag under another one (None: at the root). False if that would make a loop"""
        if parent is not None and parent in self.subtree(tag):
            return False
        last = self.db.execute(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM tags WHERE parent_id IS ?", (parent,)
        ).fetchone()[0]
        self.db.execute(
            "UPDATE tags SET parent_id = ?, position = ? WHERE id = ?", (parent, last, tag)
        )
        self.changed()
        return True

    def delete(self, tag):
        """removes a tag and what was tagged with it; its children go up to its parent"""
        parent = self.db.execute("SELECT parent_id FROM tags WHERE id = ?", (tag,)).fetchone()
        self.db.execute("UPDATE tags SET parent_id = ? WHERE parent_id = ?", (parent and parent[0], tag))
        self.db.execute("DELETE FROM tags WHERE id = ?", (tag,))
        self.changed()

    def all(self):
        """every tag as a dict (id, name, color, parent, depth), in the order of the tree"""
        rows = self.db.execute(
            "SELECT id, name, color, parent_id FROM tags ORDER BY position, id"
        ).fetchall()
        children = {}
        for id_, name, color, parent in rows:
            children.setdefault(parent, []).append(
                {"id": id_, "name": name, "color": color, "parent": parent}
            )
        ordered = []

        def walk(parent, depth):
            for tag in children.get(parent, []):
                ordered.append({**tag, "depth": depth})
                walk(tag["id"], depth + 1)

        walk(None, 0)
        return ordered

    def get(self, tag):
        row = self.db.execute("SELECT id, name, color, parent_id FROM tags WHERE id = ?", (tag,)).fetchone()
        return row and {"id": row[0], "name": row[1], "color": row[2], "parent": row[3]}

    def subtree(self, tag):
        """a tag and all the tags under it"""
        rows = self.db.execute(
            """WITH RECURSIVE sub(id) AS (
                   SELECT ? UNION SELECT tags.id FROM tags JOIN sub ON tags.parent_id = sub.id)
               SELECT id FROM sub""",
            (tag,),
        ).fetchall()
        return {row[0] for row in rows}

    def lineage(self, tag):
        """the parent tags of a tag, from its parent up to the root"""
        rows = self.db.execute(
            """WITH RECURSIVE up(id, parent_id, level) AS (
                   SELECT id, parent_id, 0 FROM tags WHERE id = ?
                   UNION SELECT tags.id, tags.parent_id, level + 1 FROM tags JOIN up ON tags.id = up.parent_id)
               SELECT id FROM up WHERE level > 0 ORDER BY level""",
            (tag,),
        ).fetchall()
        return [row[0] for row in rows]

    # the items

    def tag(self, path, tag):
        self.db.execute("INSERT OR IGNORE INTO items (path, tag_id) VALUES (?, ?)", (key(path), tag))
        self.changed()

    def untag(self, path, tag):
        self.db.execute("DELETE FROM items WHERE path = ? AND tag_id = ?", (key(path), tag))
        self.changed()

    def own(self, path):
        """the ids of the tags put on this very item"""
        rows = self.db.execute("SELECT tag_id FROM items WHERE path = ?", (key(path),)).fetchall()
        return {row[0] for row in rows}

    def effective(self, path):
        """every tag of an item, as {tag id: where it comes from}: "own" (put on the item),
        "folder" (put on a folder that contains it) or "parent" (the parent of another tag)"""
        path = Path(path)
        folders = [key(p) for p in path.parents]
        marks = ",".join("?" * len(folders))
        found = {}
        for tag in self.own(path):
            found[tag] = "own"
        if folders:
            rows = self.db.execute(f"SELECT DISTINCT tag_id FROM items WHERE path IN ({marks})", folders)
            for (tag,) in rows:
                found.setdefault(tag, "folder")
        for tag in list(found):
            for parent in self.lineage(tag):
                found.setdefault(parent, "parent")
        return found

    def tagged_with(self, tag):
        """the paths that have a tag, or a tag under it, put on them: the roots of what the tag
        covers. The files inside a tagged folder are not listed, they inherit it"""
        marks = ",".join("?" * len(self.subtree(tag)))
        rows = self.db.execute(
            f"SELECT DISTINCT path FROM items WHERE tag_id IN ({marks}) ORDER BY path",
            list(self.subtree(tag)),
        ).fetchall()
        return [row[0] for row in rows]

    # the files: any file or folder can be referenced, to remember when it was last opened

    def reference(self, path):
        """puts a file or a folder in the base, without opening it"""
        self.db.execute("INSERT OR IGNORE INTO files (path) VALUES (?)", (key(path),))
        self.changed()

    def opened(self, path, when=None):
        """the item was opened now (or at `when`, seconds since the epoch): referenced if it was not"""
        self.db.execute(
            """INSERT INTO files (path, last_opened, open_count) VALUES (?, ?, 1)
               ON CONFLICT(path) DO UPDATE SET last_opened = excluded.last_opened,
                                               open_count = open_count + 1""",
            (key(path), time.time() if when is None else when),
        )
        self.changed()

    def last_opened(self, path):
        """when the item was last opened, in seconds since the epoch, or None if never"""
        row = self.db.execute("SELECT last_opened FROM files WHERE path = ?", (key(path),)).fetchone()
        return row[0] if row else None

    def recently_opened(self, limit=20):
        """the paths last opened, the latest first, as [(path, seconds since the epoch)]"""
        return self.db.execute(
            "SELECT path, last_opened FROM files WHERE last_opened IS NOT NULL "
            "ORDER BY last_opened DESC LIMIT ?",
            (limit,),
        ).fetchall()

    # the notes

    def note(self, path):
        """the note of an item, or "" if it has none"""
        row = self.db.execute("SELECT text FROM notes WHERE path = ?", (key(path),)).fetchone()
        return row[0] if row else ""

    def set_note(self, path, text):
        """writes the note of an item; an empty text removes it"""
        if text.strip():
            self.db.execute(
                """INSERT INTO notes (path, text, updated) VALUES (?, ?, ?)
                   ON CONFLICT(path) DO UPDATE SET text = excluded.text, updated = excluded.updated""",
                (key(path), text, time.time()),
            )
        else:
            self.db.execute("DELETE FROM notes WHERE path = ?", (key(path),))
        self.changed()

    def noted(self):
        """the stored paths (see key) that have a note, to show an icon without a query per row"""
        return {row[0] for row in self.db.execute("SELECT path FROM notes")}

    def forget(self, path):
        """takes an item out of the base, with what it remembers (its tags and note too)"""
        self.db.execute("DELETE FROM files WHERE path = ?", (key(path),))
        self.db.execute("DELETE FROM items WHERE path = ?", (key(path),))
        self.db.execute("DELETE FROM notes WHERE path = ?", (key(path),))
        self.changed()

    def moved(self, old, new):
        """an item was renamed or moved: what the base knows of it, and of what it contains, follows"""
        old, new = key(old), key(new)
        escaped = old.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
        inside = escaped.rstrip(os.sep) + ("\\\\" if os.sep == "\\" else os.sep) + "%"
        for table in ("items", "files", "notes"):
            self.db.execute(
                f"UPDATE {table} SET path = ? || substr(path, ?) WHERE path = ? OR path LIKE ? ESCAPE '\\'",
                (new, len(old) + 1, old, inside),
            )
        self.changed()
