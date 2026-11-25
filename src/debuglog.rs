//! The debug log of the Python side: the same sink as the Rust code (terminal and file), so that the events
//! of the app (SQL writes of `finder.py`) and those of pyegui (PDF metadata read, mermaid diagrams
//! rendered) come out in one file, in order.

use pyo3::prelude::*;
use std::path::PathBuf;

/// Turns the debug log on: the events are written to the terminal (stderr) and, with `path`, appended to that file.
///
/// pyegui itself logs the PDF metadata it reads (`pdf_info`, the PDF views) and the mermaid diagrams it
/// pre-renders for `markdown`; an app adds its own events with `debug_log`. Off by default: a call to
/// `debug_log` then does nothing.
///
/// Args:
///     path (str, optional): file to append to. Its folder is created. Without it the log stays in the terminal.
///
/// Raises:
///     OSError: when the file cannot be opened.
///
/// Example::
///
///     from pyegui import debug_log_enable, debug_log
///     debug_log_enable("/tmp/myapp.log")
///     debug_log("sqlite", "3 files recorded")
#[pyfunction]
#[pyo3(signature = (path=None))]
fn debug_log_enable(path: Option<PathBuf>) -> PyResult<()> {
    pyegui_core::debuglog::enable(path.as_deref())?;
    Ok(())
}

/// Writes one line `HH:MM:SS.mmm [category] message` to the debug log. Does nothing while it is off.
///
/// Args:
///     category (str): what the event is about ("sqlite", "pdf", "mermaid"...).
///     message (str): the event.
///
/// Example::
///
///     debug_log("sqlite", f"{n} files recorded in {folder}")
#[pyfunction]
fn debug_log(category: &str, message: &str) {
    pyegui_core::debuglog::log(category, message);
}

/// Whether the debug log is on (to skip building an expensive message).
///
/// Example::
///
///     if debug_enabled():
///         debug_log("sqlite", describe_changes())
#[pyfunction]
fn debug_enabled() -> bool {
    pyegui_core::debuglog::enabled()
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(debug_log_enable, m)?)?;
    m.add_function(wrap_pyfunction!(debug_log, m)?)?;
    m.add_function(wrap_pyfunction!(debug_enabled, m)?)?;
    Ok(())
}
