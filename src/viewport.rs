//! More windows for one app: `Viewport(title, update_func)` is a second native window of the same
//! process (one icon in the Dock, the windows grouped by the system, Mission Control shows them all).
//! It is a state object: created once, `open()` shows it, `close()` or its close button hides it.
//! Its update function is called like the one of `run_native`, after the update function of the main
//! window, with the ui stack of its own window.

use std::sync::Mutex;

use eframe::egui::{self, ViewportBuilder, ViewportId};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::{draw_frame, kwarg, Context};

static OPEN: Mutex<Vec<Py<Viewport>>> = Mutex::new(Vec::new());
static SERIAL: Mutex<u64> = Mutex::new(0);

/// A window of the app besides the main one, with its own update function.
///
/// The windows share the process: the values (`Str`, `Int`...) and the state objects are those of the
/// app, so two windows can show the same data, or each its own. A window closed by the user (its
/// close button) is hidden, `open()` shows it again. Closing the main window ends the app.
///
/// Args:
///     title (str): the title of the window
///
///     update_func (Callable[[Context], None]): draws the window, like the one of run_native
///
/// Keyword args:
///
///     inner_width (float), inner_height (float): the size of the window. Default is 640 x 480
///
///     min_inner_width (float), min_inner_height (float): the smallest size of the window
///
///     resizable (bool): Default is True
///
///     fullsize_content (bool): the content goes under the title bar of macOS. Default is False
///
///     decorations (bool): False removes the title bar and the borders of the system (draw them with
///         window_controls and window_drag_area). Default is True
///
/// Methods:
///     open(): shows the window (it is created at the next frame)
///
///     close(): hides it
///
///     toggle(): open or close
///
///     is_open (bool): True while the window is shown
///
/// Example::
///
///     def inspector(ctx):
///         heading("Inspector")
///
///     window = Viewport("Inspector", inspector, inner_width=360, inner_height=480)
///
///     def update(ctx):
///         if button_clicked("Inspector"):
///             window.toggle()
///
///     run_native("App", update)
#[pyclass]
pub(crate) struct Viewport {
    title: String,
    update_func: Py<PyAny>,
    serial: u64,
    size: [f32; 2],
    min_size: Option<[f32; 2]>,
    resizable: bool,
    fullsize: bool,
    decorations: bool,
    open: bool,
}

#[pymethods]
impl Viewport {
    #[new]
    #[pyo3(signature = (title, update_func, **kwargs))]
    fn new(title: String, update_func: Py<PyAny>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let mut serial = SERIAL.lock().unwrap();
        *serial += 1;
        let min = (kwarg::<f32>(kwargs, "min_inner_width")?, kwarg::<f32>(kwargs, "min_inner_height")?);
        Ok(Viewport {
            title,
            update_func,
            serial: *serial,
            size: [
                kwarg::<f32>(kwargs, "inner_width")?.unwrap_or(640.0),
                kwarg::<f32>(kwargs, "inner_height")?.unwrap_or(480.0),
            ],
            min_size: match min {
                (Some(w), Some(h)) => Some([w, h]),
                _ => None,
            },
            resizable: kwarg::<bool>(kwargs, "resizable")?.unwrap_or(true),
            fullsize: kwarg::<bool>(kwargs, "fullsize_content")?.unwrap_or(false),
            decorations: kwarg::<bool>(kwargs, "decorations")?.unwrap_or(true),
            open: false,
        })
    }

    /// Shows the window.
    fn open(slf: &Bound<'_, Self>) {
        let mut this = slf.borrow_mut();
        if !this.open {
            this.open = true;
            OPEN.lock().unwrap().push(slf.clone().unbind());
        }
    }

    /// Hides the window.
    fn close(&mut self) {
        self.open = false;
    }

    /// Shows the window if it is hidden, hides it otherwise.
    fn toggle(slf: &Bound<'_, Self>) {
        let open = slf.borrow().open;
        if open {
            slf.borrow_mut().open = false;
        } else {
            Self::open(slf);
        }
    }

    #[getter]
    fn is_open(&self) -> bool {
        self.open
    }

    #[setter]
    fn set_title(&mut self, title: String) {
        self.title = title;
    }

    #[getter]
    fn title(&self) -> String {
        self.title.clone()
    }
}

/// Draws the windows that are open, after the main window: each one is an immediate viewport of egui.
/// A window the user closes is hidden; the ones closed by the app are forgotten.
pub(crate) unsafe fn show_all(ctx: &egui::Context) {
    if OPEN.lock().unwrap().is_empty() {
        return;
    }
    Python::attach(|py| {
        let list: Vec<Py<Viewport>> = OPEN.lock().unwrap().iter().map(|w| w.clone_ref(py)).collect();
        for window in &list {
            let (title, func, serial, size, min_size, resizable, fullsize, decorations) = {
                let this = window.bind(py).borrow();
                if !this.open {
                    continue;
                }
                (
                    this.title.clone(),
                    this.update_func.clone_ref(py),
                    this.serial,
                    this.size,
                    this.min_size,
                    this.resizable,
                    this.fullsize,
                    this.decorations,
                )
            };
            let mut builder = ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(size)
                .with_resizable(resizable)
                .with_decorations(decorations);
            if let Some(min) = min_size {
                builder = builder.with_min_inner_size(min);
            }
            if fullsize {
                builder = builder
                    .with_fullsize_content_view(true)
                    .with_titlebar_shown(false)
                    .with_title_shown(false);
            }
            let id = ViewportId::from_hash_of(("pyegui_window", serial));
            let close = ctx.show_viewport_immediate(id, builder, |ui, _class| {
                // no backdrop effect behind these windows: a plain background, the boxes lie over it
                ui.painter().rect_filled(ui.max_rect(), 0.0, ui.visuals().panel_fill);
                draw_frame(py, ui, func.bind(py), Context(ui.ctx().clone()));
                ui.ctx().input(|i| i.viewport().close_requested())
            });
            if close {
                window.bind(py).borrow_mut().open = false;
            }
        }
    });
    Python::attach(|py| OPEN.lock().unwrap().retain(|w| w.bind(py).borrow().open));
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Viewport>()
}
