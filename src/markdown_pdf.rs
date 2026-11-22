//! `markdown_to_pdf`: Python wrapper; the work is done by `pyegui_core::markdown_pdf`.

use std::collections::HashMap;
use std::path::PathBuf;

use pyegui_core::markdown_pdf::{build, Bar, Cover, Geo, Options, Theme};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

// ---- python ------------------------------------------------------------------------------------------

fn page_size(value: &Bound<'_, PyAny>) -> PyResult<(f32, f32)> {
    const MM: f32 = 72.0 / 25.4;
    if let Ok(name) = value.extract::<String>() {
        return match name.to_lowercase().as_str() {
            "a3" => Ok((297.0 * MM, 420.0 * MM)),
            "a4" => Ok((210.0 * MM, 297.0 * MM)),
            "a5" => Ok((148.0 * MM, 210.0 * MM)),
            "letter" => Ok((612.0, 792.0)),
            "legal" => Ok((612.0, 1008.0)),
            _ => Err(PyValueError::new_err(format!(
                "Unknown page size '{name}', expected \"A3\", \"A4\", \"A5\", \"Letter\", \"Legal\" or a (width, height) in millimeters"
            ))),
        };
    }
    let (w, h): (f32, f32) = value.extract()?;
    Ok((w * MM, h * MM))
}

/// A dict of strings
fn text_dict(value: &Bound<'_, PyAny>, name: &str) -> PyResult<HashMap<String, String>> {
    value
        .extract::<HashMap<String, String>>()
        .map_err(|_| PyValueError::new_err(format!("'{name}' takes a dict of strings")))
}

/// `header` and `footer`: a flag, a text for the center or a dict; None when the key is absent
fn bar_of(value: Option<Bound<'_, PyAny>>, name: &str, default: Option<Bar>) -> PyResult<Option<Bar>> {
    let Some(value) = value else { return Ok(default) };
    if let Ok(flag) = value.extract::<bool>() {
        return Ok(flag.then(|| Bar { center: "{page} / {pages}".into(), ..Default::default() }));
    }
    if let Ok(text) = value.extract::<String>() {
        return Ok(Some(Bar { center: text, ..Default::default() }));
    }
    let d = text_dict(&value, name)?;
    Ok(Some(Bar {
        left: d.get("left").cloned().unwrap_or_default(),
        center: d.get("center").cloned().unwrap_or_default(),
        right: d.get("right").cloned().unwrap_or_default(),
    }))
}

/// Turns a markdown text into a PDF, drawn like `markdown()` is.
///
/// The text is drawn by the same code as the viewer: headings, lists, tables, quotes, code blocks with their
/// colors, links, images and mermaid diagrams. The pages are cut between two lines of text, and a heading
/// is never alone at the bottom of a page. The text stays text (vector, selectable), the headings become
/// the bookmarks of the file and the links become links of the PDF. No window is needed and no ui is
/// involved: it can be called from a thread.
///
/// Args:
///     text (str): the markdown
///
///     path (str | None): the file to write. None: the bytes of the PDF are returned
///
/// Keyword args:
///
///     theme (str): "light" (the default), "dark" (dark pages) or "print" (black text, white paper)
///
///     title (str): title of the document, of its cover and of `{title}` in the header and the footer.
///     Default is the first heading of level 1
///
///     author (str): author written in the properties of the PDF
///
///     cover (dict | bool): a cover page: `dict(title=..., subtitle=..., author=..., date=...)`, every key is
///     optional. True is a cover with the title only
///
///     toc (bool | str | dict): a table of contents with the page of every heading, linked to the pages.
///     A str is its title, a dict is `dict(title="Contents", depth=3)`. Default is no table of contents
///
///     bookmarks (bool): the headings as the bookmarks of the PDF. Default is True
///
///     header (str | dict): text at the top of the pages, or `dict(left=..., center=..., right=...)`
///
///     footer (bool | str | dict): same as `header`. `{page}`, `{pages}` and `{title}` are replaced.
///     Default is "{page} / {pages}" in the center, False for none. No header and no footer on the cover
///
///     page_size (str | tuple): "A3", "A4" (the default), "A5", "Letter", "Legal" or (width, height) in mm
///
///     margin (float): margin of the pages in mm. Default is 20
///
///     base_dir (str): the folder that the relative paths of the images are read from
///
/// Returns:
///     bytes | None: the PDF when `path` is None
///
/// Example::
///
///     markdown_to_pdf(Path("README.md").read_text(encoding="utf-8"), "README.pdf",
///                     theme="light", cover=dict(subtitle="Version 1.0", author="Joachim"),
///                     toc=True, footer="{title}  -  {page} / {pages}", base_dir=".")
#[pyfunction]
#[pyo3(signature = (text, path=None, **kwargs))]
fn markdown_to_pdf<'py>(
    py: Python<'py>,
    text: &str,
    path: Option<String>,
    kwargs: Option<&Bound<'py, PyDict>>,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    let item = |key: &str| -> PyResult<Option<Bound<'py, PyAny>>> {
        Ok(match kwargs {
            Some(k) => k.get_item(key)?.filter(|v| !v.is_none()),
            None => None,
        })
    };
    let string = |key: &str| -> PyResult<Option<String>> { crate::kwarg::<String>(kwargs, key) };

    let theme_name = string("theme")?.unwrap_or_else(|| "light".into());
    let theme = Theme::parse(&theme_name).ok_or_else(|| {
        PyValueError::new_err(format!("Unknown theme '{theme_name}', expected \"light\", \"dark\" or \"print\""))
    })?;
    let (w, h) = match item("page_size")? {
        Some(v) => page_size(&v)?,
        None => (210.0 * 72.0 / 25.4, 297.0 * 72.0 / 25.4),
    };
    let margin = crate::kwarg::<f32>(kwargs, "margin")?.unwrap_or(20.0) * 72.0 / 25.4;
    if margin < 0.0 || margin * 2.0 >= w.min(h) {
        return Err(PyValueError::new_err("'margin' is too big for the page"));
    }

    let cover = match item("cover")? {
        None => None,
        Some(v) => match v.extract::<bool>() {
            Ok(flag) => flag.then(Cover::default),
            Err(_) => {
                let d = text_dict(&v, "cover")?;
                Some(Cover {
                    title: d.get("title").cloned(),
                    subtitle: d.get("subtitle").cloned(),
                    author: d.get("author").cloned(),
                    date: d.get("date").cloned(),
                })
            }
        },
    };
    let toc = match item("toc")? {
        None => None,
        Some(v) => {
            if let Ok(flag) = v.extract::<bool>() {
                flag.then(|| ("Table des matières".to_string(), 3))
            } else if let Ok(title) = v.extract::<String>() {
                Some((title, 3))
            } else {
                let d = v
                    .cast::<PyDict>()
                    .map_err(|_| PyValueError::new_err("'toc' takes a bool, a title or a dict(title=..., depth=...)"))?;
                let title = crate::kwarg::<String>(Some(d), "title")?.unwrap_or_else(|| "Table des matières".into());
                let depth = crate::kwarg::<u8>(Some(d), "depth")?.unwrap_or(3).clamp(1, 6);
                Some((title, depth))
            }
        }
    };
    let footer_default = Bar { center: "{page} / {pages}".into(), ..Default::default() };
    let options = Options {
        theme,
        geo: Geo { w, h, margin },
        title: string("title")?,
        author: string("author")?,
        cover,
        toc,
        bookmarks: crate::kwarg::<bool>(kwargs, "bookmarks")?.unwrap_or(true),
        header: bar_of(item("header")?, "header", None)?,
        footer: bar_of(item("footer")?, "footer", Some(footer_default))?,
        base_dir: string("base_dir")?.map(PathBuf::from),
    };

    let (bytes, _pages, _skipped) = py.detach(|| build(text, &options)).map_err(PyRuntimeError::new_err)?;
    match path {
        Some(path) => {
            std::fs::write(&path, bytes).map_err(|e| PyRuntimeError::new_err(format!("{path}: {e}")))?;
            Ok(None)
        }
        None => Ok(Some(PyBytes::new(py, &bytes))),
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(markdown_to_pdf, m)?)
}
