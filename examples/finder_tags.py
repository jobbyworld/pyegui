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

A whole folder can be indexed: index_folder() records it and everything it contains (name, parent,
kind, size, date) in the same table, search() finds the indexed items by name.
index_pdfs() then reads the PDF it found with pypdf (pages, protections, metadata) into pdf_info,
only those that are new or changed.
"""

import os
import queue
import sqlite3
import sys
import threading
import time
from pathlib import Path

HOW = ("own", "folder", "parent")  # where an effective tag comes from, strongest first
INDEX_BATCH = 5000  # rows written per transaction by index_folder()
# the columns of files filled by index_folder(), added to the bases created before it
INDEX_COLUMNS = {
    "name": "TEXT",
    "parent": "TEXT",
    "is_dir": "INTEGER",
    "size": "INTEGER",
    "modified": "REAL",
    "last_seen": "REAL",  # files: when a walk last found it
    "last_full_scan": "REAL",  # folders: when a walk last read all its entries
}

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
CREATE TABLE IF NOT EXISTS pdf_info (
    path TEXT PRIMARY KEY,
    size INTEGER, mtime REAL,  -- of the file when it was read: another one means read again
    pages INTEGER, encrypted INTEGER, needs_password INTEGER,
    can_print INTEGER, can_copy INTEGER, can_modify INTEGER,
    can_annotate INTEGER, can_fill_forms INTEGER, can_assemble INTEGER,
    title TEXT, author TEXT, subject TEXT, keywords TEXT, creator TEXT, producer TEXT,
    created REAL, modified REAL, pdf_version TEXT, error TEXT, read_at REAL
);
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


def like_inside(stored):
    """the LIKE pattern (ESCAPE backslash) of everything inside a stored folder path"""
    escaped = stored.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
    return escaped.rstrip(os.sep) + ("\\\\" if os.sep == "\\" else os.sep) + "%"


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
        # bases made before the index have only the first columns of files
        have = {row[1] for row in self.db.execute("PRAGMA table_info(files)")}
        for column, kind in INDEX_COLUMNS.items():
            if column not in have:
                self.db.execute(f"ALTER TABLE files ADD COLUMN {column} {kind}")
        self.db.executescript(
            "CREATE INDEX IF NOT EXISTS files_parent ON files(parent);"
            "CREATE INDEX IF NOT EXISTS files_name ON files(name);"
        )
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

    # the index: a whole folder and everything it contains, with name, size and date

    def index_folder(self, root, progress=None, cancel=None, workers=1):
        """walks a folder and records every file and sub-folder in the table files (name, parent,
        kind, size, modification date). A file gets last_seen, the time of the walk that found it;
        a folder gets last_full_scan, the time of the walk that read all its entries (a folder
        that could not be read keeps its old value). What was recorded under the folder and no
        longer exists is dropped. Safe to call from a thread: a file base is opened again for the
        walk (call changed() from the GUI thread to refresh the views). Symbolic links are listed
        but not followed. Returns (number of items, number of removed rows), or None if cancelled.

        `workers` threads read the folders while this thread alone writes (SQLite has one writer).
        On a local disk one reader is the fastest (measured on 166 000 items: 5.2 s with 1, 6.5 s
        with 4 or 8, the threads fight for the GIL, the system already caches the listings); more
        readers are for a slow or distant disk (network share, USB drive, cold hard disk), where the
        time goes in waiting for the answers.
        `progress(count)` is called about every thousand items, `cancel()` tells to stop."""
        root = Path(root).resolve()
        if self.path == ":memory:":
            db = self.db
        else:
            db = sqlite3.connect(str(self.path), timeout=30)
            # the index can be rebuilt: no fsync at each commit (a power cut during a walk only
            # costs the walk), which is what made the writes slower than the reading
            db.execute("PRAGMA synchronous = OFF")
        started = time.time()
        count = 0
        rows = []  # entries found, written by batches
        scanned = []  # folders whose entries were all read, not yet written
        seen_folders = {key(root)}
        done = set()  # every folder fully read in this walk
        todo = queue.Queue()  # folders to read
        results = queue.Queue()  # what the readers found, for the writer: (folder, rows, complete)
        stop = threading.Event()

        def read(folder):
            found, subfolders = [], []
            parent = key(folder)  # once per folder, not per item
            try:
                entries = list(os.scandir(folder))
            except OSError:
                return found, subfolders, False  # no right to read it, or it vanished
            for item in entries:
                if stop.is_set():
                    return found, subfolders, False
                try:
                    is_dir = item.is_dir(follow_symlinks=False)
                    info = item.stat(follow_symlinks=False)
                except OSError:
                    continue
                found.append(
                    (os.path.normcase(item.path), item.name, parent, int(is_dir),
                     0 if is_dir else info.st_size, info.st_mtime, None if is_dir else started)
                )
                if is_dir:
                    subfolders.append(item.path)
            return found, subfolders, True

        def reader():
            while (folder := todo.get()) is not None:
                try:
                    if not stop.is_set():
                        found, subfolders, complete = read(folder)
                        # the result first: a folder is always written after the one that holds it
                        results.put((folder, found, complete))
                        for sub in subfolders:
                            todo.put(sub)
                except Exception as error:  # reported by the writer
                    results.put(error)
                finally:
                    todo.task_done()

        def closer():
            todo.join()  # every folder read, the sub-folders being queued before task_done
            results.put(None)

        def flush():
            db.executemany(
                """INSERT INTO files (path, name, parent, is_dir, size, modified, last_seen)
                   VALUES (?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(path) DO UPDATE SET name = excluded.name, parent = excluded.parent,
                       is_dir = excluded.is_dir, size = excluded.size,
                       modified = excluded.modified, last_seen = excluded.last_seen""",
                rows,
            )
            db.executemany(
                "UPDATE files SET last_full_scan = ? WHERE path = ?", [(started, f) for f in scanned]
            )
            db.commit()
            rows.clear()
            scanned.clear()

        threads = [threading.Thread(target=reader, daemon=True) for _ in range(max(1, workers))]
        failure = None
        cancelled = False
        try:
            # the root itself: a folder, so no last_seen
            db.execute(
                """INSERT INTO files (path, name, parent, is_dir, size, modified) VALUES (?, ?, ?, 1, 0, ?)
                   ON CONFLICT(path) DO UPDATE SET name = excluded.name, parent = excluded.parent,
                       is_dir = 1, modified = excluded.modified""",
                (key(root), root.name or str(root), key(root.parent), root.stat().st_mtime),
            )
            for thread in threads:
                thread.start()
            todo.put(root)  # before the closer, whose join() returns at once on an empty queue
            threading.Thread(target=closer, daemon=True).start()
            while (got := results.get()) is not None:
                if isinstance(got, Exception):
                    failure = failure or got
                    stop.set()
                    continue
                folder, found, complete = got
                rows.extend(found)
                count += len(found)
                seen_folders.update(r[0] for r in found if r[3])
                if complete:
                    scanned.append(key(folder))
                    done.add(key(folder))
                if len(rows) >= INDEX_BATCH:
                    flush()
                    if progress:
                        progress(count)
                if not stop.is_set() and cancel and cancel():
                    cancelled = True
                    stop.set()
            for _ in threads:
                todo.put(None)
            if failure:
                raise failure
            if cancelled:
                db.rollback()
                return None
            flush()
            gone = self._drop_vanished(db, root, started, seen_folders, done)
            db.commit()
            if progress:
                progress(count)
            return count, gone
        finally:
            stop.set()
            if db is not self.db:
                db.close()

    @staticmethod
    def _drop_vanished(db, root, started, seen_folders, done):
        """removes the rows under root that the walk did not find again: the folders it did not
        meet (with all they contained), then the files that its fully read folders no longer hold"""
        inside = like_inside(key(root))
        gone = 0
        folders = db.execute(
            "SELECT path FROM files WHERE is_dir = 1 AND path LIKE ? ESCAPE '\\'", (inside,)
        ).fetchall()
        for (folder,) in folders:
            if folder not in seen_folders:
                below = like_inside(folder)
                gone += db.execute(
                    "DELETE FROM files WHERE path = ? OR path LIKE ? ESCAPE '\\'", (folder, below)
                ).rowcount
        stale = db.execute(
            "SELECT path, parent FROM files WHERE is_dir = 0 AND last_seen < ? "
            "AND path LIKE ? ESCAPE '\\'",
            (started, inside),
        ).fetchall()
        for path, parent in stale:
            if parent in done:
                gone += db.execute("DELETE FROM files WHERE path = ?", (path,)).rowcount
        return gone

    def search(self, text, limit=200):
        """the indexed items whose name contains the text, as [(path, is_dir, size, modified)]"""
        escaped = text.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
        return self.db.execute(
            "SELECT path, is_dir, size, modified FROM files WHERE name LIKE ? ESCAPE '\\' "
            "ORDER BY name LIMIT ?",
            (f"%{escaped}%", limit),
        ).fetchall()

    def find(self, name="", ext="", under=None, folders=True, limit=500, pdf=None):
        """the indexed items matching all the given criteria, as
        [(path, is_dir, size, modified, name, pages, encrypted)] (the last two are None for what is
        not a PDF read by index_pdfs).
        name: a text the name contains (without the case); ext: one or more extensions, separated
        by spaces or commas, with or without the dot (a filter on the extension leaves out the
        folders); under: only what is inside this folder, at any depth (None: everywhere);
        folders: False leaves out the folders.
        pdf: criteria on what pypdf read, which keep only the PDF that were read: a dict with any
        of pages_min, pages_max, protected (True: encrypted, False: not) and text (in the title,
        author, subject or keywords). Sorted by name, at most `limit` rows."""

        def contains(text):  # a LIKE pattern, ESCAPE backslash
            return "%" + text.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_") + "%"

        where, args = ["f.name IS NOT NULL"], []
        if name.strip():
            where.append(r"f.name LIKE ? ESCAPE '\'")
            args.append(contains(name.strip()))
        exts = [e.strip().lstrip(".") for e in ext.replace(",", " ").split() if e.strip(". ")]
        if exts:
            where.append("f.is_dir = 0 AND (" + " OR ".join([r"f.name LIKE ? ESCAPE '\'"] * len(exts)) + ")")
            args += ["%." + contains(e)[1:-1] for e in exts]  # ends with .ext
        elif not folders:
            where.append("f.is_dir = 0")
        if under is not None:
            where.append(r"f.path LIKE ? ESCAPE '\'")
            args.append(like_inside(key(under)))
        pdf = pdf or {}
        if pdf.get("pages_min") is not None:
            where.append("p.pages >= ?")
            args.append(pdf["pages_min"])
        if pdf.get("pages_max") is not None:
            where.append("p.pages <= ?")
            args.append(pdf["pages_max"])
        if pdf.get("protected") is not None:
            where.append("p.encrypted = ?")
            args.append(int(pdf["protected"]))
        if (pdf.get("text") or "").strip():
            columns = ("title", "author", "subject", "keywords")
            where.append("(" + " OR ".join(rf"p.{c} LIKE ? ESCAPE '\'" for c in columns) + ")")
            args += [contains(pdf["text"].strip())] * len(columns)
        # own connection outside memory: safe to call from a thread while the window draws
        db = self.db if self.path == ":memory:" else sqlite3.connect(str(self.path), timeout=30)
        try:
            return db.execute(
                "SELECT f.path, f.is_dir, f.size, f.modified, f.name, p.pages, p.encrypted "
                "FROM files f LEFT JOIN pdf_info p ON p.path = f.path "
                f"WHERE {' AND '.join(where)} ORDER BY f.name COLLATE NOCASE, f.path LIMIT ?",
                (*args, limit),
            ).fetchall()
        finally:
            if db is not self.db:
                db.close()

    def index_pdfs(self, root=None, progress=None, cancel=None):
        """reads, with pypdf, the PDF indexed under a folder (None: the whole index) that are new or
        changed since they were last read (their size or date is not the one of pdf_info), into the
        table pdf_info. Without a new walk: it also reads the PDF indexed before pdf_info existed.
        Returns (read, with an error, up to date), or None if cancelled. Run index_folder first.
        Safe to call from a thread, like index_folder. `progress(done, total)` after each file."""
        from finder_pdf import COLUMNS, read_pdf_info  # pypdf is only needed from here

        db = self.db if self.path == ":memory:" else sqlite3.connect(str(self.path), timeout=30)
        inside = "%" if root is None else like_inside(key(root))
        try:
            if db is not self.db:
                db.execute("PRAGMA synchronous = OFF")
            todo = db.execute(
                """SELECT f.path, f.size, f.modified FROM files f LEFT JOIN pdf_info p ON p.path = f.path
                   WHERE f.is_dir = 0 AND f.name LIKE '%.pdf' AND f.path LIKE ? ESCAPE '\\'
                     AND (p.path IS NULL OR p.size IS NOT f.size OR p.mtime IS NOT f.modified)
                   ORDER BY f.path""",
                (inside,),
            ).fetchall()
            total_pdf = db.execute(
                "SELECT COUNT(*) FROM files WHERE is_dir = 0 AND name LIKE '%.pdf' AND path LIKE ? ESCAPE '\\'",
                (inside,),
            ).fetchone()[0]
            errors = read = 0
            marks = ",".join("?" * (len(COLUMNS) + 4))
            for done, (path, size, modified) in enumerate(todo, 1):
                if cancel and cancel():
                    db.commit()
                    return None
                if not os.path.exists(path):  # in the index, but gone since: the next walk drops it
                    continue
                info = read_pdf_info(path)
                read += 1
                errors += bool(info["error"])
                db.execute(
                    f"INSERT OR REPLACE INTO pdf_info (path, size, mtime, read_at, {', '.join(COLUMNS)}) "
                    f"VALUES ({marks})",
                    (path, size, modified, time.time(), *(info[c] for c in COLUMNS)),
                )
                if done % 50 == 0:
                    db.commit()
                if progress:
                    progress(done, len(todo))
            # what is no longer in the index (deleted, or moved out of it) is forgotten
            db.execute("DELETE FROM pdf_info WHERE path NOT IN (SELECT path FROM files)")
            db.commit()
            return read, errors, total_pdf - len(todo)
        finally:
            if db is not self.db:
                db.close()

    def pdf_info(self, path):
        """what pypdf read of a PDF at the last indexation, as a dict, or None if it was not read"""
        cursor = self.db.execute("SELECT * FROM pdf_info WHERE path = ?", (key(path),))
        row = cursor.fetchone()
        return dict(zip((c[0] for c in cursor.description), row)) if row else None

    def pdf_summary(self, paths):
        """for many PDF at once: {path: (pages, protected)} of those read at an indexation;
        protected = asks a password, or forbids a right (print, copy, modify...)"""
        wanted = {key(p): p for p in paths}
        found = {}
        keys = list(wanted)
        for start in range(0, len(keys), 500):  # SQLite limits the number of parameters
            chunk = keys[start : start + 500]
            marks = ",".join("?" * len(chunk))
            rows = self.db.execute(
                "SELECT path, pages, needs_password, can_print, can_copy, can_modify, can_annotate,"
                f" can_fill_forms, can_assemble FROM pdf_info WHERE path IN ({marks})",
                chunk,
            )
            for path, pages, *rights in rows:
                found[wanted[path]] = (pages, rights[0] == 1 or any(r == 0 for r in rights[1:]))
        return found

    def indexed_count(self):
        """how many items the index holds"""
        return self.db.execute("SELECT COUNT(*) FROM files WHERE name IS NOT NULL").fetchone()[0]

    def forget(self, path):
        """takes an item out of the base, with what it remembers (its tags and note too)"""
        self.db.execute("DELETE FROM files WHERE path = ?", (key(path),))
        self.db.execute("DELETE FROM pdf_info WHERE path = ?", (key(path),))
        self.db.execute("DELETE FROM items WHERE path = ?", (key(path),))
        self.db.execute("DELETE FROM notes WHERE path = ?", (key(path),))
        self.changed()

    def moved(self, old, new):
        """an item was renamed or moved: what the base knows of it, and of what it contains, follows"""
        old, new = key(old), key(new)
        escaped = old.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
        inside = escaped.rstrip(os.sep) + ("\\\\" if os.sep == "\\" else os.sep) + "%"
        for table in ("items", "files", "notes", "pdf_info"):
            self.db.execute(
                f"UPDATE {table} SET path = ? || substr(path, ?) WHERE path = ? OR path LIKE ? ESCAPE '\\'",
                (new, len(old) + 1, old, inside),
            )
        self.changed()
