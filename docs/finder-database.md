# The Finder database (`tags.db`)

The Python Finder (`examples/finder.py`, class `TagStore`) and the Rust Finder (`finder-rs`,
`finder-rs/src/tagstore.rs`) keep what they know in **one SQLite file** and share it: a tag made in
one is seen by the other. The schema is declared in both (`CREATE TABLE IF NOT EXISTS`), so
whichever starts first creates it. Keep the two declarations identical when you change one.

- **Where:** `tags.db` in the user data folder of the system (macOS `~/Library/Application Support/Finder/`,
  Windows `%APPDATA%\Finder\`, Linux `$XDG_DATA_HOME/Finder/`). `PYEGUI_FINDER_TAGS` overrides the path.
- **Journal:** write-ahead log (`PRAGMA journal_mode = WAL`), so the window reads while a thread writes.
  `-wal` and `-shm` files next to the database are normal.
- **Foreign keys:** `PRAGMA foreign_keys = ON` (deleting a tag deletes its `items`).
- **Paths:** stored as absolute paths, lower case on Windows (`tag_key` in Python, `key` in Rust). Always
  compare with that form.
- **Read it:** the **Base SQL** pane of both apps (menu of the panes, or the action palette in `finder.py`)
  shows the tables and runs a query on the base opened **read-only**.

## Tables

### `tags`

| Column | Type | Meaning |
|---|---|---|
| `id` | INTEGER PRIMARY KEY | |
| `name` | TEXT NOT NULL | name of the tag (a level of the tree) |
| `color` | TEXT NOT NULL | `#rrggbb` |
| `parent_id` | INTEGER, FK `tags(id)` ON DELETE SET NULL | parent tag: the tags form a tree (`Work/Invoice` is the tag `Invoice` under `Work`) |
| `position` | INTEGER NOT NULL DEFAULT 0 | order among the tags |

### `items`

The tags put on files and folders (many to many).

| Column | Type | Meaning |
|---|---|---|
| `path` | TEXT NOT NULL | the file or folder |
| `tag_id` | INTEGER NOT NULL, FK `tags(id)` ON DELETE CASCADE | |

Primary key `(path, tag_id)`; index `items_tag(tag_id)`.

Only the **own** tags are stored. Two rules of inheritance are computed when asked, never copied:

- an item has the tags of the folders that contain it (`own` → `folder`);
- an item with a tag also has all the parent tags of that tag (`parent`).

### `files`

Every path the app knows: opened by the user, and/or indexed. One row per path.

| Column | Type | Filled by | Meaning |
|---|---|---|---|
| `path` | TEXT PRIMARY KEY | | |
| `last_opened` | REAL | opening | seconds since the epoch; index `files_last_opened` |
| `open_count` | INTEGER NOT NULL DEFAULT 0 | opening | |
| `name` | TEXT | index | last component of the path |
| `parent` | TEXT | index | `path` of the folder that holds it |
| `is_dir` | INTEGER | index | 1 for a folder |
| `size` | INTEGER | index | bytes (0 for a folder) |
| `modified` | REAL | index | modification date, seconds since the epoch |
| `last_seen` | REAL | index | files: the time of the walk or visit that found it |
| `last_full_scan` | REAL | index | folders: the time all its entries were last read |

The index columns (`name` … `last_full_scan`) are added with `ALTER TABLE` to a base that was made
without them, by whichever app opens it. A row may only have `last_opened` / `open_count` (opened, never indexed), or only the
index columns (indexed, never opened).

**How it is filled**

1. **Navigation.** Showing a folder records the folder and its direct entries (no walk below) in a thread
   (`TagStore.index_children`, `tagstore::index_children`). The entries that vanished from the folder are deleted
   (and, for a folder, what was recorded under it). If the base is locked by a long walk, the visit is skipped and
   tried again at the next one.
2. **Indexation.** The action "Indexer le dossier" walks the selected folder (or the current one) and everything
   below it, symbolic links listed but not followed. `finder.py` also drops what a walk did not meet again and can
   run several readers; `finder-rs` records without dropping.
3. **PDF.** Each time a folder is recorded, the PDF in it that are new or changed are read (see `pdf_info`).

### `pdf_info`

What a PDF says about itself, read with `pyegui.pdf_info` (Python) or `pyegui_core::pdf::info_of_path` (Rust).
A PDF is read again only if its `size` or `mtime` is no longer the one recorded.

| Column | Type | Meaning |
|---|---|---|
| `path` | TEXT PRIMARY KEY | |
| `size`, `mtime` | INTEGER, REAL | of the file when it was read |
| `pages` | INTEGER | **page count** (NULL if unknown) |
| `encrypted`, `needs_password` | INTEGER | 0 or 1; NULL when the file could not be read |
| `can_print`, `can_copy`, `can_modify`, `can_annotate`, `can_fill_forms`, `can_assemble` | INTEGER | rights of the PDF: 1 allowed, 0 forbidden, NULL unknown |
| `title`, `author`, `subject`, `keywords`, `creator`, `producer` | TEXT | document information |
| `created`, `modified` | REAL | dates, seconds since the epoch |
| `pdf_version` | TEXT | e.g. `1.7` |
| `error` | TEXT | why the file could not be read (the other columns are then mostly NULL) |
| `read_at` | REAL | when it was read |

### `favorites`

The favorites of the sidebar. A new base starts with the home folder and its usual folders.

| Column | Type | Meaning |
|---|---|---|
| `id` | INTEGER PRIMARY KEY | |
| `path` | TEXT NOT NULL UNIQUE | |
| `label` | TEXT NOT NULL | name shown |
| `icon` | TEXT NOT NULL DEFAULT `'folder'` | name of an icon |
| `position` | INTEGER NOT NULL DEFAULT 0 | order in the sidebar |

### `notes`

A free text attached to a file or folder.

| Column | Type | Meaning |
|---|---|---|
| `path` | TEXT PRIMARY KEY | |
| `text` | TEXT NOT NULL | |
| `updated` | REAL NOT NULL | seconds since the epoch |

### `meta_defs`

The definitions of the meta properties (see `core/src/meta.rs`).

| Column | Type | Meaning |
|---|---|---|
| `id` | INTEGER PRIMARY KEY | |
| `name` | TEXT NOT NULL UNIQUE | name of the property |
| `type` | TEXT NOT NULL | `text`, `longtext`, `number`, `bool`, `date` or `choice` (a name, so a type can be added with no migration) |
| `description` | TEXT NOT NULL DEFAULT `''` | |
| `config` | TEXT NOT NULL DEFAULT `'{}'` | JSON: for a `choice` `mode` (`closed` or `open`), `multiple`, `options` (`id`, `label`, `color`), `next_id`; for a `number` `unit`, `decimals` |
| `position` | INTEGER NOT NULL DEFAULT 0 | order of the properties |

### `meta_values`

The value of a property on a file or folder. Only own values are stored.

| Column | Type | Meaning |
|---|---|---|
| `path` | TEXT NOT NULL | the file or folder |
| `def_id` | INTEGER NOT NULL, FK `meta_defs(id)` ON DELETE CASCADE | |
| `value` | TEXT NOT NULL | `text`: as typed; `number`: decimal; `bool`: `1`/`0`; `date`: seconds since 1970 (midnight UTC); `choice`: the **id** of an option (`3`), or a JSON list of ids (`[3,5]`) when `multiple` |
| `source` | TEXT NOT NULL DEFAULT `'user'` | `user` (typed), later `auto` (filled by a rule of the indexation) |

Primary key `(path, def_id)`; index `meta_values_def(def_id)`. A choice keeps the id of its options, never the label:
renaming or merging an option does not touch the values.

## Queries to start from

```sql
-- the PDF with the most pages
SELECT f.path, p.pages FROM pdf_info p JOIN files f ON f.path = p.path ORDER BY p.pages DESC LIMIT 20;

-- PDF that could not be read
SELECT path, error FROM pdf_info WHERE error IS NOT NULL;

-- what a folder holds, as known by the index
SELECT name, is_dir, size, datetime(modified, 'unixepoch') FROM files WHERE parent = '/Users/me/Documents' ORDER BY name;

-- the files of one tag (own tags only: inheritance is computed by the apps)
SELECT i.path FROM items i JOIN tags t ON t.id = i.tag_id WHERE t.name = 'Invoice';

-- recently opened
SELECT path, datetime(last_opened, 'unixepoch') FROM files WHERE last_opened IS NOT NULL ORDER BY last_opened DESC LIMIT 20;
```

## Changing the schema

1. Edit the schema in **both** `examples/finder.py` (`SCHEMA`, and `INDEX_COLUMNS` for a column added to `files`) and
   `finder-rs/src/tagstore.rs` (`SCHEMA`, `INDEX_COLUMNS`).
2. A column added to an existing table needs an `ALTER TABLE ... ADD COLUMN` at opening in both (see
   `add_index_columns` in Rust, the loop over `INDEX_COLUMNS` in `TagStore.__init__`), because bases already exist.
3. Update this page.
