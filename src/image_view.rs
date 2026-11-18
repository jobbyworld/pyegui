//! `ImageView` / `image_view`: an image that is zoomed with the wheel and moved with the hand.

use eframe::egui::{self, CursorIcon, Rect, Sense, Vec2};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::style::kwarg_color;
use crate::{current_ui, kwarg, UI};

const MIN_ZOOM: f32 = 0.02;
const MAX_ZOOM: f32 = 64.0;

/// The state of an image viewer: the zoom (1.0 is the size of the image), the place of the image
/// and whether it is fitted to the room. Create it once, outside the update function; it goes back
/// to the fit when `image_view` is given another source.
///
/// Properties: `zoom` (float, 1.0 is 100%, writing it turns `fit` off), `fit` (bool, the image is
/// scaled down to the room, True by default). `reset()` fits again and centers the image.
///
/// Example::
///
///     view = ImageView()
///
///     def update_func(ctx):
///         if button_clicked("Fit"):
///             view.reset()
///         label(f"{view.zoom:.0%}")
///         image_view(view, "file:///tmp/photo.png")
#[pyclass]
pub(crate) struct ImageView {
    zoom: f32,
    fit: bool,
    /// where the center of the image is, from the center of the viewer
    offset: Vec2,
    source: String,
}

#[pymethods]
impl ImageView {
    #[new]
    fn new() -> Self {
        ImageView { zoom: 1.0, fit: true, offset: Vec2::ZERO, source: String::new() }
    }

    /// The zoom, 1.0 is the size of the image. The value used at the last frame when `fit` is on.
    #[getter]
    fn zoom(&self) -> f32 {
        self.zoom
    }

    #[setter]
    fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.fit = false;
    }

    /// True to scale the image down to the room (never up)
    #[getter]
    fn fit(&self) -> bool {
        self.fit
    }

    #[setter]
    fn set_fit(&mut self, fit: bool) {
        self.fit = fit;
        if fit {
            self.offset = Vec2::ZERO;
        }
    }

    /// Fits the image to the room and centers it.
    fn reset(&mut self) {
        self.fit = true;
        self.offset = Vec2::ZERO;
    }
}

/// Shows an image in a viewer that takes all the space available (or the height given). The wheel
/// (or a pinch) zooms around the pointer, a drag moves the image, a double click fits it again,
/// Ctrl + and Ctrl - zoom and Ctrl 0 fits, with the pointer over the viewer.
///
/// Args:
///     view (ImageView): the state of the viewer
///     source (str): the uri of the image: `file://` and a path, or an `http(s)` url
///
/// Keyword args:
///     height (float): the height, the available one by default
///
///     background (tuple | str): the color behind the image, none by default
///
/// Example::
///
///     view = ImageView()
///
///     def update_func(ctx):
///         image_view(view, "file:///tmp/photo.png", background="#202020")
#[pyfunction]
#[pyo3(signature = (view, source, **kwargs))]
unsafe fn image_view(view: &mut ImageView, source: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let ctx = ui.ctx().clone();
    let mut avail = ui.available_size();
    if let Some(h) = kwarg::<f32>(kwargs, "height")? {
        avail.y = h;
    }
    if !avail.x.is_finite() || avail.x > 10000.0 {
        avail.x = 600.0;
    }
    if !avail.y.is_finite() || avail.y > 10000.0 {
        avail.y = 400.0;
    }
    let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    if let Some(bg) = kwarg_color(kwargs, "background")? {
        painter.rect_filled(rect, 0.0, bg);
    }
    if view.source != source {
        view.source = source.to_string();
        view.reset();
    }

    let img = egui::Image::new(source.to_string());
    let poll = img.load_for_size(&ctx, rect.size());
    let natural = match &poll {
        Ok(p) => p.size(),
        Err(_) => None,
    };
    if matches!(poll, Ok(egui::load::TexturePoll::Pending { .. })) {
        ctx.request_repaint();
    }
    let Some(natural) = natural.filter(|s| s.x > 0.0 && s.y > 0.0) else {
        // loading, or not readable
        let text = if poll.is_err() { "Image illisible" } else { "…" };
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(14.0), ui.visuals().weak_text_color());
        return Ok(());
    };

    let fit_zoom = (rect.width() / natural.x).min(rect.height() / natural.y).min(1.0).clamp(MIN_ZOOM, MAX_ZOOM);
    if view.fit {
        view.zoom = fit_zoom;
    }

    // the wheel and the keys, with the pointer over the viewer
    let hover = resp.hover_pos();
    if let Some(pos) = hover {
        let mut factor = ctx.input(|i| i.zoom_delta());
        let wheel = ctx.input(|i| i.smooth_scroll_delta.y);
        if wheel != 0.0 {
            factor *= (wheel * 0.004).exp();
            ctx.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        }
        if ctx.memory(|m| m.focused().is_none()) {
            ctx.input_mut(|i| {
                if i.consume_key(egui::Modifiers::COMMAND, egui::Key::Plus) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Equals) {
                    factor *= 1.25;
                }
                if i.consume_key(egui::Modifiers::COMMAND, egui::Key::Minus) {
                    factor /= 1.25;
                }
                if i.consume_key(egui::Modifiers::COMMAND, egui::Key::Num0) {
                    view.fit = true;
                    view.offset = Vec2::ZERO;
                    view.zoom = fit_zoom;
                }
            });
        }
        if (factor - 1.0).abs() > 1e-4 {
            let new = (view.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            let ratio = new / view.zoom;
            // what is under the pointer stays under it
            let center = rect.center() + view.offset;
            view.offset = (pos + (center - pos) * ratio) - rect.center();
            view.zoom = new;
            view.fit = false;
        }
    }
    if resp.dragged() {
        view.offset += resp.drag_delta();
        view.fit = false;
    }
    if resp.double_clicked() {
        view.reset();
        view.zoom = fit_zoom;
    }
    if resp.hovered() {
        ctx.set_cursor_icon(if resp.dragged() { CursorIcon::Grabbing } else { CursorIcon::Grab });
    }

    // the image stays at least partly in the viewer
    let size = natural * view.zoom;
    let limit = (size + rect.size()) / 2.0 - Vec2::splat(48.0).min(size / 2.0);
    view.offset = Vec2::new(view.offset.x.clamp(-limit.x.max(0.0), limit.x.max(0.0)), view.offset.y.clamp(-limit.y.max(0.0), limit.y.max(0.0)));

    let target = Rect::from_center_size(rect.center() + view.offset, size);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    egui::Image::new(source.to_string()).fit_to_exact_size(size).paint_at(&child, target);
    Ok(())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ImageView>()?;
    m.add_function(wrap_pyfunction!(image_view, m)?)?;
    Ok(())
}
