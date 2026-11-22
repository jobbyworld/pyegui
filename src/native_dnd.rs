//! Drag and drop with the file manager of the system: files dropped on the window from the Finder
//! (the Explorer), and files dragged out of the window to the Finder (the Explorer, the desktop,
//! another application).

use std::path::PathBuf;

use pyo3::prelude::*;

use crate::{panel_parent_ui, UI};
use pyegui_core::native_dnd;

/// Starts a drag of files out of the window, to the Finder or to any application that takes files.
/// Call it on the frame a drag starts on an item (``response.drag_started`` of an
/// ``interactive`` area, or a drag of the file views with ``drag_out=True``): the mouse button has
/// to be down. The drag goes on by itself, with the icons of the files under the pointer, and the
/// drop is done by the receiver (a copy in the Finder, a move with Cmd held). The window gets no
/// pointer event until the mouse button is released. Works on macOS and on Windows (on Windows the
/// call returns once the drag is over), returns False elsewhere or if there is nothing to drag.
///
/// Args:
///     paths (list[str]): the files and folders to drag
///
/// Returns:
///     bool: True if the drag started
///
/// Example::
///
///     r = interactive("row", lambda: label(name))
///     if r.drag_started:
///         start_file_drag([path])
#[pyfunction]
fn start_file_drag(paths: Vec<String>) -> bool {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).filter(|p| p.exists()).collect();
    if paths.is_empty() {
        return false;
    }
    native_dnd::start_drag(&paths)
}

/// The modifier keys held now, read from the system. Unlike ``modifiers`` it is right while files
/// are dragged, when the window gets no keyboard event: it is what decides if a drop copies or
/// moves. ``command`` is the Command key on macOS and Ctrl elsewhere; ``alt`` is Option on macOS.
///
/// Returns:
///     dict: the keys shift, ctrl, alt and command, each a bool
///
/// Example::
///
///     if dropped_files():
///         copy = system_modifiers()["alt"]  # Option copies on a same disk, on macOS
#[pyfunction]
unsafe fn system_modifiers(py: Python<'_>) -> PyResult<Bound<'_, pyo3::types::PyDict>> {
    let dict = pyo3::types::PyDict::new(py);
    let (shift, ctrl, alt, command) = match native_dnd::held_modifiers() {
        Some(held) => held,
        None => {
            let held = panel_parent_ui(&UI)?.ctx().input(|i| i.modifiers);
            (held.shift, held.ctrl, held.alt, held.command)
        }
    };
    dict.set_item("shift", shift)?;
    dict.set_item("ctrl", ctrl)?;
    dict.set_item("alt", alt)?;
    dict.set_item("command", command)?;
    Ok(dict)
}

/// The files of the last drag started by this window (``start_file_drag``, the drag of the file
/// views). When ``dropped_files`` gives these same files, the drop comes from the window itself:
/// the app moves them instead of copying them, as the Finder does inside a disk.
///
/// Returns:
///     list[str]: the full paths, empty before a first drag
///
/// Example::
///
///     dropped = dropped_files()
///     if dropped and sorted(dropped) == sorted(dragged_files()):
///         move(dropped, folder)
#[pyfunction]
fn dragged_files() -> Vec<String> {
    native_dnd::dragged_files().iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

/// The files dropped on the window this frame, from the Finder (the Explorer or another
/// application). Empty most of the time. The drop is a copy request: it is up to the app to copy
/// or move them, see ``hovered_files`` to show where they would go.
///
/// Returns:
///     list[str]: the full paths
///
/// Example::
///
///     for path in dropped_files():
///         shutil.copy(path, current_folder)
#[pyfunction]
unsafe fn dropped_files() -> PyResult<Vec<String>> {
    let ctx = panel_parent_ui(&UI)?.ctx().clone();
    Ok(native_dnd::dropped_files(&ctx).iter().map(|p| p.to_string_lossy().into_owned()).collect())
}

/// The files being dragged over the window, from the Finder (the Explorer or another application),
/// not dropped yet. Empty when nothing is dragged over the window. It is the moment to show a
/// frame or a message "Drop here".
///
/// Returns:
///     list[str]: the full paths
///
/// Example::
///
///     if hovered_files():
///         draw_rect(rect, stroke=(accent, 2))
#[pyfunction]
unsafe fn hovered_files() -> PyResult<Vec<String>> {
    let ctx = panel_parent_ui(&UI)?.ctx().clone();
    Ok(native_dnd::hovered_files(&ctx).iter().map(|p| p.to_string_lossy().into_owned()).collect())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // the file views of the core take part in the drag and drop of the system through these
    native_dnd::register();
    m.add_function(wrap_pyfunction!(start_file_drag, m)?)?;
    m.add_function(wrap_pyfunction!(dropped_files, m)?)?;
    m.add_function(wrap_pyfunction!(dragged_files, m)?)?;
    m.add_function(wrap_pyfunction!(system_modifiers, m)?)?;
    m.add_function(wrap_pyfunction!(hovered_files, m)?)?;
    Ok(())
}
