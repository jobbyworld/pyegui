//! FileWatcher: the folders a file view shows are listed once and watched by the system
//! (FSEvents on macOS, ReadDirectoryChangesW on Windows, inotify on Linux). A change made by any
//! program invalidates the listing and asks egui for a new frame, so the views follow it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use egui;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// A file or folder of a listing
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub hidden: bool,
}

/// What the thread of the system watcher writes: the paths that changed and when
#[derive(Default)]
pub struct Changes {
    /// increases at each change
    pub generation: u64,
    /// canonical path -> generation of its last change (a change of a child counts for its folder)
    pub changed: HashMap<PathBuf, u64>,
    /// everything changed at this generation (the system lost events)
    pub all_changed_at: u64,
    /// watched paths that were removed or renamed: their watch must be set again
    pub removed: HashSet<PathBuf>,
    /// to ask for a new frame when something changed
    pub ctx: Option<egui::Context>,
    /// the last events of the system, for FileWatcher.take_events
    pub log: VecDeque<Logged>,
}

/// An event of the system as it came, kept for debugging
pub struct Logged {
    pub time: SystemTime,
    pub kind: String,
    pub paths: Vec<PathBuf>,
}

/// The events kept until take_events reads them
const LOG_SIZE: usize = 1000;

impl Changes {
    pub fn generation_of(&self, path: &Path) -> u64 {
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

pub struct Listing {
    pub generation: u64,
    pub entries: Arc<Vec<Entry>>,
}

pub struct Watched {
    pub last_used: Instant,
    /// watched with FileWatcher.watch: kept until unwatch
    pub pinned: bool,
    /// the system accepted the watch
    pub ok: bool,
}

/// The state shared by a FileWatcher and the views that use it
pub struct Core {
    pub changes: Arc<Mutex<Changes>>,
    pub watcher: Option<RecommendedWatcher>,
    pub error: Option<String>,
    pub watched: HashMap<PathBuf, Watched>,
    /// listings by the path the views asked for
    pub cache: HashMap<PathBuf, Listing>,
    /// path asked for -> canonical path, the form the system gives in its events
    pub canonical: HashMap<PathBuf, PathBuf>,
    /// FileWatcher.changed: the generation of a path the last time it was asked
    pub seen: HashMap<PathBuf, u64>,
    pub last_cleanup: Instant,
}

/// An event of the system, as `Core::take_events` gives it
pub struct Event {
    /// seconds since the epoch
    pub time: f64,
    /// the kind of notify, e.g. "Create(File)", "Modify(Name(From))", "Remove(Any)"
    pub kind: String,
    pub paths: Vec<PathBuf>,
}

/// A folder unused for this long is no longer watched
const UNUSED: Duration = Duration::from_secs(10);

impl Core {
    pub fn new() -> Self {
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

    pub fn changes(&self) -> MutexGuard<'_, Changes> {
        self.changes.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn canonical(&mut self, path: &Path) -> PathBuf {
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
    pub fn watch(&mut self, path: &Path, pinned: bool) -> PathBuf {
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

    pub fn unwatch(&mut self, canonical: &Path) {
        if let Some(watched) = self.watched.remove(canonical) {
            if watched.ok {
                if let Some(watcher) = &mut self.watcher {
                    let _ = watcher.unwatch(canonical);
                }
            }
        }
    }

    pub fn set_ctx(&mut self, ctx: &egui::Context) {
        let mut changes = self.changes();
        if changes.ctx.is_none() {
            changes.ctx = Some(ctx.clone());
        }
    }

    /// The entries of a folder, read again only when it changed, in no particular order
    pub fn list(&mut self, ctx: Option<&egui::Context>, dir: &Path) -> Arc<Vec<Entry>> {
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
    pub fn cleanup(&mut self) {
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

    pub fn generation_of(&mut self, path: &Path) -> u64 {
        let canonical = self.canonical(path);
        self.changes().generation_of(&canonical)
    }

    /// Watches a file or a folder until `unwatch`, even when no view shows it
    pub fn watch_path(&mut self, path: &Path) {
        self.watch(&expand(path), true);
    }

    /// Stops watching a path given to `watch_path`
    pub fn unwatch_path(&mut self, path: &Path) {
        let canonical = self.canonical(&expand(path));
        self.unwatch(&canonical);
    }

    /// True once after each change of the path (the file itself, or an item of the folder) seen by
    /// the system. The first call returns false and starts counting
    pub fn has_changed(&mut self, path: &Path) -> bool {
        let path = expand(path);
        let canonical = self.canonical(&path);
        let generation = self.generation_of(&path);
        match self.seen.insert(canonical, generation) {
            Some(seen) => generation > seen,
            None => false,
        }
    }

    /// A number that increases at each change of a path the watcher watches
    pub fn version(&self) -> u64 {
        self.changes().generation
    }

    /// The paths watched now, the ones the views show included, sorted
    pub fn watched_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.watched.keys().map(|p| p.to_string_lossy().into_owned()).collect();
        paths.sort();
        paths
    }

    /// The paths watched now with their pinned state, sorted by path
    pub fn watched_list(&self) -> Vec<(PathBuf, bool)> {
        let mut paths: Vec<(PathBuf, bool)> = self.watched.iter().map(|(p, w)| (p.clone(), w.pinned)).collect();
        paths.sort();
        paths
    }

    /// Pins (kept until `unwatch`) or unpins a watched path, given in the canonical form of `watched_list`
    pub fn pin(&mut self, canonical: &Path, pinned: bool) {
        if let Some(w) = self.watched.get_mut(canonical) {
            w.pinned = pinned;
            w.last_used = Instant::now();
        }
    }

    /// The events of the system since the last call, oldest first (the last 1000 are kept)
    pub fn take_events(&self) -> Vec<Event> {
        let events: Vec<Logged> = self.changes().log.drain(..).collect();
        events
            .into_iter()
            .map(|e| Event {
                time: e.time.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0),
                kind: e.kind,
                paths: e.paths,
            })
            .collect()
    }

    /// The reason the system refused to watch, None when it works
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }

    /// The items of a folder sorted like a file manager, read again only when it changed
    pub fn list_dir(&mut self, folder: &Path, hidden: bool) -> Vec<Entry> {
        let entries = self.list(None, &expand(folder));
        let mut entries: Vec<Entry> = entries.iter().filter(|e| hidden || !e.hidden).cloned().collect();
        entries.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        entries
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

pub fn entry(path: &Path) -> Option<Entry> {
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
pub fn default_core() -> Arc<Mutex<Core>> {
    DEFAULT.get_or_init(|| Arc::new(Mutex::new(Core::new()))).clone()
}

pub fn lock(core: &Arc<Mutex<Core>>) -> MutexGuard<'_, Core> {
    core.lock().unwrap_or_else(|e| e.into_inner())
}

/// ~ is the home folder
pub fn expand(path: &Path) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

/// Compares names like a file manager: without case, and "file2" before "file10"
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
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
