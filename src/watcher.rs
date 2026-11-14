//! FileWatcher: the folders a file view shows are listed once and watched by the system
//! (FSEvents on macOS, ReadDirectoryChangesW on Windows, inotify on Linux). A change made by any
//! program invalidates the listing and asks egui for a new frame, so the views follow it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// A file or folder of a listing
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub hidden: bool,
}

/// What the thread of the system watcher writes: the paths that changed and when
#[derive(Default)]
struct Changes {
    /// increases at each change
    generation: u64,
    /// canonical path -> generation of its last change (a change of a child counts for its folder)
    changed: HashMap<PathBuf, u64>,
    /// everything changed at this generation (the system lost events)
    all_changed_at: u64,
    /// watched paths that were removed or renamed: their watch must be set again
    removed: HashSet<PathBuf>,
    /// to ask for a new frame when something changed
    ctx: Option<egui::Context>,
    /// the last events of the system, for FileWatcher.take_events
    log: VecDeque<Logged>,
}

/// An event of the system as it came, kept for debugging
struct Logged {
    time: SystemTime,
    kind: String,
    paths: Vec<PathBuf>,
}

/// The events kept until take_events reads them
const LOG_SIZE: usize = 1000;

impl Changes {
    fn generation_of(&self, path: &Path) -> u64 {
        self.changed.get(path).copied().unwrap_or(0).max(self.all_changed_at)
    }

    fn record(&mut self, event: notify::Result<notify::Event>) {
        self.generation += 1;
        let generation = self.generation;
        let event = match event {
            Ok(event) => event,
            Err(err) => {
                self.keep(format!("Error({err})"), err.paths.clone());
                self.all_changed_at = generation;
                return self.repaint();
            }
        };
        self.keep(
            if event.need_rescan() {
                format!("{:?} (rescan)", event.kind)
            } else {
                format!("{:?}", event.kind)
            },
            event.paths.clone(),
        );
        if event.need_rescan() {
            self.all_changed_at = generation;
        }
        if matches!(event.kind, EventKind::Access(_)) {
            return; // reading a file changes nothing, and the views read
        }
        let gone = matches!(
            event.kind,
            EventKind::Remove(_) | EventKind::Modify(notify::event::ModifyKind::Name(_))
        );
        for path in event.paths {
            if let Some(parent) = path.parent() {
                self.changed.insert(parent.to_path_buf(), generation);
            }
            if gone {
                self.removed.insert(path.clone());
            }
            self.changed.insert(path, generation);
        }
        if self.changed.len() > 20_000 {
            // never grows without end: everything is read again once
            self.changed.clear();
            self.all_changed_at = generation;
        }
        self.repaint();
    }

    fn keep(&mut self, kind: String, paths: Vec<PathBuf>) {
        if self.log.len() == LOG_SIZE {
            self.log.pop_front();
        }
        self.log.push_back(Logged {
            time: SystemTime::now(),
            kind,
            paths,
        });
    }

    fn repaint(&self) {
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }
}

struct Listing {
    generation: u64,
    entries: Arc<Vec<Entry>>,
}

struct Watched {
    last_used: Instant,
    /// watched with FileWatcher.watch: kept until unwatch
    pinned: bool,
    /// the system accepted the watch
    ok: bool,
}

/// The state shared by a FileWatcher and the views that use it
pub(crate) struct Core {
    changes: Arc<Mutex<Changes>>,
    watcher: Option<RecommendedWatcher>,
    error: Option<String>,
    watched: HashMap<PathBuf, Watched>,
    /// listings by the path the views asked for
    cache: HashMap<PathBuf, Listing>,
    /// path asked for -> canonical path, the form the system gives in its events
    canonical: HashMap<PathBuf, PathBuf>,
    /// FileWatcher.changed: the generation of a path the last time it was asked
    seen: HashMap<PathBuf, u64>,
    last_cleanup: Instant,
}

/// A folder unused for this long is no longer watched
const UNUSED: Duration = Duration::from_secs(10);

impl Core {
    fn new() -> Self {
        let changes = Arc::new(Mutex::new(Changes::default()));
        let sink = changes.clone();
        let (watcher, error) = match notify::recommended_watcher(move |event| {
            if let Ok(mut changes) = sink.lock() {
                changes.record(event);
            }
        }) {
            Ok(watcher) => (Some(watcher), None),
            Err(err) => (None, Some(err.to_string())),
        };
        Core {
            changes,
            watcher,
            error,
            watched: HashMap::new(),
            cache: HashMap::new(),
            canonical: HashMap::new(),
            seen: HashMap::new(),
            last_cleanup: Instant::now(),
        }
    }

    fn changes(&self) -> MutexGuard<'_, Changes> {
        self.changes.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn canonical(&mut self, path: &Path) -> PathBuf {
        if let Some(found) = self.canonical.get(path) {
            return found.clone();
        }
        match fs::canonicalize(path) {
            Ok(found) => {
                self.canonical.insert(path.to_path_buf(), found.clone());
                found
            }
            Err(_) => path.to_path_buf(), // does not exist (yet): not remembered
        }
    }

    /// Watches the path if it is not, and marks it as used now
    fn watch(&mut self, path: &Path, pinned: bool) -> PathBuf {
        let canonical = self.canonical(path);
        let rewatch = self.changes().removed.remove(&canonical);
        let now = Instant::now();
        let watched = self.watched.entry(canonical.clone()).or_insert(Watched {
            last_used: now,
            pinned,
            ok: false,
        });
        watched.last_used = now;
        watched.pinned |= pinned;
        if rewatch || !watched.ok {
            if let Some(watcher) = &mut self.watcher {
                if rewatch {
                    let _ = watcher.unwatch(&canonical);
                    self.canonical.remove(path); // it may have been replaced by a link
                }
                // a folder that does not exist yet is tried again at the next frame
                watched.ok = canonical.exists()
                    && watcher.watch(&canonical, RecursiveMode::NonRecursive).is_ok();
            }
        }
        canonical
    }

    fn unwatch(&mut self, canonical: &Path) {
        if let Some(watched) = self.watched.remove(canonical) {
            if watched.ok {
                if let Some(watcher) = &mut self.watcher {
                    let _ = watcher.unwatch(canonical);
                }
            }
        }
    }

    fn set_ctx(&mut self, ctx: &egui::Context) {
        let mut changes = self.changes();
        if changes.ctx.is_none() {
            changes.ctx = Some(ctx.clone());
        }
    }

    /// The entries of a folder, read again only when it changed, in no particular order
    pub(crate) fn list(&mut self, ctx: Option<&egui::Context>, dir: &Path) -> Arc<Vec<Entry>> {
        if let Some(ctx) = ctx {
            self.set_ctx(ctx);
        }
        let canonical = self.watch(dir, false);
        let (generation, now) = {
            let changes = self.changes();
            (changes.generation_of(&canonical), changes.generation)
        };
        if let Some(listing) = self.cache.get(dir) {
            if listing.generation >= generation {
                let entries = listing.entries.clone();
                self.cleanup();
                return entries;
            }
        }
        // the generation is taken before reading: a change during the read is seen next frame
        let entries = Arc::new(read_dir(dir));
        self.cache.insert(
            dir.to_path_buf(),
            Listing {
                generation: now,
                entries: entries.clone(),
            },
        );
        self.cleanup();
        entries
    }

    /// Forgets the folders no view has listed for a while
    fn cleanup(&mut self) {
        if self.last_cleanup.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_cleanup = Instant::now();
        let old: Vec<PathBuf> = self
            .watched
            .iter()
            .filter(|(_, w)| !w.pinned && w.last_used.elapsed() > UNUSED)
            .map(|(path, _)| path.clone())
            .collect();
        for path in &old {
            self.unwatch(path);
        }
        let watched = &self.watched;
        let canonical = &self.canonical;
        self.cache.retain(|dir, _| {
            canonical
                .get(dir)
                .is_some_and(|c| watched.contains_key(c))
        });
    }

    fn generation_of(&mut self, path: &Path) -> u64 {
        let canonical = self.canonical(path);
        self.changes().generation_of(&canonical)
    }
}

#[cfg(target_os = "macos")]
fn hidden_flag(meta: &fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    meta.st_flags() & 0x8000 != 0 // UF_HIDDEN, e.g. ~/Library
}

#[cfg(target_os = "windows")]
fn hidden_flag(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x2 != 0 // FILE_ATTRIBUTE_HIDDEN
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn hidden_flag(_meta: &fs::Metadata) -> bool {
    false
}

pub(crate) fn entry(path: &Path) -> Option<Entry> {
    let link = fs::symlink_metadata(path).ok()?;
    // a link shows what it points to, or itself when that is gone
    let meta = if link.file_type().is_symlink() {
        fs::metadata(path).unwrap_or(link.clone())
    } else {
        link.clone()
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    Some(Entry {
        hidden: name.starts_with('.') || hidden_flag(&link),
        name,
        is_dir: meta.is_dir(),
        size: if meta.is_dir() { 0 } else { meta.len() },
        modified: meta.modified().ok(),
        path: path.to_path_buf(),
    })
}

fn read_dir(dir: &Path) -> Vec<Entry> {
    let Ok(read) = fs::read_dir(dir) else {
        return vec![];
    };
    read.filter_map(|e| e.ok())
        .filter_map(|e| entry(&e.path()))
        .collect()
}

static DEFAULT: OnceLock<Arc<Mutex<Core>>> = OnceLock::new();

/// The watcher of the views created without one
pub(crate) fn default_core() -> Arc<Mutex<Core>> {
    DEFAULT.get_or_init(|| Arc::new(Mutex::new(Core::new()))).clone()
}

pub(crate) fn lock(core: &Arc<Mutex<Core>>) -> MutexGuard<'_, Core> {
    core.lock().unwrap_or_else(|e| e.into_inner())
}

/// Keeps the file views up to date with the disk: the folders they show are watched by the
/// system, and a file created, removed, renamed or modified by any program, inside or outside the
/// app, shows at the next frame. A new frame is asked for when something changes, so the window
/// follows even when the mouse does not move.
///
/// The views create one for themselves: give the same FileWatcher to all of them (and use it for
/// your own files) to share the listings and the watches. A folder is watched while a view shows
/// it, and a few seconds more; watch() keeps a path watched until unwatch().
///
/// Usage::
///
///     watcher = FileWatcher()
///     files = FileListView("files", "~/Documents", watcher=watcher)
///     tree = FileTreeView("tree", "~/Documents", watcher=watcher)
///     notes = Path("~/notes.md").expanduser()
///     watcher.watch(notes)
///     text = notes.read_text()
///
///     def update_func(ctx):
///         global text
///         if watcher.changed(notes):  # saved by an editor: read it again
///             text = notes.read_text()
///         file_list_view(files)
///         label(text)
#[pyclass]
pub(crate) struct FileWatcher {
    pub(crate) core: Arc<Mutex<Core>>,
}

#[pymethods]
impl FileWatcher {
    #[new]
    fn new() -> Self {
        FileWatcher {
            core: Arc::new(Mutex::new(Core::new())),
        }
    }

    /// Watches a file or a folder until unwatch, even when no view shows it. See changed
    fn watch(&self, path: PathBuf) {
        lock(&self.core).watch(&expand(&path), true);
    }

    /// Stops watching a path given to watch
    fn unwatch(&self, path: PathBuf) {
        let mut core = lock(&self.core);
        let canonical = core.canonical(&expand(&path));
        core.unwatch(&canonical);
    }

    /// True once after each change of the path (the file itself, or an item of the folder) seen
    /// by the system, False otherwise. The path has to be watched: by watch(), or shown by a view.
    /// The first call returns False and starts counting
    fn changed(&self, path: PathBuf) -> bool {
        let mut core = lock(&self.core);
        let path = expand(&path);
        let canonical = core.canonical(&path);
        let generation = core.generation_of(&path);
        match core.seen.insert(canonical, generation) {
            Some(seen) => generation > seen,
            None => false,
        }
    }

    /// A number that increases at each change of a path the watcher watches: compare it with the
    /// one of the last frame to know that something changed on the disk
    #[getter]
    fn version(&self) -> u64 {
        lock(&self.core).changes().generation
    }

    /// The paths watched now, the ones the views show included
    #[getter]
    fn watched(&self) -> Vec<String> {
        let mut paths: Vec<String> = lock(&self.core)
            .watched
            .keys()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        paths.sort();
        paths
    }

    /// The events the system sent since the last call, oldest first, for debugging: each one is a
    /// dict time (timestamp), kind (the kind of notify, e.g. "Create(File)", "Modify(Name(From))",
    /// "Remove(Any)") and paths (list[str]). All events are there, also the ones the views ignore
    /// (Access). Only the last 1000 are kept between two calls
    fn take_events<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyDict>>> {
        let events: Vec<Logged> = lock(&self.core).changes().log.drain(..).collect();
        events
            .into_iter()
            .map(|e| {
                let d = PyDict::new(py);
                d.set_item(
                    "time",
                    e.time
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0),
                )?;
                d.set_item("kind", e.kind)?;
                d.set_item(
                    "paths",
                    e.paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>(),
                )?;
                Ok(d)
            })
            .collect()
    }

    /// The reason the system refused to watch, None when it works. The views still list the
    /// folders, but no longer follow the changes made outside the app
    #[getter]
    fn error(&self) -> Option<String> {
        lock(&self.core).error.clone()
    }

    /// The items of a folder, read again only when it changed, and the folder watched. Each one is
    /// a dict: path, name, is_dir, size (bytes), modified (timestamp or None), hidden. For a view
    /// of your own that stays up to date like the file views
    ///
    /// Args:
    ///     folder (str): the folder
    ///
    ///     hidden (bool): with the hidden items (a name starting with a dot, or hidden by the
    ///         system). Default is False
    #[pyo3(signature = (folder, hidden=false))]
    fn list_dir<'py>(&self, py: Python<'py>, folder: PathBuf, hidden: bool) -> PyResult<Vec<Bound<'py, PyDict>>> {
        let entries = lock(&self.core).list(None, &expand(&folder));
        let mut entries: Vec<&Entry> = entries.iter().filter(|e| hidden || !e.hidden).collect();
        entries.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        entries
            .into_iter()
            .map(|e| {
                let d = PyDict::new(py);
                d.set_item("path", e.path.to_string_lossy())?;
                d.set_item("name", &e.name)?;
                d.set_item("is_dir", e.is_dir)?;
                d.set_item("size", e.size)?;
                d.set_item(
                    "modified",
                    e.modified
                        .and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs_f64()),
                )?;
                d.set_item("hidden", e.hidden)?;
                Ok(d)
            })
            .collect()
    }
}

/// ~ is the home folder
pub(crate) fn expand(path: &Path) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

/// Compares names like a file manager: without case, and "file2" before "file10"
pub(crate) fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = a.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    a.next();
                }
                let mut nb = String::new();
                while let Some(c) = b.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    b.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let order = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x
                    .to_lowercase()
                    .cmp(y.to_lowercase())
                    .then(Ordering::Equal);
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<FileWatcher>()?;
    Ok(())
}
