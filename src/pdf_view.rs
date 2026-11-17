//! PDF pages on screen: `PdfDocument` (an open file), `PdfView` / `pdf_view` (a viewer that scrolls
//! through the pages, zooms and fits them) and `pdf_thumbnail` (one small page, clickable).
//!
//! The pages are drawn by `craft-pdf` (hayro) in threads, never in the GUI thread: what is on screen
//! is asked at each frame, the pages that are not ready show a placeholder (or a coarser picture of
//! the same page), and the thread that finishes one asks for a repaint. The pictures are kept as
//! textures in a cache with a memory budget, shared by all the views and the thumbnails.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use craft_pdf::interpret::InterpreterSettings;
use craft_pdf::raster::{self, Options, RenderCache};
use craft_pdf::syntax::object::ObjectIdentifier;
use craft_pdf::syntax::Pdf;
use eframe::egui::{self, Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::pdf::{info_dict, open_with_info, Info};
use crate::pdf_nav::{links as page_links, outline, page_ids, Bookmark, Target};
use crate::pdf_text::{extract_page, fold, PageText};
use crate::style::kwarg_color;
use crate::{current_ui, kwarg, UI};

/// Logical pixels of one point of the page at a zoom of 1 (100%): 96 pixels for 72 points.
const POINT: f32 = 96.0 / 72.0;
/// Longest side of a picture of a page, in pixels, and most pixels of one.
const MAX_SIDE: f32 = 8192.0;
const MAX_PIXELS: f32 = 40_000_000.0;
/// Memory kept in textures, in bytes. What the frame uses is never dropped.
const TEXTURE_BUDGET: usize = 384 * 1024 * 1024;
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;
/// Steps of the size of the pictures: a power of two per 8 steps, so a picture is at most 4.5% off.
const STEPS: f32 = 8.0;

// ---------------------------------------------------------------------------------------------
// the document

/// An open PDF, shared by the threads that draw its pages and read its text.
pub(crate) struct Doc {
    id: u64,
    pdf: Pdf,
    /// The size of every page in points, its rotation applied.
    sizes: Vec<(f32, f32)>,
    max_w: f32,
    max_h: f32,
    /// the page (from 0) of every page object, to find where a link goes
    ids: HashMap<ObjectIdentifier, usize>,
    /// the text of the pages read so far, and the pages that were asked for
    texts: Mutex<HashMap<usize, Arc<PageText>>>,
    asked: Mutex<HashSet<usize>>,
    outline: OnceLock<Vec<Bookmark>>,
}

const _: fn() = || {
    fn shared<T: Send + Sync>() {}
    shared::<Doc>();
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Characters of text kept for a document: past it the pages read are forgotten (and read again if needed).
const TEXT_BUDGET: usize = 8_000_000;

impl Doc {
    fn new(pdf: Pdf) -> Option<Doc> {
        let sizes = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf.pages()
                .iter()
                .map(|p| {
                    let (w, h) = p.render_dimensions();
                    if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 { (w, h) } else { (612.0, 792.0) }
                })
                .collect::<Vec<_>>()
        }))
        .ok()?;
        let max_w = sizes.iter().map(|s| s.0).fold(1.0, f32::max);
        let max_h = sizes.iter().map(|s| s.1).fold(1.0, f32::max);
        let ids = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| page_ids(&pdf))).unwrap_or_default();
        Some(Doc {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            pdf,
            sizes,
            max_w,
            max_h,
            ids,
            texts: Mutex::new(HashMap::new()),
            asked: Mutex::new(HashSet::new()),
            outline: OnceLock::new(),
        })
    }

    /// The text of a page (from 0) with its links, read now (and kept).
    fn page_text(&self, page: usize) -> Arc<PageText> {
        if let Some(text) = self.cached_text(page) {
            return text;
        }
        let mut text = extract_page(&self.pdf, page);
        text.links = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| page_links(&self.pdf, &self.ids, page))).unwrap_or_default();
        let text = Arc::new(text);
        let mut texts = self.texts.lock().unwrap_or_else(|e| e.into_inner());
        if texts.values().map(|t| t.chars.len()).sum::<usize>() > TEXT_BUDGET {
            texts.clear();
            self.asked.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
        texts.insert(page, Arc::clone(&text));
        text
    }

    /// The text of a page when it is read already.
    fn cached_text(&self, page: usize) -> Option<Arc<PageText>> {
        self.texts.lock().unwrap_or_else(|e| e.into_inner()).get(&page).cloned()
    }

    /// Asks the text thread to read the text of a page, once.
    fn ask_text(self: &Arc<Self>, ctx: &egui::Context, page: usize) {
        if self.cached_text(page).is_some() || !self.asked.lock().unwrap_or_else(|e| e.into_inner()).insert(page) {
            return;
        }
        let _ = text_sender(ctx).send((Arc::clone(self), page));
    }

    fn outline(&self) -> &[Bookmark] {
        self.outline.get_or_init(|| std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| outline(&self.pdf, &self.ids))).unwrap_or_default())
    }
}

/// What a load gives: what the file says about itself, the document when it opened, the bytes when it
/// waits for a password.
struct Loaded {
    info: Info,
    doc: Option<Arc<Doc>>,
    bytes: Option<Arc<Vec<u8>>>,
}

fn load(bytes: Arc<Vec<u8>>, password: Option<&str>) -> Loaded {
    let (mut info, pdf) = open_with_info(Arc::clone(&bytes), password);
    let mut doc = None;
    let mut waiting = None;
    if let Some(pdf) = pdf {
        match Doc::new(pdf) {
            Some(d) => doc = Some(Arc::new(d)),
            None => info.error = Some("the PDF reader failed on this file".into()),
        }
    } else if info.needs_password {
        waiting = Some(bytes);
    }
    Loaded { info, doc, bytes: waiting }
}

fn read_and_load(path: &str, password: Option<&str>) -> Loaded {
    match std::fs::read(path) {
        Ok(bytes) => load(Arc::new(bytes), password),
        // the text of the OS depends on the language and the code page: the kind says it all
        Err(e) => Loaded { info: Info { error: Some(format!("cannot read the file ({:?})", e.kind())), ..Info::default() }, doc: None, bytes: None },
    }
}

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
    doc: Option<Arc<Doc>>,
    path: String,
    /// the file while it waits for a password
    bytes: Option<Arc<Vec<u8>>>,
    info: Info,
    /// where the thread that reads the file puts the result
    loading: Option<Arc<Mutex<Option<Loaded>>>>,
}

impl PdfDocument {
    fn apply(&mut self, loaded: Loaded) {
        self.info = loaded.info;
        self.doc = loaded.doc;
        self.bytes = loaded.bytes;
    }

    /// Takes what the loading thread has finished. Returns True while the file is still read.
    fn poll(&mut self) -> bool {
        let Some(slot) = &self.loading else { return false };
        let done = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
        match done {
            Some(loaded) => {
                self.apply(loaded);
                self.loading = None;
                false
            }
            None => true,
        }
    }
}

#[pymethods]
impl PdfDocument {
    #[new]
    #[pyo3(signature = (path, password=None, wait=true))]
    fn new(py: Python<'_>, path: String, password: Option<String>, wait: bool) -> Self {
        let mut document = PdfDocument { doc: None, path: path.clone(), bytes: None, info: Info::default(), loading: None };
        if wait {
            let loaded = py.detach(|| read_and_load(&path, password.as_deref()));
            document.apply(loaded);
        } else {
            let slot = Arc::new(Mutex::new(None));
            let out = Arc::clone(&slot);
            let spawned = std::thread::Builder::new().name("pdf-open".into()).spawn(move || {
                let loaded = read_and_load(&path, password.as_deref());
                *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(loaded);
            });
            match spawned {
                Ok(_) => document.loading = Some(slot),
                Err(e) => document.info.error = Some(format!("cannot start the reading thread ({:?})", e.kind())),
            }
        }
        document
    }

    /// Path of the file
    #[getter]
    fn path(&self) -> &str {
        &self.path
    }

    /// True while a thread reads the file (`wait=False`)
    #[getter]
    fn loading(&mut self) -> bool {
        self.poll()
    }

    /// Number of pages, 0 when the document is not open
    #[getter]
    fn pages(&mut self) -> usize {
        self.poll();
        self.doc.as_ref().map_or(0, |d| d.sizes.len())
    }

    /// True while the PDF asks a password to be opened
    #[getter]
    fn needs_password(&mut self) -> bool {
        self.poll();
        self.info.needs_password
    }

    /// Why the file could not be read, None when it is open (or waits for a password)
    #[getter]
    fn error(&mut self) -> Option<String> {
        self.poll();
        self.info.error.clone()
    }

    /// What the document says about itself, the dict of `pdf_info`
    #[getter]
    fn info<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        self.poll();
        info_dict(py, &self.info)
    }

    /// Opens a PDF that asks a password. Returns True when it is open, False when the password is
    /// wrong (or when the document did not need one: then it is open already).
    fn unlock(&mut self, py: Python<'_>, password: &str) -> bool {
        self.poll();
        if self.doc.is_some() {
            return true;
        }
        let Some(bytes) = self.bytes.clone() else { return false };
        let loaded = py.detach(|| load(bytes, Some(password)));
        self.apply(loaded);
        self.doc.is_some()
    }

    /// Size of a page in points (1/72 inch), its rotation applied.
    ///
    /// Args:
    ///     page (int): the number of the page, from 1
    fn page_size(&mut self, page: usize) -> PyResult<(f32, f32)> {
        self.poll();
        self.doc
            .as_ref()
            .and_then(|d| page.checked_sub(1).and_then(|i| d.sizes.get(i).copied()))
            .ok_or_else(|| PyIndexError::new_err(format!("no page {page}")))
    }

    /// The text of a page, as it is read in the page (a newline where the line changes, a space
    /// between the words). It replaces pypdf's `extract_text`.
    ///
    /// Args:
    ///     page (int): the number of the page, from 1
    fn text(&mut self, py: Python<'_>, page: usize) -> PyResult<String> {
        self.poll();
        let doc = self.doc.clone().ok_or_else(|| PyIndexError::new_err(format!("no page {page}")))?;
        let index = page.checked_sub(1).filter(|i| *i < doc.sizes.len()).ok_or_else(|| PyIndexError::new_err(format!("no page {page}")))?;
        Ok(py.detach(|| doc.page_text(index).text()))
    }

    /// The bookmarks of the document: a list of dicts with `title`, `level` (0 for the top ones),
    /// `page` (from 1, None when the bookmark goes nowhere), `y` (points from the top of the page, None
    /// when the destination gives none) and `uri` (a web address, for a bookmark that opens one).
    /// Give `page` and `y` to `PdfView.go_to`.
    #[getter]
    fn outline<'py>(&mut self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyDict>>> {
        self.poll();
        let Some(doc) = self.doc.clone() else { return Ok(Vec::new()) };
        let marks = py.detach(|| doc.outline().to_vec());
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
    fn render_png(&mut self, py: Python<'_>, page: usize, width: Option<u32>, scale: f32, dark: bool) -> PyResult<Vec<u8>> {
        self.poll();
        let doc = self.doc.clone().ok_or_else(|| PyIndexError::new_err(format!("no page {page}")))?;
        let index = page.checked_sub(1).filter(|i| *i < doc.sizes.len()).ok_or_else(|| PyIndexError::new_err(format!("no page {page}")))?;
        py.detach(|| render_png(&doc, index, width, scale, dark)).map_err(PyValueError::new_err)
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
    fn save_png(&mut self, py: Python<'_>, page: usize, path: &str, width: Option<u32>, scale: f32, dark: bool) -> PyResult<()> {
        let bytes = self.render_png(py, page, width, scale, dark)?;
        std::fs::write(path, bytes).map_err(|e| pyo3::exceptions::PyOSError::new_err(format!("{path}: {}", e.kind())))
    }

    fn __len__(&mut self) -> usize {
        self.pages()
    }

    fn __repr__(&mut self) -> String {
        let pages = self.pages();
        format!("PdfDocument({:?}, pages={})", self.path, pages)
    }
}

// ---------------------------------------------------------------------------------------------
// the pictures of the pages, drawn in threads

/// A picture asked for: the document, the page (from 0), the size step and the dark colors.
type PageKey = (u64, usize, i32, bool);

/// The pixel size of a page at a size step, inside the caps.
fn picture_size(size: (f32, f32), step: i32) -> (u32, u32) {
    let (pw, ph) = size;
    let mut q = (step as f32 / STEPS).exp2();
    let (mut w, mut h) = (pw * q, ph * q);
    let side = w.max(h);
    if side > MAX_SIDE {
        q *= MAX_SIDE / side;
        (w, h) = (pw * q, ph * q);
    }
    if w * h > MAX_PIXELS {
        let k = (MAX_PIXELS / (w * h)).sqrt();
        (w, h) = (w * k, h * k);
    }
    ((w.round() as u32).max(1), (h.round() as u32).max(1))
}

/// The size step of a page drawn at `scale` physical pixels per point.
fn step_for(scale: f32) -> i32 {
    (scale.max(0.01).log2() * STEPS).round() as i32
}

/// Draws a page into `width` x `height` pixels, on white, or in the colors of a dark theme.
fn draw_page(doc: &Doc, page: usize, width: u32, height: u32, dark: bool, cancel: Option<Arc<AtomicBool>>) -> Result<raster::Raster, raster::Error> {
    let pages = doc.pdf.pages();
    let page = pages.get(page).ok_or(raster::Error::InvalidPage)?;
    let (pw, ph) = page.render_dimensions();
    if !(pw.is_finite() && ph.is_finite() && pw > 0.0 && ph > 0.0) {
        return Err(raster::Error::InvalidPage);
    }
    let options = Options {
        scale: [width as f32 / pw, height as f32 / ph],
        size: Some([width, height]),
        offset: [0.0, 0.0],
        background: if dark { [30, 30, 30, 255] } else { [255, 255, 255, 255] },
        color_replacement: dark.then_some(raster::ColorReplacement { text: [222, 222, 222], background: [30, 30, 30], only_black: false, line_art: true }),
    };
    let mut settings = InterpreterSettings::default();
    settings.cancelled = cancel;
    raster::render(page, &RenderCache::new(), &settings, &options)
}

struct Job {
    key: PageKey,
    doc: Arc<Doc>,
    cancel: Arc<AtomicBool>,
    /// the smaller, the sooner
    priority: f32,
}

enum Outcome {
    Picture { width: u32, height: u32, rgba: Vec<u8> },
    Failed(&'static str),
    Cancelled,
}

struct Done {
    key: PageKey,
    outcome: Outcome,
}

struct Shared {
    queue: Mutex<Vec<Job>>,
    wake: Condvar,
    done: Mutex<Vec<Done>>,
}

fn render(job: &Job) -> Outcome {
    let (width, height) = picture_size(job.doc.sizes[job.key.1], job.key.2);
    let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| draw_page(&job.doc, job.key.1, width, height, job.key.3, Some(Arc::clone(&job.cancel)))));
    match drawn {
        Ok(Ok(r)) => Outcome::Picture { width: r.width, height: r.height, rgba: r.bytes().to_vec() },
        Ok(Err(raster::Error::Cancelled)) => Outcome::Cancelled,
        Ok(Err(raster::Error::OutputLimit | raster::Error::Allocation)) => Outcome::Failed("the page is too big to draw"),
        Ok(Err(_)) => Outcome::Failed("the page cannot be drawn"),
        Err(_) => Outcome::Failed("the page drawer failed on this page"),
    }
}

fn worker(shared: Arc<Shared>, ctx: egui::Context) {
    loop {
        let job = {
            let mut queue = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                queue.retain(|j| !j.cancel.load(Ordering::Relaxed));
                let best = queue.iter().enumerate().min_by(|a, b| a.1.priority.total_cmp(&b.1.priority)).map(|(i, _)| i);
                if let Some(i) = best {
                    break queue.swap_remove(i);
                }
                queue = shared.wake.wait(queue).unwrap_or_else(|e| e.into_inner());
            }
        };
        let outcome = render(&job);
        shared.done.lock().unwrap_or_else(|e| e.into_inner()).push(Done { key: job.key, outcome });
        ctx.request_repaint();
    }
}

fn shared(ctx: &egui::Context) -> &'static Arc<Shared> {
    static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
    SHARED.get_or_init(|| {
        let shared = Arc::new(Shared { queue: Mutex::new(Vec::new()), wake: Condvar::new(), done: Mutex::new(Vec::new()) });
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get() / 2).clamp(1, 4);
        for n in 0..threads {
            let (shared, ctx) = (Arc::clone(&shared), ctx.clone());
            let _ = std::thread::Builder::new().name(format!("pdf-page-{n}")).spawn(move || worker(shared, ctx));
        }
        shared
    })
}

/// A page as the bytes of a PNG: `width` pixels wide, or `scale` pixels per point.
fn render_png(doc: &Doc, page: usize, width: Option<u32>, scale: f32, dark: bool) -> Result<Vec<u8>, String> {
    let (pw, ph) = doc.sizes[page];
    let k = match width {
        Some(w) if w > 0 => w as f32 / pw,
        Some(_) => return Err("width must be positive".into()),
        None if scale.is_finite() && scale > 0.0 => scale,
        None => return Err("scale must be positive".into()),
    };
    let (w, h) = ((pw * k).round().max(1.0), (ph * k).round().max(1.0));
    if w.max(h) > MAX_SIDE || w * h > MAX_PIXELS {
        return Err(format!("the picture would be {w} x {h} pixels, the limit is {MAX_SIDE} on a side"));
    }
    let raster = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| draw_page(doc, page, w as u32, h as u32, dark, None)))
        .map_err(|_| "the page drawer failed on this page".to_owned())?
        .map_err(|e| format!("the page cannot be drawn: {e}"))?;
    let image = image::RgbaImage::from_raw(raster.width, raster.height, raster.bytes().to_vec()).ok_or("the picture has a wrong size")?;
    let mut out = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// the text of the pages and the search, in threads

/// The thread that reads the text of the pages the GUI asks for, one after the other.
fn text_sender(ctx: &egui::Context) -> &'static std::sync::mpsc::Sender<(Arc<Doc>, usize)> {
    static TX: OnceLock<std::sync::mpsc::Sender<(Arc<Doc>, usize)>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(Arc<Doc>, usize)>();
        let ctx = ctx.clone();
        let _ = std::thread::Builder::new().name("pdf-text".into()).spawn(move || {
            for (doc, page) in rx {
                doc.page_text(page);
                ctx.request_repaint();
            }
        });
        tx
    })
}

/// The matches of a search, filled by its thread.
#[derive(Default)]
struct Hits {
    by_page: HashMap<usize, Vec<(usize, usize)>>,
    /// every match in the order of the document: page, start, end
    order: Vec<(usize, usize, usize)>,
    scanned: usize,
}

pub(crate) struct Search {
    hits: Mutex<Hits>,
    done: AtomicBool,
    cancel: AtomicBool,
}

impl Search {
    fn total(&self) -> usize {
        self.hits.lock().unwrap_or_else(|e| e.into_inner()).order.len()
    }
}

/// Looks for `query` in the pages of a document, in a thread, from the first page.
fn start_search(doc: &Arc<Doc>, query: &str, accents: bool, ctx: &egui::Context) -> Arc<Search> {
    let search = Arc::new(Search { hits: Mutex::new(Hits::default()), done: AtomicBool::new(false), cancel: AtomicBool::new(false) });
    let (doc, out, ctx) = (Arc::clone(doc), Arc::clone(&search), ctx.clone());
    let query: Vec<char> = if accents { query.trim().chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect() } else { query.trim().chars().map(fold).collect() };
    let spawned = std::thread::Builder::new().name("pdf-search".into()).spawn(move || {
        for page in 0..doc.sizes.len() {
            if out.cancel.load(Ordering::Relaxed) {
                break;
            }
            let found = doc.page_text(page).find(&query, accents);
            let mut hits = out.hits.lock().unwrap_or_else(|e| e.into_inner());
            hits.scanned = page + 1;
            if !found.is_empty() {
                hits.order.extend(found.iter().map(|&(s, e)| (page, s, e)));
                hits.by_page.insert(page, found);
                ctx.request_repaint();
            }
        }
        out.done.store(true, Ordering::Relaxed);
        ctx.request_repaint();
    });
    if spawned.is_err() {
        search.done.store(true, Ordering::Relaxed);
    }
    search
}

// ---------------------------------------------------------------------------------------------
// the textures of the GUI thread

struct Entry {
    texture: egui::TextureHandle,
    bytes: usize,
    used: u64,
}

#[derive(Default)]
struct Cache {
    frame: u64,
    textures: HashMap<PageKey, Entry>,
    pending: HashMap<PageKey, Arc<AtomicBool>>,
    failed: HashMap<PageKey, &'static str>,
    /// what the frame that is drawn asks for
    wanted: HashSet<PageKey>,
    bytes: usize,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

impl Cache {
    /// Called by every widget before it draws: at the first call of a frame, drops the jobs that
    /// the last frame did not ask for again and takes what the threads finished.
    fn begin(&mut self, ctx: &egui::Context) {
        let frame = ctx.cumulative_frame_nr();
        if frame != self.frame {
            self.frame = frame;
            let wanted = std::mem::take(&mut self.wanted);
            self.pending.retain(|key, cancel| {
                let keep = wanted.contains(key);
                if !keep {
                    cancel.store(true, Ordering::Relaxed);
                }
                keep
            });
        }
        let done = std::mem::take(&mut *shared(ctx).done.lock().unwrap_or_else(|e| e.into_inner()));
        for Done { key, outcome } in done {
            let cancelled = self.pending.remove(&key).is_none();
            match outcome {
                // a picture nobody asks for any more is still worth keeping for a moment
                Outcome::Picture { width, height, rgba } => {
                    let image = egui::ColorImage::from_rgba_premultiplied([width as usize, height as usize], &rgba);
                    let name = format!("pdf-{}-{}-{}-{}", key.0, key.1, key.2, u8::from(key.3));
                    let texture = ctx.load_texture(name, image, egui::TextureOptions::LINEAR);
                    self.bytes += rgba.len();
                    if let Some(old) = self.textures.insert(key, Entry { texture, bytes: rgba.len(), used: self.frame }) {
                        self.bytes -= old.bytes;
                    }
                }
                Outcome::Failed(why) if !cancelled => {
                    self.failed.insert(key, why);
                }
                Outcome::Failed(_) | Outcome::Cancelled => {}
            }
        }
        while self.bytes > TEXTURE_BUDGET {
            let oldest = self.textures.iter().filter(|(_, e)| e.used < self.frame).min_by_key(|(_, e)| e.used).map(|(k, _)| *k);
            let Some(key) = oldest else { break };
            if let Some(e) = self.textures.remove(&key) {
                self.bytes -= e.bytes;
            }
        }
    }

    /// Asks for the picture of a page at a size step; returns the texture to draw now: the picture
    /// asked for, or the closest one of the same page, or none.
    fn want(&mut self, ctx: &egui::Context, doc: &Arc<Doc>, page: usize, step: i32, dark: bool, priority: f32) -> Option<egui::TextureId> {
        let key = (doc.id, page, step, dark);
        self.wanted.insert(key);
        if let Some(e) = self.textures.get_mut(&key) {
            e.used = self.frame;
            return Some(e.texture.id());
        }
        if !self.pending.contains_key(&key) && !self.failed.contains_key(&key) {
            let cancel = Arc::new(AtomicBool::new(false));
            self.pending.insert(key, Arc::clone(&cancel));
            let shared = shared(ctx);
            shared.queue.lock().unwrap_or_else(|e| e.into_inner()).push(Job { key, doc: Arc::clone(doc), cancel, priority });
            shared.wake.notify_one();
        }
        let frame = self.frame;
        let best = self
            .textures
            .iter_mut()
            .filter(|(k, _)| k.0 == doc.id && k.1 == page && k.3 == dark)
            .min_by_key(|(k, _)| (k.2 - step).abs())?;
        best.1.used = frame;
        Some(best.1.texture.id())
    }

    fn failed(&self, doc: &Doc, page: usize, step: i32, dark: bool) -> Option<&'static str> {
        self.failed.get(&(doc.id, page, step, dark)).copied()
    }
}

/// Where a page is on the screen: the rectangle it fills, its size in points as it is stored, and the
/// rotation (0, 90, 180 or 270 degrees clockwise) the view gives it.
#[derive(Clone, Copy, Debug)]
struct Geo {
    rect: Rect,
    w: f32,
    h: f32,
    rot: u16,
}

impl Geo {
    /// Screen pixels per point.
    fn scale(&self) -> f32 {
        self.rect.width() / if self.rot % 180 == 0 { self.w } else { self.h }
    }

    /// A point of the page (points, top left origin, not rotated) on the screen.
    fn to_screen(&self, p: Pos2) -> Pos2 {
        let s = self.scale();
        let d = match self.rot {
            0 => Vec2::new(p.x, p.y),
            90 => Vec2::new(self.h - p.y, p.x),
            180 => Vec2::new(self.w - p.x, self.h - p.y),
            _ => Vec2::new(p.y, self.w - p.x),
        };
        self.rect.min + d * s
    }

    /// The point of the page under a point of the screen.
    fn from_screen(&self, q: Pos2) -> Pos2 {
        let d = (q - self.rect.min) / self.scale();
        match self.rot {
            0 => Pos2::new(d.x, d.y),
            90 => Pos2::new(d.y, self.h - d.x),
            180 => Pos2::new(self.w - d.x, self.h - d.y),
            _ => Pos2::new(self.w - d.y, d.x),
        }
    }

    fn rect_to_screen(&self, r: Rect) -> Rect {
        Rect::from_two_pos(self.to_screen(r.min), self.to_screen(r.max))
    }
}

/// Draws a page: its picture (rotated), or a blank sheet with a sign while the picture is drawn.
fn paint_page(ui: &egui::Ui, doc: &Arc<Doc>, page: usize, geo: Geo, dark: bool, priority: f32, draw: bool) {
    let ctx = ui.ctx();
    let step = step_for(geo.scale() * ctx.pixels_per_point());
    let (texture, failed) = CACHE.with_borrow_mut(|c| (c.want(ctx, doc, page, step, dark, priority), c.failed(doc, page, step, dark)));
    if !draw {
        return;
    }
    let rect = geo.rect;
    let painter = ui.painter();
    painter.rect_filled(rect.translate(egui::vec2(1.5, 2.5)).expand(1.0), 2.0, Color32::from_black_alpha(45));
    painter.rect_filled(rect, 0.0, if dark { Color32::from_gray(30) } else { Color32::WHITE });
    let sign = if dark { Color32::from_gray(140) } else { Color32::from_gray(160) };
    match (texture, failed) {
        (Some(id), _) => {
            let (w, h) = (geo.w, geo.h);
            let mut mesh = egui::Mesh::with_texture(id);
            for (corner, uv) in [(Pos2::new(0.0, 0.0), Pos2::new(0.0, 0.0)), (Pos2::new(w, 0.0), Pos2::new(1.0, 0.0)), (Pos2::new(w, h), Pos2::new(1.0, 1.0)), (Pos2::new(0.0, h), Pos2::new(0.0, 1.0))] {
                mesh.vertices.push(egui::epaint::Vertex { pos: geo.to_screen(corner), uv, color: Color32::WHITE });
            }
            mesh.indices.extend([0, 1, 2, 0, 2, 3]);
            painter.add(egui::Shape::mesh(mesh));
        }
        (None, Some(why)) => {
            painter.text(rect.center(), Align2::CENTER_CENTER, format!("⚠ {why}"), FontId::proportional(14.0), sign);
        }
        (None, None) => {
            painter.text(rect.center(), Align2::CENTER_CENTER, "…", FontId::proportional(20.0), sign);
        }
    }
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_black_alpha(60)), egui::StrokeKind::Outside);
}

// ---------------------------------------------------------------------------------------------
// the viewer

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fit {
    Width,
    Page,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Hand,
}

/// A boundary between two characters of the document: the page (from 0) and the place in its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct At {
    page: usize,
    at: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Selection {
    anchor: At,
    head: At,
}

impl Selection {
    fn ordered(&self) -> (At, At) {
        if self.anchor <= self.head { (self.anchor, self.head) } else { (self.head, self.anchor) }
    }

    fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// The text of a selection, from the pages whose text is read.
fn selection_text(doc: &Doc, sel: &Selection) -> String {
    let (lo, hi) = sel.ordered();
    let mut parts = Vec::new();
    for page in lo.page..=hi.page.min(doc.sizes.len().saturating_sub(1)) {
        let Some(text) = doc.cached_text(page) else { continue };
        let from = if page == lo.page { lo.at } else { 0 };
        let to = if page == hi.page { hi.at } else { text.chars.len() };
        parts.push(text.string(from, to));
    }
    parts.join("\n")
}

static NEXT_VIEW: AtomicU64 = AtomicU64::new(1);

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
    id: u64,
    /// the document given, which may still be read by its thread
    source: Option<Py<PdfDocument>>,
    /// the document of the last frame, once open
    doc: Option<Arc<Doc>>,
    current: usize,
    /// a page (from 0) and a height in it (points from its top) to go to
    goto: Option<(usize, f32)>,
    zoom: f32,
    fit: Fit,
    continuous: bool,
    rotation: u16,
    dark: bool,
    tool: Tool,
    /// the scroll offset and the viewport of the last frame
    offset: Vec2,
    viewport: Rect,
    last_zoom: f32,
    set_offset: Option<Vec2>,
    busy: bool,
    sel: Option<Selection>,
    copy_request: bool,
    search: Option<Arc<Search>>,
    /// a search asked, started at the next frame (it needs the context and the open document)
    pending_search: Option<(String, bool)>,
    /// the match that is current, from 0, and whether it is to be shown
    hit: usize,
    reveal: bool,
    reveal_first: bool,
}

fn parse_fit(fit: &str) -> PyResult<Fit> {
    match fit {
        "width" => Ok(Fit::Width),
        "page" => Ok(Fit::Page),
        "none" => Ok(Fit::None),
        other => Err(PyValueError::new_err(format!("fit is \"width\", \"page\" or \"none\", not {other:?}"))),
    }
}

#[pymethods]
impl PdfView {
    #[new]
    #[pyo3(signature = (doc=None, fit="width", zoom=1.0, continuous=true, page=1))]
    fn new(doc: Option<Py<PdfDocument>>, fit: &str, zoom: f32, continuous: bool, page: usize) -> PyResult<Self> {
        let page = page.max(1) - 1;
        Ok(PdfView {
            id: NEXT_VIEW.fetch_add(1, Ordering::Relaxed),
            source: doc,
            doc: None,
            current: page,
            goto: Some((page, 0.0)),
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            fit: parse_fit(fit)?,
            continuous,
            rotation: 0,
            dark: false,
            tool: Tool::Select,
            offset: Vec2::ZERO,
            viewport: Rect::NOTHING,
            last_zoom: 0.0,
            set_offset: None,
            busy: false,
            sel: None,
            copy_request: false,
            search: None,
            pending_search: None,
            hit: 0,
            reveal: false,
            reveal_first: false,
        })
    }

    /// The document shown (write only): a `PdfDocument`, or None to show nothing. Giving the document
    /// that is shown already changes nothing.
    #[setter]
    fn set_doc(&mut self, doc: Option<Py<PdfDocument>>) {
        self.source = doc;
    }

    /// The page at the top of the window, from 1. Writing it goes to that page.
    #[getter]
    fn page(&self) -> usize {
        self.current + 1
    }

    #[setter]
    fn set_page(&mut self, page: usize) {
        self.go_to(page, 0.0);
    }

    /// Goes to a page, from 1, and to a height in it: points from its top (0 shows the top of the
    /// page, a bookmark or a link of the document gives its own).
    #[pyo3(signature = (page, y=0.0))]
    fn go_to(&mut self, page: usize, y: f32) {
        // without the document (it may be read still) the page is kept as it is: the next frame limits it
        let last = self.doc.as_ref().map_or(usize::MAX, |d| d.sizes.len().max(1) - 1);
        let page = (page.max(1) - 1).min(last);
        self.goto = Some((page, y.max(0.0)));
        self.current = page;
    }

    /// Number of pages of the document shown, 0 without one
    #[getter]
    fn pages(&self) -> usize {
        self.doc.as_ref().map_or(0, |d| d.sizes.len())
    }

    /// The zoom, 1.0 is 100% (96 dpi). The value that is used at the last frame when `fit` is on.
    /// Writing it sets `fit` to `"none"`.
    #[getter]
    fn zoom(&self) -> f32 {
        self.zoom
    }

    #[setter]
    fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.fit = Fit::None;
    }

    /// `"width"`, `"page"` or `"none"`
    #[getter]
    fn fit(&self) -> &'static str {
        match self.fit {
            Fit::Width => "width",
            Fit::Page => "page",
            Fit::None => "none",
        }
    }

    #[setter]
    fn set_fit(&mut self, fit: &str) -> PyResult<()> {
        self.fit = parse_fit(fit)?;
        Ok(())
    }

    /// True to scroll through all the pages, False to show the current page only
    #[getter]
    fn continuous(&self) -> bool {
        self.continuous
    }

    #[setter]
    fn set_continuous(&mut self, continuous: bool) {
        if self.continuous != continuous {
            self.continuous = continuous;
            self.goto = Some((self.current, 0.0));
            self.offset = Vec2::ZERO;
        }
    }

    /// How the pages are turned on the screen, in degrees clockwise: 0, 90, 180 or 270. It changes
    /// the look only, not the file.
    #[getter]
    fn rotation(&self) -> u16 {
        self.rotation
    }

    #[setter]
    fn set_rotation(&mut self, degrees: i32) {
        let turned = (degrees.rem_euclid(360) / 90) * 90;
        if turned as u16 != self.rotation {
            self.rotation = turned as u16;
            self.goto = Some((self.current, 0.0));
        }
    }

    /// Turns the pages by a number of degrees (90 clockwise, -90 the other way)
    #[pyo3(signature = (degrees=90))]
    fn rotate(&mut self, degrees: i32) {
        self.set_rotation(i32::from(self.rotation) + degrees);
    }

    /// True to draw the pages in dark colors (light text on a dark sheet; the images are kept)
    #[getter]
    fn dark(&self) -> bool {
        self.dark
    }

    #[setter]
    fn set_dark(&mut self, dark: bool) {
        self.dark = dark;
    }

    /// `"select"` (default): a drag selects text, a double click a word, a triple click a line, Ctrl+C
    /// copies. `"hand"`: a drag moves the pages.
    #[getter]
    fn tool(&self) -> &'static str {
        match self.tool {
            Tool::Select => "select",
            Tool::Hand => "hand",
        }
    }

    #[setter]
    fn set_tool(&mut self, tool: &str) -> PyResult<()> {
        self.tool = match tool {
            "select" => Tool::Select,
            "hand" => Tool::Hand,
            other => return Err(PyValueError::new_err(format!("tool is \"select\" or \"hand\", not {other:?}"))),
        };
        Ok(())
    }

    /// True while pages of the view are being drawn (their place shows a "…")
    #[getter]
    fn busy(&self) -> bool {
        self.busy
    }

    /// Goes to the next page (one page further, the last one stays)
    fn next_page(&mut self) {
        self.set_page(self.current + 2);
    }

    /// Goes to the previous page
    fn previous_page(&mut self) {
        self.set_page(self.current);
    }

    /// The text that is selected, from the pages whose text is read (the ones that were shown)
    #[getter]
    fn selected_text(&self) -> String {
        match (&self.doc, &self.sel) {
            (Some(doc), Some(sel)) if !sel.is_empty() => selection_text(doc, sel),
            _ => String::new(),
        }
    }

    /// Removes the selection
    fn clear_selection(&mut self) {
        self.sel = None;
    }

    /// Puts the selected text in the clipboard (at the next frame)
    fn copy_selection(&mut self) {
        self.copy_request = true;
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
        if let Some(old) = self.search.take() {
            old.cancel.store(true, Ordering::Relaxed);
        }
        self.hit = 0;
        self.reveal = false;
        self.pending_search = (!text.trim().is_empty()).then(|| (text.to_owned(), accents));
    }

    /// Ends the search and removes its marks
    fn clear_search(&mut self) {
        self.search("", false);
    }

    /// Number of matches found so far
    #[getter]
    fn matches(&self) -> usize {
        self.search.as_ref().map_or(0, |s| s.total())
    }

    /// The current match, from 1 (0 when there is none)
    #[getter]
    fn match_index(&self) -> usize {
        match &self.search {
            Some(s) if s.total() > 0 => self.hit + 1,
            _ => 0,
        }
    }

    /// True while the search reads pages
    #[getter]
    fn searching(&self) -> bool {
        self.pending_search.is_some() || self.search.as_ref().is_some_and(|s| !s.done.load(Ordering::Relaxed))
    }

    /// Goes to the next match (the first one after the last)
    fn next_match(&mut self) {
        self.step_match(1);
    }

    /// Goes to the previous match (the last one before the first)
    fn previous_match(&mut self) {
        self.step_match(-1);
    }
}

impl PdfView {
    fn step_match(&mut self, step: i64) {
        let total = self.search.as_ref().map_or(0, |s| s.total());
        if total > 0 {
            self.hit = (self.hit as i64 + step).rem_euclid(total as i64) as usize;
            self.reveal = true;
        }
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
unsafe fn pdf_view(py: Python<'_>, view: &mut PdfView, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let ctx = ui.ctx().clone();
    CACHE.with_borrow_mut(|c| c.begin(&ctx));
    let margin = kwarg::<f32>(kwargs, "margin")?.unwrap_or(12.0).max(0.0);
    let gap = kwarg::<f32>(kwargs, "gap")?.unwrap_or(12.0).max(0.0);
    let background = kwarg_color(kwargs, "background")?;

    let mut avail = ui.available_size();
    if let Some(height) = kwarg::<f32>(kwargs, "height")? {
        avail.y = height;
    }
    // in a scroll area the available size has no end
    if !avail.x.is_finite() || avail.x > 10000.0 {
        avail.x = 600.0;
    }
    if !avail.y.is_finite() || avail.y > 10000.0 {
        avail.y = 800.0;
    }
    let avail = avail.max(Vec2::splat(20.0));

    // the document given: it may be read by its thread still, or have changed since the last frame
    let (resolved, loading) = match &view.source {
        Some(source) => {
            let mut d = source.bind(py).borrow_mut();
            let loading = d.poll();
            (d.doc.clone(), loading)
        }
        None => (None, false),
    };
    if resolved.as_ref().map(|d| d.id) != view.doc.as_ref().map(|d| d.id) {
        // a first document keeps the page asked for, another one starts at the first page
        if view.doc.is_some() {
            view.current = 0;
            view.goto = Some((0, 0.0));
        }
        view.offset = Vec2::ZERO;
        view.sel = None;
        if let Some(old) = view.search.take() {
            old.cancel.store(true, Ordering::Relaxed);
        }
        view.doc = resolved;
    }
    if loading {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    view.busy = loading;
    let Some(doc) = view.doc.clone().filter(|d| !d.sizes.is_empty()) else {
        let (rect, _) = ui.allocate_exact_size(avail, Sense::hover());
        if let Some(color) = background {
            ui.painter().rect_filled(rect, 0.0, color);
        }
        let text = if loading { "Opening…" } else if view.source.is_some() { "(empty or unreadable PDF)" } else { "(no document)" };
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(14.0), ui.visuals().weak_text_color());
        return Ok(());
    };
    let n = doc.sizes.len();
    view.current = view.current.min(n - 1);
    let single = !view.continuous;

    // a search asked since the last frame
    if let Some((query, accents)) = view.pending_search.take() {
        view.search = Some(start_search(&doc, &query, accents, &ctx));
        view.reveal_first = true;
    }
    if view.reveal_first && view.search.as_ref().is_some_and(|s| s.total() > 0) {
        view.hit = 0;
        view.reveal = true;
        view.reveal_first = false;
    }
    let current_hit = view.search.as_ref().and_then(|s| s.hits.lock().unwrap_or_else(|e| e.into_inner()).order.get(view.hit).copied());
    if view.reveal && single {
        if let Some((page, ..)) = current_hit {
            if page != view.current {
                view.goto = Some((page, 0.0));
            }
        }
    }
    if let Some((p, y)) = view.goto {
        view.goto = Some((p.min(n - 1), y));
    }
    // single page mode: the page changes at once
    if single {
        if let Some((p, _)) = view.goto {
            view.current = p;
        }
    }

    // the pointer over the viewer: zoom and keys
    let hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| view.viewport.contains(*p));
    let typing = ui.memory(|m| m.focused().is_some());
    let mut anchor: Option<Vec2> = None;
    let mut key_move: Option<i32> = None; // pages
    let mut key_scroll: Option<f32> = None; // windows
    if let Some(pos) = hover {
        let zoom_delta = ui.input(|i| i.zoom_delta());
        let mut factor = zoom_delta;
        if !typing {
            ui.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::PageDown) {
                    if single { key_move = Some(1) } else { key_scroll = Some(0.9) }
                }
                if i.consume_key(Modifiers::NONE, Key::PageUp) {
                    if single { key_move = Some(-1) } else { key_scroll = Some(-0.9) }
                }
                if i.consume_key(Modifiers::NONE, Key::Home) {
                    key_move = Some(i32::MIN);
                }
                if i.consume_key(Modifiers::NONE, Key::End) {
                    key_move = Some(i32::MAX);
                }
                if i.consume_key(Modifiers::COMMAND, Key::Plus) || i.consume_key(Modifiers::COMMAND, Key::Equals) {
                    factor *= 1.25;
                }
                if i.consume_key(Modifiers::COMMAND, Key::Minus) {
                    factor /= 1.25;
                }
                if i.consume_key(Modifiers::COMMAND, Key::Num0) {
                    view.fit = Fit::Width;
                }
            });
        }
        if (factor - 1.0).abs() > 1e-4 {
            let base = if view.fit == Fit::None { view.zoom } else { view.last_zoom.max(MIN_ZOOM) };
            view.zoom = (base * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            view.fit = Fit::None;
            anchor = Some(pos - view.viewport.min);
        }
    }
    if let Some(step) = key_move {
        let target = match step {
            i32::MIN => 0,
            i32::MAX => n - 1,
            d => (view.current as i64 + i64::from(d)).clamp(0, n as i64 - 1) as usize,
        };
        view.goto = Some((target, 0.0));
        if single {
            view.current = target;
        }
    }

    // the pages as they are shown
    let rot = view.rotation;
    let swapped = rot % 180 != 0;
    let shown = |i: usize| {
        let (w, h) = doc.sizes[i];
        if swapped { (h, w) } else { (w, h) }
    };
    let (max_w, max_h) = if swapped { (doc.max_h, doc.max_w) } else { (doc.max_w, doc.max_h) };

    // the zoom of the fits
    match view.fit {
        Fit::Width => view.zoom = ((avail.x - 2.0 * margin) / (max_w * POINT)).clamp(MIN_ZOOM, MAX_ZOOM),
        Fit::Page => {
            let (w, h) = if single { shown(view.current) } else { (max_w, max_h) };
            let z = ((avail.x - 2.0 * margin) / (w * POINT)).min((avail.y - 2.0 * margin) / (h * POINT));
            view.zoom = z.clamp(MIN_ZOOM, MAX_ZOOM);
        }
        Fit::None => {}
    }
    // a zoom that changed keeps what is looked at where it is
    if view.last_zoom > 0.0 && (view.zoom / view.last_zoom - 1.0).abs() > 1e-4 {
        let ratio = view.zoom / view.last_zoom;
        let p = anchor.unwrap_or(Vec2::ZERO);
        view.set_offset = Some(((view.offset + p) * ratio - p).max(Vec2::ZERO));
    }
    view.last_zoom = view.zoom;
    let scale = view.zoom * POINT;

    // where the pages are, relative to the top of the content
    let range = if single { view.current..view.current + 1 } else { 0..n };
    let mut tops = Vec::with_capacity(range.len());
    let mut bottoms = Vec::with_capacity(range.len());
    let mut y = margin;
    for i in range.clone() {
        tops.push(y);
        bottoms.push(y + shown(i).1 * scale);
        y += shown(i).1 * scale + gap;
    }
    let total_h = (y - gap + margin).max(margin * 2.0);
    let shown_w = if single { shown(view.current).0 } else { max_w };
    let content_w = avail.x.max(shown_w * scale + 2.0 * margin);
    let first_page = range.start;
    let page_geo = |page: usize, top: f32, left: f32| {
        let (sw, sh) = shown(page);
        let (w, h) = doc.sizes[page];
        Geo { rect: Rect::from_min_size(Pos2::new(left + (content_w - sw * scale) / 2.0, top), Vec2::new(sw * scale, sh * scale)), w, h, rot }
    };
    if let Some((p, at)) = view.goto.take() {
        let top = tops.get(p.saturating_sub(first_page)).copied().unwrap_or(0.0);
        let inside = if rot == 0 { at * scale } else { 0.0 };
        view.set_offset = Some(Vec2::new(view.offset.x, (top + inside - margin).max(0.0)));
        view.current = p;
    }
    if let Some(w) = key_scroll {
        view.set_offset = Some(Vec2::new(view.offset.x, (view.offset.y + w * view.viewport.height()).max(0.0)));
    }
    // the match to show: the place of its first rectangle
    if view.reveal {
        if let Some((page, start, end)) = current_hit {
            match doc.cached_text(page) {
                Some(text) if (first_page..first_page + tops.len()).contains(&page) => {
                    if let Some(r) = text.rects(start, end).first() {
                        let geo = page_geo(page, tops[page - first_page], 0.0);
                        let at = geo.rect_to_screen(*r);
                        let want = Vec2::new((at.center().x - view.viewport.width() / 2.0).max(0.0), (at.top() - view.viewport.height() * 0.35).max(0.0));
                        // sideways only when the match is out of the window
                        let visible_x = at.right() > view.offset.x && at.left() < view.offset.x + view.viewport.width();
                        view.set_offset = Some(Vec2::new(if visible_x { view.offset.x } else { want.x }, want.y));
                    }
                    view.reveal = false;
                }
                Some(_) => {}
                None => doc.ask_text(&ctx, page),
            }
        }
    }

    let tool = view.tool;
    let sel = view.sel;
    let dark = view.dark;
    let search = view.search.clone();
    let mut out_sel = sel;
    let mut out_link: Option<crate::pdf_nav::Target> = None;
    let mut edge = 0.0f32;

    let mut area = egui::ScrollArea::both().id_salt(("pdf_view", view.id)).auto_shrink([false, false]).max_width(avail.x).max_height(avail.y);
    if let Some(offset) = view.set_offset.take() {
        area = area.scroll_offset(offset);
    }
    let output = area.show(ui, |ui| {
        // with the hand the scroll area takes the drags itself
        let sense = if tool == Tool::Select { Sense::click_and_drag() } else { Sense::click() };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(content_w, total_h), sense);
        if let Some(color) = background {
            ui.painter().rect_filled(ui.clip_rect(), 0.0, color);
        }
        let clip = ui.clip_rect();
        let ahead = clip.height();
        let (from, to) = (clip.top() - ahead, clip.bottom() + ahead);
        let first = bottoms.partition_point(|&b| rect.top() + b < from);
        let centre = clip.center().y;
        let mut geos: Vec<(usize, Geo)> = Vec::new();
        for (k, &top) in tops.iter().enumerate().skip(first) {
            let page = first_page + k;
            let geo = page_geo(page, rect.top() + top, rect.left());
            if geo.rect.top() > to {
                break;
            }
            let visible = geo.rect.bottom() >= clip.top() && geo.rect.top() <= clip.bottom();
            let priority = (geo.rect.center().y - centre).abs() + if visible { 0.0 } else { 1.0e6 };
            paint_page(ui, &doc, page, geo, dark, priority, visible);
            if visible {
                geos.push((page, geo));
            }
        }

        // the pointer, and the page under it (the nearest one while it is dragged out of the pages)
        let pointer = response.interact_pointer_pos().or_else(|| response.hover_pos());
        let page_at = |pos: Pos2| -> Option<(usize, Geo)> {
            geos.iter()
                .find(|(_, g)| g.rect.contains(pos))
                .or_else(|| geos.iter().min_by(|a, b| a.1.rect.distance_to_pos(pos).total_cmp(&b.1.rect.distance_to_pos(pos))))
                .copied()
        };
        let boundary = |pos: Pos2| -> Option<At> {
            let (page, geo) = page_at(pos)?;
            let text = doc.cached_text(page)?;
            let (at, _) = text.boundary_at(geo.from_screen(pos))?;
            Some(At { page, at })
        };
        // the text of the page under the pointer is read before it is needed
        if tool == Tool::Select {
            if let Some((page, _)) = response.hover_pos().and_then(page_at) {
                doc.ask_text(ui.ctx(), page);
            }
        }

        // the marks: matches, selection, links
        let (lo, hi) = out_sel.filter(|s| !s.is_empty()).map_or((None, None), |s| {
            let (lo, hi) = s.ordered();
            (Some(lo), Some(hi))
        });
        let mut over_link = false;
        for &(page, geo) in &geos {
            let painter = ui.painter();
            if let Some(search) = &search {
                let found = search.hits.lock().unwrap_or_else(|e| e.into_inner()).by_page.get(&page).cloned();
                if let Some(found) = found {
                    match doc.cached_text(page) {
                        Some(text) => {
                            for (s, e) in found {
                                let color = if current_hit == Some((page, s, e)) { Color32::from_rgba_unmultiplied(255, 120, 0, 150) } else { Color32::from_rgba_unmultiplied(255, 210, 0, 110) };
                                for r in text.rects(s, e) {
                                    painter.rect_filled(geo.rect_to_screen(r).expand(1.0), 2.0, color);
                                }
                            }
                        }
                        None => doc.ask_text(ui.ctx(), page),
                    }
                }
            }
            if let (Some(lo), Some(hi)) = (lo, hi) {
                if (lo.page..=hi.page).contains(&page) {
                    match doc.cached_text(page) {
                        Some(text) => {
                            let from = if page == lo.page { lo.at } else { 0 };
                            let to = if page == hi.page { hi.at } else { text.chars.len() };
                            for r in text.rects(from, to) {
                                painter.rect_filled(geo.rect_to_screen(r), 1.0, Color32::from_rgba_unmultiplied(60, 120, 255, 90));
                            }
                        }
                        None => doc.ask_text(ui.ctx(), page),
                    }
                }
            }
            if let (Some(pos), Some(text)) = (response.hover_pos(), doc.cached_text(page)) {
                if let Some(link) = text.links.iter().find(|l| geo.rect_to_screen(l.rect).contains(pos)) {
                    over_link = true;
                    painter.rect_stroke(geo.rect_to_screen(link.rect), 1.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(60, 120, 255, 160)), egui::StrokeKind::Outside);
                    if response.clicked() {
                        out_link = Some(link.target.clone());
                    }
                }
            }
        }

        // the cursor
        if response.hovered() || response.dragged() {
            if over_link {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            } else if tool == Tool::Hand {
                ui.ctx().set_cursor_icon(if response.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
            } else if let Some((page, geo)) = response.hover_pos().and_then(page_at) {
                let inside = response.hover_pos().is_some_and(|p| geo.rect.contains(p));
                let on_text = doc.cached_text(page).zip(response.hover_pos()).and_then(|(t, p)| t.boundary_at(geo.from_screen(p))).is_some_and(|(_, d)| d <= 0.5);
                if inside && on_text {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                }
            }
        }

        // the selection with the mouse
        if tool == Tool::Select && out_link.is_none() {
            if let Some(pos) = pointer {
                if response.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(b) = boundary(pos) {
                        out_sel = Some(Selection { anchor: b, head: b });
                    }
                } else if response.dragged_by(egui::PointerButton::Primary) {
                    if let (Some(s), Some(b)) = (out_sel, boundary(pos)) {
                        out_sel = Some(Selection { head: b, ..s });
                    }
                    // near the edge of the window the pages scroll
                    if pos.y < clip.top() + 24.0 {
                        edge = -(24.0 - (pos.y - clip.top()).max(0.0)) * 0.8;
                    } else if pos.y > clip.bottom() - 24.0 {
                        edge = (24.0 - (clip.bottom() - pos.y).max(0.0)) * 0.8;
                    }
                } else if response.triple_clicked() {
                    if let Some(b) = boundary(pos) {
                        let (a, z) = doc.cached_text(b.page).map_or((b.at, b.at), |t| t.line_around(b.at));
                        out_sel = Some(Selection { anchor: At { page: b.page, at: a }, head: At { page: b.page, at: z } });
                    }
                } else if response.double_clicked() {
                    if let Some(b) = boundary(pos) {
                        let (a, z) = doc.cached_text(b.page).map_or((b.at, b.at), |t| t.word_around(b.at));
                        out_sel = Some(Selection { anchor: At { page: b.page, at: a }, head: At { page: b.page, at: z } });
                    }
                } else if response.clicked() {
                    out_sel = None;
                }
            }
        }
    });
    view.offset = output.state.clamped_offset();
    view.viewport = output.inner_rect;
    view.sel = out_sel;
    if edge != 0.0 {
        view.set_offset = Some(Vec2::new(view.offset.x, (view.offset.y + edge).max(0.0)));
        ctx.request_repaint();
    }
    match out_link {
        Some(crate::pdf_nav::Target::Page { page, y }) => view.goto = Some((page, y.unwrap_or(0.0))),
        Some(crate::pdf_nav::Target::Uri(uri)) => ctx.open_url(egui::OpenUrl::new_tab(uri)),
        None => {}
    }

    // Escape removes the selection, Ctrl+C (Cmd+C) copies it
    if hover.is_some() && !typing {
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            view.sel = None;
        }
    }
    let copy_event = hover.is_some() && !typing && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)));
    if copy_event || std::mem::take(&mut view.copy_request) {
        if let Some(sel) = view.sel.filter(|s| !s.is_empty()) {
            ctx.copy_text(selection_text(&doc, &sel));
        }
    }

    // the page at the top of the window (a bit under the top), unless a jump is going on
    if !single && view.set_offset.is_none() && edge == 0.0 {
        let line = view.offset.y + view.viewport.height() * 0.3;
        let k = tops.partition_point(|&t| t <= line).saturating_sub(1);
        view.current = (first_page + k).min(n - 1);
    }
    view.busy = CACHE.with_borrow(|c| c.pending.keys().any(|k| k.0 == doc.id)) || search_running(&view.search);
    Ok(())
}

fn search_running(search: &Option<Arc<Search>>) -> bool {
    search.as_ref().is_some_and(|s| !s.done.load(Ordering::Relaxed))
}

// ---------------------------------------------------------------------------------------------
// a thumbnail

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
unsafe fn pdf_thumbnail(mut doc: PyRefMut<'_, PdfDocument>, page: usize, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<bool> {
    let ui = current_ui(&UI)?;
    let ctx = ui.ctx().clone();
    CACHE.with_borrow_mut(|c| c.begin(&ctx));
    let size = kwarg::<f32>(kwargs, "size")?.unwrap_or(96.0).max(8.0);
    let selected = kwarg::<bool>(kwargs, "selected")?.unwrap_or(false);
    let label = kwarg::<bool>(kwargs, "label")?.unwrap_or(true);
    let dark = kwarg::<bool>(kwargs, "dark")?.unwrap_or(false);
    if doc.poll() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    let Some(d) = doc.doc.clone() else { return Ok(false) };
    let index = page.checked_sub(1).filter(|i| *i < d.sizes.len()).ok_or_else(|| PyIndexError::new_err(format!("no page {page}")))?;
    let (pw, ph) = d.sizes[index];
    let k = size / pw.max(ph);
    let (w, h) = (pw * k, ph * k);
    let text_h = if label { 16.0 } else { 0.0 };
    let pad = 4.0;
    let (outer, response) = ui.allocate_exact_size(Vec2::new(size + 2.0 * pad, size.min(h + 2.0 * pad) + text_h), Sense::click());
    if ui.is_rect_visible(outer) {
        if selected {
            ui.painter().rect_stroke(outer, 3.0, Stroke::new(2.0, ui.visuals().selection.bg_fill), egui::StrokeKind::Inside);
        } else if response.hovered() {
            ui.painter().rect_stroke(outer, 3.0, Stroke::new(1.0, ui.visuals().widgets.hovered.bg_stroke.color), egui::StrokeKind::Inside);
        }
        let rect = Rect::from_min_size(Pos2::new(outer.center().x - w / 2.0, outer.top() + pad), Vec2::new(w, h));
        let geo = Geo { rect, w: pw, h: ph, rot: 0 };
        paint_page(ui, &d, index, geo, dark, (ui.clip_rect().center().y - rect.center().y).abs(), true);
        if label {
            ui.painter().text(
                Pos2::new(outer.center().x, outer.bottom() - 2.0),
                Align2::CENTER_BOTTOM,
                page.to_string(),
                FontId::proportional(11.0),
                ui.visuals().weak_text_color(),
            );
        }
    }
    Ok(response.clicked())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PdfDocument>()?;
    m.add_class::<PdfView>()?;
    m.add_function(wrap_pyfunction!(pdf_view, m)?)?;
    m.add_function(wrap_pyfunction!(pdf_thumbnail, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_goes_to_the_screen_and_back_for_every_rotation() {
        for rot in [0u16, 90, 180, 270] {
            let (w, h) = (200.0, 300.0);
            let swapped = rot % 180 != 0;
            let size = if swapped { Vec2::new(h, w) } else { Vec2::new(w, h) } * 2.0;
            let geo = Geo { rect: Rect::from_min_size(Pos2::new(10.0, 20.0), size), w, h, rot };
            assert!((geo.scale() - 2.0).abs() < 1e-4);
            for p in [Pos2::new(0.0, 0.0), Pos2::new(200.0, 0.0), Pos2::new(50.0, 120.0), Pos2::new(200.0, 300.0)] {
                let back = geo.from_screen(geo.to_screen(p));
                assert!((back - p).length() < 1e-3, "rotation {rot}: {p:?} came back as {back:?}");
                assert!(geo.rect.expand(0.01).contains(geo.to_screen(p)), "rotation {rot}: {p:?} is outside the page");
            }
        }
    }

    #[test]
    fn a_clockwise_quarter_turn_puts_the_top_left_corner_at_the_top_right() {
        let geo = Geo { rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0)), w: 200.0, h: 300.0, rot: 90 };
        assert_eq!(geo.to_screen(Pos2::new(0.0, 0.0)), Pos2::new(300.0, 0.0));
    }

    #[test]
    fn a_selection_is_ordered_whatever_the_way_it_was_dragged() {
        let (a, b) = (At { page: 0, at: 9 }, At { page: 2, at: 1 });
        assert_eq!(Selection { anchor: b, head: a }.ordered(), (a, b));
        assert!(Selection { anchor: a, head: a }.is_empty());
    }

    #[test]
    fn the_size_of_a_picture_stays_inside_the_caps() {
        let size = (612.0, 792.0);
        assert_eq!(picture_size(size, step_for(1.0)), (612, 792));
        let (w, h) = picture_size(size, step_for(50.0));
        assert!(w as f32 <= MAX_SIDE && h as f32 <= MAX_SIDE && (w as f32) * (h as f32) <= MAX_PIXELS * 1.01);
        assert_eq!(step_for(1.0), 0);
        assert_eq!(step_for(2.0), 8);
    }
}
