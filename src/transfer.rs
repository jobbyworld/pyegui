//! What an app does with files dropped on it: copy or move them, by the rules of the file managers,
//! in a thread with a progress to draw.

use std::path::PathBuf;

use pyegui_core::transfer::{drop_operation_for, same_volume, Transfer};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::{panel_parent_ui, UI};

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
    let held = match pyegui_core::native_dnd::held_modifiers() {
        Some(held) => held,
        None => {
            let held = panel_parent_ui(&UI)?.ctx().input(|i| i.modifiers);
            (held.shift, held.ctrl, held.alt, held.command)
        }
    };
    Ok(drop_operation_for(held, &sources, &folder))
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
    inner: Transfer,
}

#[pymethods]
impl FileTransfer {
    #[new]
    fn new() -> Self {
        FileTransfer { inner: Transfer::new() }
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
        self.inner.start(sources, folder, moving);
        Ok(())
    }

    /// The number of items not done yet. 0 when nothing is going on
    #[getter]
    fn running(&self) -> usize {
        self.inner.running()
    }

    /// The bytes to copy or move since the transfer started (it grows as the items are measured)
    #[getter]
    fn total(&self) -> u64 {
        self.inner.total()
    }

    /// The bytes done
    #[getter]
    fn done(&self) -> u64 {
        self.inner.done()
    }

    /// How far it is, from 0 to 1, for ``progress``
    #[getter]
    fn progress(&self) -> f64 {
        self.inner.progress()
    }

    /// The items done since the last call, as (folder, new path) pairs, oldest first. Each is
    /// returned once: it is the moment to refresh the folder and to select the new items
    fn finished(&self) -> Vec<(String, String)> {
        self.inner
            .finished()
            .into_iter()
            .map(|(f, t)| (f.to_string_lossy().into_owned(), t.to_string_lossy().into_owned()))
            .collect()
    }

    /// The failures since the last call, as "source: reason" texts. Each is returned once
    fn errors(&self) -> Vec<String> {
        self.inner.errors()
    }
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<FileTransfer>()?;
    m.add_function(wrap_pyfunction!(drop_operation, m)?)?;
    m.add_function(wrap_pyfunction!(py_same_volume, m)?)?;
    Ok(())
}
