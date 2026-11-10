use std::cell::RefCell;

use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkScrollOptions, CommonMarkViewer};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::{current_ui, kwarg, UI};

thread_local! {
    // keeps loaded images and highlighted code between frames
    static CACHE: RefCell<CommonMarkCache> = RefCell::new(CommonMarkCache::default());
}

/// Shows markdown text: headings, lists, tables, quotes, code blocks, links and images.
///
/// Keyword args:
///
///     scrollable (bool): show the text in a scroll area, drawing only the visible part. Faster for long texts
///
///     id (str): id of the scroll area, needed if there are several scrollable markdown texts
///
///     max_image_width (int): max width of the images
///
///     default_width (int): width of the text, default is the available width
///
/// Example::
///
///     markdown('''
///     # Title
///     Some **bold** and *italic* text, a [link](https://github.com/emilk/egui).
///
///     - item 1
///     - item 2
///
///     ```python
///     print("hello")
///     ```
///     ''')
#[pyfunction]
#[pyo3(signature = (text, **kwargs))]
unsafe fn markdown(text: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let mut viewer = CommonMarkViewer::new();

    if let Some(width) = kwarg::<usize>(kwargs, "max_image_width")? {
        viewer = viewer.max_image_width(Some(width));
    }
    if let Some(width) = kwarg::<usize>(kwargs, "default_width")? {
        viewer = viewer.default_width(Some(width));
    }

    let scrollable = kwarg::<bool>(kwargs, "scrollable")?.unwrap_or(false);
    let id = kwarg::<String>(kwargs, "id")?.unwrap_or_else(|| "markdown".to_string());

    CACHE.with_borrow_mut(|cache| {
        if scrollable {
            viewer.show_scrollable(
                egui::Id::unique(id),
                ui,
                cache,
                &CommonMarkScrollOptions::default(),
                text,
            );
        } else {
            viewer.show(ui, cache, text);
        }
    });
    Ok(())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(markdown, m)?)?;
    Ok(())
}
