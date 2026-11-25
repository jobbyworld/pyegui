//! PDF: what a file says about itself (`pdf_info`), read with `craft-pdf` (hayro), without Python.
//!
//! The file is read and parsed with the GIL released, so a big PDF read in a thread does not make the
//! window stutter (pypdf, plain Python, did). Nothing is raised for a bad file: the reason is in the
//! `error` key of the answer.

use pyo3::prelude::*;
use pyo3::types::PyDict;

pub(crate) use pyegui_core::pdf::Info;
use pyegui_core::pdf::{info_of_path, PERMISSIONS};

/// What the Python side gets: the dict of `pdf_info`.
pub(crate) fn info_dict<'py>(py: Python<'py>, info: &Info) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("pages", info.pages)?;
    // a file that could not be read tells nothing about its encryption and its rights
    let known = info.error.is_none();
    d.set_item("encrypted", known.then_some(u8::from(info.encrypted)))?;
    d.set_item("needs_password", known.then_some(u8::from(info.needs_password)))?;
    for (key, bit) in PERMISSIONS {
        // not encrypted, or rights unknown: everything is allowed
        let allowed = info.permissions.is_none_or(|p| p & (1 << (bit - 1)) != 0);
        d.set_item(key, known.then_some(u8::from(allowed)))?;
    }
    d.set_item("title", &info.title)?;
    d.set_item("author", &info.author)?;
    d.set_item("subject", &info.subject)?;
    d.set_item("keywords", &info.keywords)?;
    d.set_item("creator", &info.creator)?;
    d.set_item("producer", &info.producer)?;
    d.set_item("created", info.created)?;
    d.set_item("modified", info.modified)?;
    d.set_item("pdf_version", &info.pdf_version)?;
    d.set_item("error", &info.error)?;
    Ok(d)
}

/// Reads what a PDF says about itself: its page count, version, metadata, dates and what it
/// forbids, without any other library (it replaces pypdf).
///
/// Returns a dict that always has the same keys, None where something is unknown or missing:
/// `pages`, `encrypted` (0 or 1), `needs_password` (0 or 1), the rights `can_print`, `can_copy`,
/// `can_modify`, `can_annotate`, `can_fill_forms`, `can_assemble` (1 allowed, 0 forbidden), the
/// texts `title`, `author`, `subject`, `keywords`, `creator`, `producer`, the dates `created` and
/// `modified` (seconds since the epoch), `pdf_version` (``"1.7"``) and `error`.
///
/// Nothing is raised: a file that cannot be read gives the reason in `error`. The file is read and
/// parsed with the GIL released, so it can be called from a thread, and it is closed at once (it
/// can be moved or deleted afterwards on Windows).
///
/// The rights are those written in the PDF; only the viewers respect them. A PDF that asks a
/// password to open (`needs_password`) gives neither its pages nor its metadata, but its rights are
/// still read, they are not encrypted. A PDF with an owner password only opens with an empty one,
/// as in every viewer.
///
/// Args:
///     path (str): path of the PDF file
///
/// Example::
///
///     info = pdf_info("contract.pdf")
///     if info["error"] is None:
///         print(info["pages"], info["title"], info["can_print"])
#[pyfunction]
fn pdf_info<'py>(py: Python<'py>, path: String) -> PyResult<Bound<'py, PyDict>> {
    let info = py.detach(|| info_of_path(&path));
    info_dict(py, &info)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(pdf_info, m)?)?;
    Ok(())
}
