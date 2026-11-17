//! What an app does with files dropped on it: copy or move them, by the rules of the file managers,
//! in a thread with a progress to draw.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::{panel_parent_ui, UI};

/// The disk of a path: the device number on Unix, the drive (or the share) on Windows
#[cfg(unix)]
fn volume_of(path: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    // a path that does not exist yet is on the volume of its first parent that does
    path.ancestors().find_map(|p| fs::metadata(p).ok()).map(|m| m.dev().to_string())
}

#[cfg(not(unix))]
fn volume_of(path: &Path) -> Option<String> {
    use std::path::Component;
    let full = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().ok()?.join(path) };
    match full.components().next()? {
        Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().to_lowercase()),
        _ => Some(String::new()),
    }
}

fn same_volume(a: &Path, b: &Path) -> bool {
    match (volume_of(a), volume_of(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// True if two paths are on the same disk (the same volume): a drop between them moves, else it
/// copies. A path that does not exist yet counts for its first parent that does.
///
/// Args:
///     a (str): a path
///     b (str): another path
///
/// Returns:
///     bool: True if both are on the same volume
///
/// Example::
///
///     same_volume("/Users/me/a.txt", "/Volumes/Backup/")  # False
#[pyfunction(name = "same_volume")]
fn py_same_volume(a: PathBuf, b: PathBuf) -> bool {
    same_volume(&a, &b)
}

/// What a drop of files on a folder does by the rules of the file managers, with the keys held now:
/// on the same volume the files are moved, and Option (macOS) or Ctrl (Windows, Linux) copies them
/// instead; on two volumes they are copied, and Cmd (macOS) or Shift (Windows, Linux) moves them
/// instead. The keys are read from the system, so it is right while files are dragged over the
/// window. Call it when the files hover (to say it) and again on the frame they are dropped.
///
/// Args:
///     sources (list[str]): the files dropped, see ``dropped_files`` and ``hovered_files``
///     folder (str): the folder they are dropped on
///
/// Returns:
///     str: "move" or "copy", the ``operation`` to give to ``FileTransfer.start``
///
/// Example::
///
///     for_folder = view.drop_target or current_folder
///     if hovered_files():
///         label(f"Drop to {drop_operation(hovered_files(), for_folder)} here")
#[pyfunction]
unsafe fn drop_operation(sources: Vec<PathBuf>, folder: PathBuf) -> PyResult<&'static str> {
    let (shift, ctrl, alt, command) = match crate::native_dnd::held_modifiers() {
        Some(held) => held,
        None => {
            let held = panel_parent_ui(&UI)?.ctx().input(|i| i.modifiers);
            (held.shift, held.ctrl, held.alt, held.command)
        }
    };
    let mac = cfg!(target_os = "macos");
    let force_move = if mac { command } else { shift };
    let force_copy = if mac { alt } else { ctrl };
    if !sources.is_empty() && sources.iter().all(|s| same_volume(s, &folder)) {
        return Ok(if force_copy { "copy" } else { "move" });
    }
    Ok(if force_move { "move" } else { "copy" })
}

/// path, or "name copy", "name copy 2"... when it exists
fn unique(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut n = 1;
    loop {
        let suffix = if n == 1 { " copy".to_string() } else { format!(" copy {n}") };
        let candidate = path.with_file_name(format!("{stem}{suffix}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// The bytes of a file, or of all the files of a folder
fn tree_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else { return 0 };
    if meta.is_dir() {
        fs::read_dir(path).map(|it| it.flatten().map(|e| tree_size(&e.path())).sum()).unwrap_or(0)
    } else {
        meta.len()
    }
}

#[derive(Default)]
struct State {
    /// the items not done yet
    running: AtomicUsize,
    total: AtomicU64,
    done: AtomicU64,
    /// (folder, target) of the items done, until read
    finished: Mutex<Vec<(PathBuf, PathBuf)>>,
    errors: Mutex<Vec<String>>,
}

fn copy_file(source: &Path, target: &Path, state: &State) -> std::io::Result<()> {
    let mut src = fs::File::open(source)?;
    let mut dst = fs::File::create(target)?;
    let mut block = vec![0u8; 4 * 1024 * 1024];
    loop {
        let n = src.read(&mut block)?;
        if n == 0 {
            break;
        }
        dst.write_all(&block[..n])?;
        state.done.fetch_add(n as u64, Ordering::Relaxed);
    }
    // the permissions and the dates, like a copy of the file managers
    if let Ok(meta) = src.metadata() {
        let _ = fs::set_permissions(target, meta.permissions());
        if let Ok(modified) = meta.modified() {
            let _ = dst.set_modified(modified);
        }
    }
    Ok(())
}

/// Copies a file or a folder, adding the bytes copied to the progress
fn copy_tree(source: &Path, target: &Path, state: &State) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(source)?;
    if meta.is_dir() {
        fs::create_dir(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &target.join(entry.file_name()), state)?;
        }
        let _ = fs::set_permissions(target, meta.permissions());
        return Ok(());
    }
    #[cfg(unix)]
    if meta.file_type().is_symlink() {
        return std::os::unix::fs::symlink(fs::read_link(source)?, target);
    }
    copy_file(source, target, state)
}

fn remove_tree(path: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn transfer_one(source: &Path, folder: &Path, moving: bool, state: &State) -> Result<PathBuf, String> {
    if !source.exists() && fs::symlink_metadata(source).is_err() {
        return Err("not found".into());
    }
    if folder.starts_with(source) {
        return Err("a folder cannot go into itself".into());
    }
    let name = source.file_name().ok_or("no name")?;
    let target = unique(&folder.join(name));
    let size = tree_size(source);
    state.total.fetch_add(size, Ordering::Relaxed);
    if moving && same_volume(source, folder) {
        fs::rename(source, &target).map_err(|e| e.to_string())?; // no byte moves: done at once
        state.done.fetch_add(size, Ordering::Relaxed);
    } else {
        copy_tree(source, &target, state).map_err(|e| e.to_string())?;
        if moving {
            remove_tree(source).map_err(|e| e.to_string())?;
        }
    }
    Ok(target)
}

/// Copies or moves files into a folder in a thread, and tells how far it is, so that the window
/// does not freeze on a big file and can draw a progress bar. A name that exists is not replaced:
/// the new one is "name copy", "name copy 2"... A move on the same volume is a rename, between two
/// volumes it is a copy followed by the removal of the original. Create it once, outside the update
/// function, and read it at each frame.
///
/// Usage::
///
///     transfer = FileTransfer()
///
///     def update_func(ctx):
///         sources, folder = dropped_files(), files.drop_target or current
///         if sources:
///             transfer.start(sources, folder, operation=drop_operation(sources, folder))
///         if transfer.running:
///             progress(transfer.progress)
///         for folder, target in transfer.finished():
///             refresh(folder)
///         for error in transfer.errors():
///             print(error)
#[pyclass]
pub(crate) struct FileTransfer {
    state: Arc<State>,
}

#[pymethods]
impl FileTransfer {
    #[new]
    fn new() -> Self {
        FileTransfer { state: Arc::new(State::default()) }
    }

    /// Starts to copy or move the files into the folder and returns at once. Files started while
    /// others are still going add to the same progress
    ///
    /// Args:
    ///     sources (list[str]): the files and folders
    ///     folder (str): the folder to put them in
    ///     operation (str): "copy" (the default) or "move", see ``drop_operation``
    #[pyo3(signature = (sources, folder, operation="copy"))]
    fn start(&self, sources: Vec<PathBuf>, folder: PathBuf, operation: &str) -> PyResult<()> {
        let moving = match operation {
            "move" => true,
            "copy" => false,
            other => return Err(PyValueError::new_err(format!("operation is \"copy\" or \"move\", not \"{other}\""))),
        };
        if sources.is_empty() {
            return Ok(());
        }
        let state = self.state.clone();
        if state.running.load(Ordering::SeqCst) == 0 {
            state.total.store(0, Ordering::SeqCst);
            state.done.store(0, Ordering::SeqCst);
        }
        state.running.fetch_add(sources.len(), Ordering::SeqCst);
        std::thread::spawn(move || {
            for source in sources {
                match transfer_one(&source, &folder, moving, &state) {
                    Ok(target) => state.finished.lock().unwrap().push((folder.clone(), target)),
                    Err(error) => state.errors.lock().unwrap().push(format!("{}: {error}", source.display())),
                }
                state.running.fetch_sub(1, Ordering::SeqCst);
            }
        });
        Ok(())
    }

    /// The number of items not done yet. 0 when nothing is going on
    #[getter]
    fn running(&self) -> usize {
        self.state.running.load(Ordering::SeqCst)
    }

    /// The bytes to copy or move since the transfer started (it grows as the items are measured)
    #[getter]
    fn total(&self) -> u64 {
        self.state.total.load(Ordering::Relaxed)
    }

    /// The bytes done
    #[getter]
    fn done(&self) -> u64 {
        self.state.done.load(Ordering::Relaxed)
    }

    /// How far it is, from 0 to 1, for ``progress``
    #[getter]
    fn progress(&self) -> f64 {
        let total = self.state.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        (self.state.done.load(Ordering::Relaxed) as f64 / total as f64).min(1.0)
    }

    /// The items done since the last call, as (folder, new path) pairs, oldest first. Each is
    /// returned once: it is the moment to refresh the folder and to select the new items
    fn finished(&self) -> Vec<(String, String)> {
        let mut finished = self.state.finished.lock().unwrap();
        finished.drain(..).map(|(f, t)| (f.to_string_lossy().into_owned(), t.to_string_lossy().into_owned())).collect()
    }

    /// The failures since the last call, as "source: reason" texts. Each is returned once
    fn errors(&self) -> Vec<String> {
        self.state.errors.lock().unwrap().drain(..).collect()
    }
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<FileTransfer>()?;
    m.add_function(wrap_pyfunction!(drop_operation, m)?)?;
    m.add_function(wrap_pyfunction!(py_same_volume, m)?)?;
    Ok(())
}
