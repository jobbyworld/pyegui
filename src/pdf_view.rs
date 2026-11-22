//! PDF pages on screen: `PdfDocument` (an open file), `PdfView` / `pdf_view` (a viewer that scrolls
//! through the pages, zooms and fits them) and `pdf_thumbnail` (one small page, clickable).
//! The viewer itself is in `pyegui-core`; this file is its Python side.

use std::sync::{Arc, Mutex};

use pyegui_core::pdf_nav::Target;
use pyegui_core::pdf_view as core;
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::pdf::info_dict;
use crate::style::kwarg_color;
use crate::{current_ui, kwarg, UI};

/// An open PDF file.
///
/// The file is read at once (it is not kept open, so it can be moved or deleted afterwards on Windows)
/// and its pages are drawn on demand by `pdf_view` and `pdf_thumbnail`. A document is a state object:
/// create it once, outside the update function, and share it between the views and the thumbnails.
///
/// A big file takes a while to read: with `wait=False` the constructor returns at once, the file is
/// read by a thread and `loading` is True until it is open. The views and the thumbnails wait for it
/// by themselves, so the window does not freeze.
///
/// A file that cannot be opened does not raise: `error` tells why and `pages` is 0. A PDF that asks a
/// password has `needs_password` set: give it with `unlock` (or as `password`).
///
/// Properties (read only): `path`, `pages`, `loading`, `needs_password`, `error`, `info`, the dict of
/// `pdf_info` (title, author, dates, rights...) read from the open document, and `outline`, the
/// bookmarks of the document. `text(page)` reads the text of a page and `render_png` / `save_png`
/// draw one as a picture.
///
/// Args:
///     path (str): path of the PDF file
///
///     password (str): the password to open it with, none by default (a PDF that has only an owner
///         password opens without one)
///
///     wait (bool): False reads the file in a thread, True by default
///
/// Example::
///
///     doc = PdfDocument("contract.pdf")
///     if doc.needs_password and not doc.unlock("secret"):
///         print("wrong password")
///     print(doc.pages, doc.info["title"], doc.page_size(1))
///
///     big = PdfDocument("archive.pdf", wait=False)   # the window does not wait
///     view = PdfView(big)
#[pyclass]
pub(crate) struct PdfDocument {
    pub(crate) inner: Arc<Mutex<core::PdfDocument>>,
}

impl PdfDocument {
    fn lock(&self) -> std::sync::MutexGuard<'_, core::PdfDocument> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[pymethods]
impl PdfDocument {
    #[new]
    #[pyo3(signature = (path, password=None, wait=true))]
    fn new(py: Python<'_>, path: String, password: Option<String>, wait: bool) -> Self {
        PdfDocument { inner: Arc::new(Mutex::new(py.detach(|| core::PdfDocument::open(path, password, wait)))) }
    }

    /// Path of the file
    #[getter]
    fn path(&self) -> String {
        self.lock().path().to_owned()
    }

    /// True while a thread reads the file (`wait=False`)
    #[getter]
    fn loading(&self) -> bool {
        self.lock().loading()
    }

    /// Number of pages, 0 when the document is not open
    #[getter]
    fn pages(&self) -> usize {
        self.lock().pages()
    }

    /// True while the PDF asks a password to be opened
    #[getter]
    fn needs_password(&self) -> bool {
        self.lock().needs_password()
    }

    /// Why the file could not be read, None when it is open (or waits for a password)
    #[getter]
    fn error(&self) -> Option<String> {
        self.lock().error()
    }

    /// What the document says about itself, the dict of `pdf_info`
    #[getter]
    fn info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let info = self.lock().info();
        info_dict(py, &info)
    }

    /// Opens a PDF that asks a password. Returns True when it is open, False when the password is
    /// wrong (or when the document did not need one: then it is open already).
    fn unlock(&self, py: Python<'_>, password: &str) -> bool {
        let inner = self.inner.clone();
        py.detach(|| inner.lock().unwrap_or_else(|e| e.into_inner()).unlock(password))
    }

    /// Size of a page in points (1/72 inch), its rotation applied.
    ///
    /// Args:
    ///     page (int): the number of the page, from 1
    fn page_size(&self, page: usize) -> PyResult<(f32, f32)> {
        self.lock().page_size(page).ok_or_else(|| PyIndexError::new_err(format!("no page {page}")))
    }

    /// The text of a page, as it is read in the page (a newline where the line changes, a space
    /// between the words). It replaces pypdf's `extract_text`.
    ///
    /// Args:
    ///     page (int): the number of the page, from 1
    fn text(&self, py: Python<'_>, page: usize) -> PyResult<String> {
        let inner = self.inner.clone();
        py.detach(|| inner.lock().unwrap_or_else(|e| e.into_inner()).text(page))
            .ok_or_else(|| PyIndexError::new_err(format!("no page {page}")))
    }

    /// The bookmarks of the document: a list of dicts with `title`, `level` (0 for the top ones),
    /// `page` (from 1, None when the bookmark goes nowhere), `y` (points from the top of the page, None
    /// when the destination gives none) and `uri` (a web address, for a bookmark that opens one).
    /// Give `page` and `y` to `PdfView.go_to`.
    #[getter]
    fn outline<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyDict>>> {
        let inner = self.inner.clone();
        let marks = py.detach(|| inner.lock().unwrap_or_else(|e| e.into_inner()).outline());
        marks
            .into_iter()
            .map(|b| {
                let d = PyDict::new(py);
                d.set_item("title", b.title)?;
                d.set_item("level", b.level)?;
                let (page, y, uri) = match b.target {
                    Some(Target::Page { page, y }) => (Some(page + 1), y, None),
                    Some(Target::Uri(uri)) => (None, None, Some(uri)),
                    None => (None, None, None),
                };
                d.set_item("page", page)?;
                d.set_item("y", y)?;
                d.set_item("uri", uri)?;
                Ok(d)
            })
            .collect()
    }

    /// Draws a page and returns it as the bytes of a PNG picture.
    ///
    /// Args:
    ///     page (int): the number of the page, from 1
    ///
    /// Keyword args:
    ///     width (int): the width of the picture in pixels (its height follows)
    ///
    ///     scale (float): pixels per point when `width` is not given, 1.0 by default (72 dpi), 2.0
    ///         for 144 dpi
    ///
    ///     dark (bool): the dark colors, False by default
    #[pyo3(signature = (page, width=None, scale=1.0, dark=false))]
    fn render_png(&self, py: Python<'_>, page: usize, width: Option<u32>, scale: f32, dark: bool) -> PyResult<Vec<u8>> {
        let inner = self.inner.clone();
        py.detach(|| inner.lock().unwrap_or_else(|e| e.into_inner()).render_png(page, width, scale, dark))
            .ok_or_else(|| PyIndexError::new_err(format!("no page {page}")))?
            .map_err(PyValueError::new_err)
    }

    /// Draws a page and writes it as a PNG file.
    ///
    /// Args:
    ///     page (int): the number of the page, from 1
    ///
    ///     path (str): the file to write
    ///
    /// Keyword args: the same as `render_png`.
    #[pyo3(signature = (page, path, width=None, scale=1.0, dark=false))]
    fn save_png(&self, py: Python<'_>, page: usize, path: &str, width: Option<u32>, scale: f32, dark: bool) -> PyResult<()> {
        let bytes = self.render_png(py, page, width, scale, dark)?;
        std::fs::write(path, bytes).map_err(|e| pyo3::exceptions::PyOSError::new_err(format!("{path}: {}", e.kind())))
    }

    fn __len__(&self) -> usize {
        self.lock().pages()
    }

    fn __repr__(&self) -> String {
        let mut doc = self.lock();
        let pages = doc.pages();
        format!("PdfDocument({:?}, pages={})", doc.path, pages)
    }
}

/// The state of a PDF viewer: the document it shows, the page, the zoom, the scrolling, the
/// selection of text and the search.
///
/// Create it once, outside the update function, and draw it with `pdf_view`. In the continuous mode
/// (the default) the pages follow one another in a scroll area and `page` is the one at the top of
/// the window; in the single page mode only the current page is shown.
///
/// Properties: `doc` (a `PdfDocument` or None, write only), `page` (from 1, read it to show a page
/// number, write it to go to a page), `pages`, `zoom` (1.0 is 100%, 96 dpi; writing it turns `fit`
/// off), `fit` (`"width"`, `"page"` or `"none"`), `continuous`, `rotation` (0, 90, 180 or 270, the
/// look only), `dark` (draw the pages in dark colors), `tool` (`"select"` selects text with the
/// mouse, `"hand"` drags the pages), `busy` (pages are being drawn), `selected_text`, and `matches`,
/// `match_index`, `searching` for the search.
///
/// Args:
///     doc (PdfDocument): the document to show, none by default
///
///     fit (str): `"width"` (default) fits the widest page to the window, `"page"` fits a whole
///         page, `"none"` uses `zoom`
///
///     zoom (float): the zoom when `fit` is `"none"`, 1.0 by default
///
///     continuous (bool): scroll through all the pages (True, default) or show one page
///
///     page (int): the first page shown, 1 by default
///
/// Example::
///
///     doc = PdfDocument("contract.pdf")
///     view = PdfView(doc)
///     query = Str("")
///
///     def update_func(ctx):
///         if button_clicked("Fit the page"):
///             view.fit = "page"
///         if text_edit_singleline(query).changed:
///             view.search(query.value)
///         label(f"page {view.page} / {view.pages}, {view.matches} matches")
///         pdf_view(view)
#[pyclass]
pub(crate) struct PdfView {
    inner: core::PdfView,
}

#[pymethods]
impl PdfView {
    #[new]
    #[pyo3(signature = (doc=None, fit="width", zoom=1.0, continuous=true, page=1))]
    fn new(py: Python<'_>, doc: Option<Py<PdfDocument>>, fit: &str, zoom: f32, continuous: bool, page: usize) -> PyResult<Self> {
        let source = doc.map(|d| d.borrow(py).inner.clone());
        core::PdfView::new(source, fit, zoom, continuous, page).map(|inner| PdfView { inner }).map_err(PyValueError::new_err)
    }

    /// The document shown (write only): a `PdfDocument`, or None to show nothing. Giving the document
    /// that is shown already changes nothing.
    #[setter]
    fn set_doc(&mut self, py: Python<'_>, doc: Option<Py<PdfDocument>>) {
        self.inner.set_doc(doc.map(|d| d.borrow(py).inner.clone()));
    }

    /// Shows another document in place of the one shown, at the same page and zoom: for a file that
    /// was saved again, read again by a new `PdfDocument` (while it is read the view shows "Opening…").
    /// Giving a document with `doc = ...` instead starts at the first page.
    fn replace_doc(&mut self, py: Python<'_>, doc: Option<Py<PdfDocument>>) {
        self.inner.replace_doc(doc.map(|d| d.borrow(py).inner.clone()));
    }

    /// The page at the top of the window, from 1. Writing it goes to that page.
    #[getter]
    fn page(&self) -> usize {
        self.inner.page()
    }

    #[setter]
    fn set_page(&mut self, page: usize) {
        self.inner.set_page(page)
    }

    /// Goes to a page, from 1, and to a height in it: points from its top (0 shows the top of the
    /// page, a bookmark or a link of the document gives its own).
    #[pyo3(signature = (page, y=0.0))]
    fn go_to(&mut self, page: usize, y: f32) {
        self.inner.go_to(page, y)
    }

    /// Number of pages of the document shown, 0 without one
    #[getter]
    fn pages(&self) -> usize {
        self.inner.pages()
    }

    /// The zoom, 1.0 is 100% (96 dpi). The value that is used at the last frame when `fit` is on.
    /// Writing it sets `fit` to `"none"`.
    #[getter]
    fn zoom(&self) -> f32 {
        self.inner.zoom()
    }

    #[setter]
    fn set_zoom(&mut self, zoom: f32) {
        self.inner.set_zoom(zoom)
    }

    /// `"width"`, `"page"` or `"none"`
    #[getter]
    fn fit(&self) -> &'static str {
        self.inner.fit()
    }

    #[setter]
    fn set_fit(&mut self, fit: &str) -> PyResult<()> {
        self.inner.set_fit(fit).map_err(PyValueError::new_err)
    }

    /// True to scroll through all the pages, False to show the current page only (the wheel then goes
    /// to the next page past the end of the page, and to the previous one past its top)
    #[getter]
    fn continuous(&self) -> bool {
        self.inner.continuous()
    }

    #[setter]
    fn set_continuous(&mut self, continuous: bool) {
        self.inner.set_continuous(continuous)
    }

    /// How the pages are turned on the screen, in degrees clockwise: 0, 90, 180 or 270. It changes
    /// the look only, not the file.
    #[getter]
    fn rotation(&self) -> u16 {
        self.inner.rotation()
    }

    #[setter]
    fn set_rotation(&mut self, degrees: i32) {
        self.inner.set_rotation(degrees)
    }

    /// Turns the pages by a number of degrees (90 clockwise, -90 the other way)
    #[pyo3(signature = (degrees=90))]
    fn rotate(&mut self, degrees: i32) {
        self.inner.rotate(degrees)
    }

    /// True to draw the pages in dark colors (light text on a dark sheet; the images are kept)
    #[getter]
    fn dark(&self) -> bool {
        self.inner.dark()
    }

    #[setter]
    fn set_dark(&mut self, dark: bool) {
        self.inner.set_dark(dark)
    }

    /// `"select"` (default): a drag selects text, a double click a word, a triple click a line, Ctrl+C
    /// copies. `"hand"`: a drag moves the pages.
    #[getter]
    fn tool(&self) -> &'static str {
        self.inner.tool()
    }

    #[setter]
    fn set_tool(&mut self, tool: &str) -> PyResult<()> {
        self.inner.set_tool(tool).map_err(PyValueError::new_err)
    }

    /// The scroll offset of the last frame, in points from the top left of the content: `(x, y)`
    #[getter]
    fn scroll_offset(&self) -> (f32, f32) {
        self.inner.scroll_offset()
    }

    /// True while pages of the view are being drawn (their place shows a "…")
    #[getter]
    fn busy(&self) -> bool {
        self.inner.busy()
    }

    /// Goes to the next page (one page further, the last one stays)
    fn next_page(&mut self) {
        self.inner.next_page()
    }

    /// Goes to the previous page
    fn previous_page(&mut self) {
        self.inner.previous_page()
    }

    /// The text that is selected, from the pages whose text is read (the ones that were shown)
    #[getter]
    fn selected_text(&self) -> String {
        self.inner.selected_text()
    }

    /// Removes the selection
    fn clear_selection(&mut self) {
        self.inner.clear_selection()
    }

    /// Puts the selected text in the clipboard (at the next frame)
    fn copy_selection(&mut self) {
        self.inner.copy_selection()
    }

    /// Looks for a text in all the pages, in a thread. The matches are marked in the pages as they are
    /// found and the view goes to the first one; `matches` counts them and `next_match` and
    /// `previous_match` go from one to another. An empty text ends the search.
    ///
    /// Args:
    ///     text (str): the text to find, in a page it may continue on the next line
    ///
    ///     accents (bool): True tells "é" from "e" and "É" from "é", False (default) ignores the case
    ///         and the accents
    #[pyo3(signature = (text, accents=false))]
    fn search(&mut self, text: &str, accents: bool) {
        self.inner.search(text, accents)
    }

    /// Ends the search and removes its marks
    fn clear_search(&mut self) {
        self.inner.clear_search()
    }

    /// Number of matches found so far
    #[getter]
    fn matches(&self) -> usize {
        self.inner.matches()
    }

    /// The current match, from 1 (0 when there is none)
    #[getter]
    fn match_index(&self) -> usize {
        self.inner.match_index()
    }

    /// True while the search reads pages
    #[getter]
    fn searching(&self) -> bool {
        self.inner.searching()
    }

    /// Goes to the next match (the first one after the last)
    fn next_match(&mut self) {
        self.inner.next_match()
    }

    /// Goes to the previous match (the last one before the first)
    fn previous_match(&mut self) {
        self.inner.previous_match()
    }
}

/// Shows the pages of a PDF in a viewer that takes all the space available (or the height given).
/// The pages are drawn by threads: the window never waits for them, a page that is not ready shows
/// a "…" (or a blurry picture of itself while a sharper one is drawn).
///
/// The wheel and the scroll bars scroll, Ctrl + wheel (or a pinch) zooms around the pointer, and
/// with the pointer over the viewer PageUp / PageDown go a page (a window in the continuous mode)
/// further, Home and End to the first and the last page, Ctrl + and Ctrl - zoom, Ctrl 0 fits the
/// width. A drag selects text (see `PdfView.tool`), a double click a word, a triple click a line,
/// Ctrl+C copies it, Escape removes the selection. The links of the pages work: a click goes to the
/// page or opens the address in the browser. The matches of `PdfView.search` are marked.
///
/// Args:
///     view (PdfView): the state of the viewer
///
/// Keyword args:
///     height (float): the height, the available one by default
///
///     margin (float): the space around the pages, 12 by default
///
///     gap (float): the space between two pages, 12 by default
///
///     background (tuple | str): the color behind the pages, the one of the panel by default
///
/// Example::
///
///     view = PdfView(PdfDocument("contract.pdf"))
///
///     def update_func(ctx):
///         pdf_view(view, background="#808080")
#[pyfunction]
#[pyo3(signature = (view, **kwargs))]
unsafe fn pdf_view(view: &mut PdfView, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let mut options = core::PdfViewOptions::default();
    if let Some(margin) = kwarg::<f32>(kwargs, "margin")? {
        options.margin = margin;
    }
    if let Some(gap) = kwarg::<f32>(kwargs, "gap")? {
        options.gap = gap;
    }
    options.background = kwarg_color(kwargs, "background")?;
    options.height = kwarg::<f32>(kwargs, "height")?;
    core::pdf_view(ui, &mut view.inner, &options);
    Ok(())
}

/// Shows one page of a document as a small picture, and returns True when it is clicked. Draw
/// the thumbnails of the pages in a vertical scroll area next to a `pdf_view` and go to the page that
/// is clicked. The pictures are drawn by threads like the ones of the viewer, only for the thumbnails
/// that are on screen.
///
/// Args:
///     doc (PdfDocument): the document
///
///     page (int): the number of the page, from 1
///
/// Keyword args:
///     size (float): the longest side of the picture, 96 by default
///
///     selected (bool): frames the thumbnail as the page shown, False by default
///
///     label (bool): writes the number of the page under the picture, True by default
///
///     dark (bool): draws the page in dark colors, False by default
///
/// Example::
///
///     def sidebar():
///         def thumbnails():
///             for p in range(1, doc.pages + 1):
///                 if pdf_thumbnail(doc, p, selected=p == view.page):
///                     view.page = p
///         scroll_area_vertical(thumbnails)
#[pyfunction]
#[pyo3(signature = (doc, page, **kwargs))]
unsafe fn pdf_thumbnail(doc: PyRefMut<'_, PdfDocument>, page: usize, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<bool> {
    let ui = current_ui(&UI)?;
    let mut options = core::ThumbnailOptions::default();
    if let Some(size) = kwarg::<f32>(kwargs, "size")? {
        options.size = size;
    }
    options.selected = kwarg::<bool>(kwargs, "selected")?.unwrap_or(false);
    options.label = kwarg::<bool>(kwargs, "label")?.unwrap_or(true);
    options.dark = kwarg::<bool>(kwargs, "dark")?.unwrap_or(false);
    let mut document = doc.lock();
    core::pdf_thumbnail(ui, &mut document, page, &options).map_err(PyIndexError::new_err)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let _ = core::PAN_HOOK.set(|panning| {
        crate::cursor::hand(panning);
    });
    m.add_class::<PdfDocument>()?;
    m.add_class::<PdfView>()?;
    m.add_function(wrap_pyfunction!(pdf_view, m)?)?;
    m.add_function(wrap_pyfunction!(pdf_thumbnail, m)?)?;
    Ok(())
}
