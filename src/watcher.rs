//! FileWatcher: the folders a file view shows are listed once and watched by the system
//! (FSEvents on macOS, ReadDirectoryChangesW on Windows, inotify on Linux). A change made by any
//! program invalidates the listing and asks egui for a new frame, so the views follow it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use pyo3::prelude::*;
use pyo3::types::PyDict;

pub(crate) use pyegui_core::watcher::{default_core, expand, lock, Core};

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
        lock(&self.core).watch_path(&path);
    }

    /// Stops watching a path given to watch
    fn unwatch(&self, path: PathBuf) {
        lock(&self.core).unwatch_path(&path);
    }

    /// True once after each change of the path (the file itself, or an item of the folder) seen
    /// by the system, False otherwise. The path has to be watched: by watch(), or shown by a view.
    /// The first call returns False and starts counting
    fn changed(&self, path: PathBuf) -> bool {
        lock(&self.core).has_changed(&path)
    }

    /// A number that increases at each change of a path the watcher watches: compare it with the
    /// one of the last frame to know that something changed on the disk
    #[getter]
    fn version(&self) -> u64 {
        lock(&self.core).version()
    }

    /// The paths watched now, the ones the views show included
    #[getter]
    fn watched(&self) -> Vec<String> {
        lock(&self.core).watched_paths()
    }

    /// The events the system sent since the last call, oldest first, for debugging: each one is a
    /// dict time (timestamp), kind (the kind of notify, e.g. "Create(File)", "Modify(Name(From))",
    /// "Remove(Any)") and paths (list[str]). All events are there, also the ones the views ignore
    /// (Access). Only the last 1000 are kept between two calls
    fn take_events<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyDict>>> {
        lock(&self.core)
            .take_events()
            .into_iter()
            .map(|e| {
                let d = PyDict::new(py);
                d.set_item("time", e.time)?;
                d.set_item("kind", e.kind)?;
                d.set_item("paths", e.paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>())?;
                Ok(d)
            })
            .collect()
    }

    /// The reason the system refused to watch, None when it works. The views still list the
    /// folders, but no longer follow the changes made outside the app
    #[getter]
    fn error(&self) -> Option<String> {
        lock(&self.core).error()
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
        let entries = lock(&self.core).list_dir(&folder, hidden);
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

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<FileWatcher>()?;
    Ok(())
}
