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
use craft_pdf::syntax::Pdf;
use eframe::egui::{self, Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::pdf::{info_dict, open_with_info, Info};
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

/// An open PDF, shared by the threads that draw its pages.
pub(crate) struct Doc {
    id: u64,
    pdf: Pdf,
    info: Info,
    /// The size of every page in points, its rotation applied.
    sizes: Vec<(f32, f32)>,
    max_w: f32,
    max_h: f32,
}

const _: fn() = || {
    fn shared<T: Send + Sync>() {}
    shared::<Doc>();
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl Doc {
    fn new(pdf: Pdf, info: Info) -> Option<Doc> {
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
        Some(Doc { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), pdf, info, sizes, max_w, max_h })
    }
}

/// An open PDF file.
///
/// The file is read at once (it is not kept open, so it can be moved or deleted afterwards on Windows)
/// and its pages are drawn on demand by `pdf_view` and `pdf_thumbnail`. A document is a state object:
/// create it once, outside the update function, and share it between the views and the thumbnails.
///
/// A file that cannot be opened does not raise: `error` tells why and `pages` is 0. A PDF that asks a
/// password has `needs_password` set: give it with `unlock` (or as `password`).
///
/// Properties (read only): `path`, `pages`, `needs_password`, `error`, and `info`, the dict of
/// `pdf_info` (title, author, dates, rights...) read from the open document.
///
/// Args:
///     path (str): path of the PDF file
///
///     password (str): the password to open it with, none by default (a PDF that has only an owner
///         password opens without one)
///
/// Example::
///
///     doc = PdfDocument("contract.pdf")
///     if doc.needs_password and not doc.unlock("secret"):
///         print("wrong password")
///     print(doc.pages, doc.info["title"], doc.page_size(1))
#[pyclass]
pub(crate) struct PdfDocument {
    doc: Option<Arc<Doc>>,
    path: String,
    /// the file while it waits for a password
    bytes: Option<Arc<Vec<u8>>>,
    info: Info,
}

impl PdfDocument {
    fn load(&mut self, bytes: Arc<Vec<u8>>, password: Option<&str>) {
        let (info, pdf) = open_with_info(Arc::clone(&bytes), password);
        self.info = info;
        self.doc = None;
        self.bytes = None;
        if let Some(pdf) = pdf {
            match Doc::new(pdf, self.info.clone()) {
                Some(doc) => self.doc = Some(Arc::new(doc)),
                None => self.info.error = Some("the PDF reader failed on this file".into()),
            }
        } else if self.info.needs_password {
            self.bytes = Some(bytes);
        }
    }
}

#[pymethods]
impl PdfDocument {
    #[new]
    #[pyo3(signature = (path, password=None))]
    fn new(py: Python<'_>, path: String, password: Option<String>) -> Self {
        let mut document = PdfDocument { doc: None, path: path.clone(), bytes: None, info: Info::default() };
        let read = py.detach(|| std::fs::read(&path));
        match read {
            Ok(bytes) => {
                py.detach(|| document.load(Arc::new(bytes), password.as_deref()));
            }
            // the text of the OS depends on the language and the code page: the kind says it all
            Err(e) => document.info.error = Some(format!("cannot read the file ({:?})", e.kind())),
        }
        document
    }

    /// Path of the file
    #[getter]
    fn path(&self) -> &str {
        &self.path
    }

    /// Number of pages, 0 when the document is not open
    #[getter]
    fn pages(&self) -> usize {
        self.doc.as_ref().map_or(0, |d| d.sizes.len())
    }

    /// True while the PDF asks a password to be opened
    #[getter]
    fn needs_password(&self) -> bool {
        self.info.needs_password
    }

    /// Why the file could not be read, None when it is open (or waits for a password)
    #[getter]
    fn error(&self) -> Option<String> {
        self.info.error.clone()
    }

    /// What the document says about itself, the dict of `pdf_info`
    #[getter]
    fn info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        info_dict(py, &self.info)
    }

    /// Opens a PDF that asks a password. Returns True when it is open, False when the password is
    /// wrong (or when the document did not need one: then it is open already).
    fn unlock(&mut self, py: Python<'_>, password: &str) -> bool {
        if self.doc.is_some() {
            return true;
        }
        let Some(bytes) = self.bytes.clone() else { return false };
        py.detach(|| self.load(bytes, Some(password)));
        self.doc.is_some()
    }

    /// Size of a page in points (1/72 inch), its rotation applied.
    ///
    /// Args:
    ///     page (int): the number of the page, from 1
    fn page_size(&self, page: usize) -> PyResult<(f32, f32)> {
        self.doc
            .as_ref()
            .and_then(|d| page.checked_sub(1).and_then(|i| d.sizes.get(i).copied()))
            .ok_or_else(|| PyIndexError::new_err(format!("no page {page}")))
    }

    fn __len__(&self) -> usize {
        self.pages()
    }

    fn __repr__(&self) -> String {
        format!("PdfDocument({:?}, pages={})", self.path, self.pages())
    }
}

// ---------------------------------------------------------------------------------------------
// the pictures of the pages, drawn in threads

/// A picture asked for: the document, the page (from 0) and the size step.
type PageKey = (u64, usize, i32);

/// The pixel size of a page at a size step, inside the caps.
fn picture_size(doc: &Doc, page: usize, step: i32) -> (u32, u32) {
    let (pw, ph) = doc.sizes[page];
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
    let (width, height) = picture_size(&job.doc, job.key.1, job.key.2);
    let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let pages = job.doc.pdf.pages();
        let page = pages.get(job.key.1)?;
        let (pw, ph) = page.render_dimensions();
        if !(pw.is_finite() && ph.is_finite() && pw > 0.0 && ph > 0.0) {
            return Some(Err(raster::Error::InvalidPage));
        }
        let options = Options {
            scale: [width as f32 / pw, height as f32 / ph],
            size: Some([width, height]),
            offset: [0.0, 0.0],
            background: [255, 255, 255, 255],
            color_replacement: None,
        };
        let mut settings = InterpreterSettings::default();
        settings.cancelled = Some(Arc::clone(&job.cancel));
        Some(raster::render(page, &RenderCache::new(), &settings, &options))
    }));
    match rendered {
        Ok(Some(Ok(r))) => Outcome::Picture { width: r.width, height: r.height, rgba: r.bytes().to_vec() },
        Ok(Some(Err(raster::Error::Cancelled))) => Outcome::Cancelled,
        Ok(Some(Err(raster::Error::OutputLimit | raster::Error::Allocation))) => Outcome::Failed("the page is too big to draw"),
        Ok(Some(Err(_))) | Ok(None) => Outcome::Failed("the page cannot be drawn"),
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
                    let name = format!("pdf-{}-{}-{}", key.0, key.1, key.2);
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
    fn want(&mut self, ctx: &egui::Context, doc: &Arc<Doc>, page: usize, step: i32, priority: f32) -> Option<egui::TextureId> {
        let key = (doc.id, page, step);
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
            .filter(|(k, _)| k.0 == doc.id && k.1 == page)
            .min_by_key(|(k, _)| (k.2 - step).abs())?;
        best.1.used = frame;
        Some(best.1.texture.id())
    }

    fn failed(&self, doc: &Doc, page: usize, step: i32) -> Option<&'static str> {
        self.failed.get(&(doc.id, page, step)).copied()
    }
}

/// Draws a page in `rect`: its picture, or a blank sheet with a sign while it is drawn.
fn paint_page(ui: &egui::Ui, doc: &Arc<Doc>, page: usize, rect: Rect, priority: f32, draw: bool) {
    let ctx = ui.ctx();
    let step = step_for(rect.width() / doc.sizes[page].0 * ctx.pixels_per_point());
    let (texture, failed) = CACHE.with_borrow_mut(|c| (c.want(ctx, doc, page, step, priority), c.failed(doc, page, step)));
    if !draw {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect.translate(egui::vec2(1.5, 2.5)).expand(1.0), 2.0, Color32::from_black_alpha(45));
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    match (texture, failed) {
        (Some(id), _) => {
            painter.image(id, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        (None, Some(why)) => {
            painter.text(rect.center(), Align2::CENTER_CENTER, format!("⚠ {why}"), FontId::proportional(14.0), Color32::from_gray(120));
        }
        (None, None) => {
            painter.text(rect.center(), Align2::CENTER_CENTER, "…", FontId::proportional(20.0), Color32::from_gray(160));
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

static NEXT_VIEW: AtomicU64 = AtomicU64::new(1);

/// The state of a PDF viewer: the document it shows, the page, the zoom and the scrolling.
///
/// Create it once, outside the update function, and draw it with `pdf_view`. In the continuous mode
/// (the default) the pages follow one another in a scroll area and `page` is the one at the top of
/// the window; in the single page mode only the current page is shown.
///
/// Properties: `doc` (a `PdfDocument` or None, write only), `page` (from 1, read it to show a page
/// number, write it to go to a page), `pages`, `zoom` (1.0 is 100%, 96 dpi; writing it turns `fit`
/// off), `fit` (`"width"`, `"page"` or `"none"`), `continuous`, `busy` (pages are being drawn).
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
///
///     def update_func(ctx):
///         if button_clicked("Fit the page"):
///             view.fit = "page"
///         label(f"page {view.page} / {view.pages}")
///         pdf_view(view)
#[pyclass]
pub(crate) struct PdfView {
    id: u64,
    doc: Option<Arc<Doc>>,
    current: usize,
    goto: Option<usize>,
    zoom: f32,
    fit: Fit,
    continuous: bool,
    /// the scroll offset and the viewport of the last frame
    offset: Vec2,
    viewport: Rect,
    last_zoom: f32,
    set_offset: Option<Vec2>,
    busy: bool,
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
    fn new(doc: Option<PyRef<'_, PdfDocument>>, fit: &str, zoom: f32, continuous: bool, page: usize) -> PyResult<Self> {
        let page = page.max(1) - 1;
        Ok(PdfView {
            id: NEXT_VIEW.fetch_add(1, Ordering::Relaxed),
            doc: doc.and_then(|d| d.doc.clone()),
            current: page,
            goto: Some(page),
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            fit: parse_fit(fit)?,
            continuous,
            offset: Vec2::ZERO,
            viewport: Rect::NOTHING,
            last_zoom: 0.0,
            set_offset: None,
            busy: false,
        })
    }

    /// The document shown (write only): a `PdfDocument`, or None to show nothing. Giving the document
    /// that is shown already changes nothing.
    #[setter]
    fn set_doc(&mut self, doc: Option<PyRef<'_, PdfDocument>>) {
        let new = doc.and_then(|d| d.doc.clone());
        if new.as_ref().map(|d| d.id) != self.doc.as_ref().map(|d| d.id) {
            self.doc = new;
            self.current = 0;
            self.goto = Some(0);
            self.offset = Vec2::ZERO;
        }
    }

    /// The page at the top of the window, from 1. Writing it goes to that page.
    #[getter]
    fn page(&self) -> usize {
        self.current + 1
    }

    #[setter]
    fn set_page(&mut self, page: usize) {
        let last = self.doc.as_ref().map_or(0, |d| d.sizes.len()).max(1) - 1;
        let page = (page.max(1) - 1).min(last);
        self.goto = Some(page);
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
            self.goto = Some(self.current);
            self.offset = Vec2::ZERO;
        }
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
}

/// Shows the pages of a PDF in a viewer that takes all the space available (or the height given).
/// The pages are drawn by threads: the window never waits for them, a page that is not ready shows
/// a "…" (or a blurry picture of itself while a sharper one is drawn).
///
/// The wheel and the scroll bars scroll, Ctrl + wheel (or a pinch) zooms around the pointer, and
/// with the pointer over the viewer PageUp / PageDown go a page (a window in the continuous mode)
/// further, Home and End to the first and the last page.
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

    let Some(doc) = view.doc.clone().filter(|d| !d.sizes.is_empty()) else {
        let (rect, _) = ui.allocate_exact_size(avail, Sense::hover());
        if let Some(color) = background {
            ui.painter().rect_filled(rect, 0.0, color);
        }
        let text = if view.doc.is_some() { "(empty PDF)" } else { "(no document)" };
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(14.0), ui.visuals().weak_text_color());
        view.busy = false;
        return Ok(());
    };
    let n = doc.sizes.len();
    view.current = view.current.min(n - 1);
    let single = !view.continuous;
    if let Some(p) = view.goto {
        view.goto = Some(p.min(n - 1));
    }
    // single page mode: the page changes at once
    if single {
        if let Some(p) = view.goto.take() {
            view.current = p.min(n - 1);
            view.set_offset = Some(Vec2::ZERO);
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
        if (zoom_delta - 1.0).abs() > 1e-4 {
            let base = if view.fit == Fit::None { view.zoom } else { view.last_zoom.max(MIN_ZOOM) };
            view.zoom = (base * zoom_delta).clamp(MIN_ZOOM, MAX_ZOOM);
            view.fit = Fit::None;
            anchor = Some(pos - view.viewport.min);
        }
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
            });
        }
    }
    if let Some(step) = key_move {
        let target = match step {
            i32::MIN => 0,
            i32::MAX => n - 1,
            d => (view.current as i64 + i64::from(d)).clamp(0, n as i64 - 1) as usize,
        };
        view.goto = Some(target);
        if single {
            view.current = target;
            view.goto = None;
            view.set_offset = Some(Vec2::ZERO);
        }
    }

    // the zoom of the fits
    match view.fit {
        Fit::Width => view.zoom = ((avail.x - 2.0 * margin) / (doc.max_w * POINT)).clamp(MIN_ZOOM, MAX_ZOOM),
        Fit::Page => {
            let (w, h) = if single { doc.sizes[view.current] } else { (doc.max_w, doc.max_h) };
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
        bottoms.push(y + doc.sizes[i].1 * scale);
        y += doc.sizes[i].1 * scale + gap;
    }
    let total_h = (y - gap + margin).max(margin * 2.0);
    let shown_w = if single { doc.sizes[view.current].0 } else { doc.max_w };
    let content_w = avail.x.max(shown_w * scale + 2.0 * margin);
    if let Some(p) = view.goto.take() {
        let top = tops.get(p.saturating_sub(range.start)).copied().unwrap_or(0.0);
        view.set_offset = Some(Vec2::new(view.offset.x, (top - margin).max(0.0)));
        view.current = p;
    }
    if let Some(w) = key_scroll {
        view.set_offset = Some(Vec2::new(view.offset.x, (view.offset.y + w * view.viewport.height()).max(0.0)));
    }

    let mut area = egui::ScrollArea::both().id_salt(("pdf_view", view.id)).auto_shrink([false, false]).max_width(avail.x).max_height(avail.y);
    if let Some(offset) = view.set_offset.take() {
        area = area.scroll_offset(offset);
    }
    let sizes = &doc.sizes;
    let first_page = range.start;
    let output = area.show(ui, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(content_w, total_h), Sense::hover());
        if let Some(color) = background {
            ui.painter().rect_filled(ui.clip_rect(), 0.0, color);
        }
        let clip = ui.clip_rect();
        let ahead = clip.height();
        let (from, to) = (clip.top() - ahead, clip.bottom() + ahead);
        let first = bottoms.partition_point(|&b| rect.top() + b < from);
        let centre = clip.center().y;
        for (k, &top) in tops.iter().enumerate().skip(first) {
            let page = first_page + k;
            let (w, h) = (sizes[page].0 * scale, sizes[page].1 * scale);
            let page_rect = Rect::from_min_size(Pos2::new(rect.left() + (content_w - w) / 2.0, rect.top() + top), Vec2::new(w, h));
            if page_rect.top() > to {
                break;
            }
            let visible = page_rect.bottom() >= clip.top() && page_rect.top() <= clip.bottom();
            let priority = (page_rect.center().y - centre).abs() + if visible { 0.0 } else { 1.0e6 };
            paint_page(ui, &doc, page, page_rect, priority, visible);
        }
    });
    view.offset = output.state.offset();
    view.viewport = output.inner_rect;
    // the page at the top of the window (a bit under the top), unless a jump is going on
    if !single {
        let line = view.offset.y + view.viewport.height() * 0.3;
        let k = tops.partition_point(|&t| t <= line).saturating_sub(1);
        view.current = (first_page + k).min(n - 1);
    }
    view.busy = CACHE.with_borrow(|c| c.pending.keys().any(|k| k.0 == doc.id));
    Ok(())
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
unsafe fn pdf_thumbnail(doc: &PdfDocument, page: usize, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<bool> {
    let ui = current_ui(&UI)?;
    let ctx = ui.ctx().clone();
    CACHE.with_borrow_mut(|c| c.begin(&ctx));
    let size = kwarg::<f32>(kwargs, "size")?.unwrap_or(96.0).max(8.0);
    let selected = kwarg::<bool>(kwargs, "selected")?.unwrap_or(false);
    let label = kwarg::<bool>(kwargs, "label")?.unwrap_or(true);
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
        let page_rect = Rect::from_min_size(Pos2::new(outer.center().x - w / 2.0, outer.top() + pad), Vec2::new(w, h));
        paint_page(ui, &d, index, page_rect, (ui.clip_rect().center().y - page_rect.center().y).abs(), true);
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
