//! Widgets that show where a path is: path_breadcrumb (the folders from the root, the ancestors
//! shortened when room is missing) and path_combo (the name of the folder, a menu of its ancestors)

use std::path::{Component, Path, PathBuf};

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, FontId, Id, LayerId, Order, Rect, Sense, Stroke, TextStyle, TextWrapMode,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::icons::icon_uri;
use crate::watcher::expand;
use crate::{current_ui, kwarg, run_nested_call, UI};

/// The functions called on the mouse events of a path widget, read from its keyword args
struct Events<'py> {
    on_click: Option<Bound<'py, PyAny>>,
    on_double_click: Option<Bound<'py, PyAny>>,
    on_right_click: Option<Bound<'py, PyAny>>,
    on_middle_click: Option<Bound<'py, PyAny>>,
    context_menu: Option<Bound<'py, PyAny>>,
    /// what happened during this frame: called once the widget is drawn
    calls: Vec<(Bound<'py, PyAny>, String)>,
    menus: Vec<(egui::Response, String)>,
}

impl<'py> Events<'py> {
    fn read(kwargs: Option<&Bound<'py, PyDict>>) -> PyResult<Self> {
        Ok(Events {
            on_click: kwarg(kwargs, "on_click")?,
            on_double_click: kwarg(kwargs, "on_double_click")?,
            on_right_click: kwarg(kwargs, "on_right_click")?,
            on_middle_click: kwarg(kwargs, "on_middle_click")?,
            context_menu: kwarg(kwargs, "context_menu")?,
            calls: vec![],
            menus: vec![],
        })
    }

    /// Notes what the mouse did to the element of the path
    fn see(&mut self, response: &egui::Response, path: &str, left: bool) {
        let pairs = [
            (left && response.clicked(), &self.on_click),
            (left && response.double_clicked(), &self.on_double_click),
            (response.secondary_clicked(), &self.on_right_click),
            (response.middle_clicked(), &self.on_middle_click),
        ];
        for (happened, function) in pairs {
            if let (true, Some(function)) = (happened, function) {
                self.calls.push((function.clone(), path.to_string()));
            }
        }
        if self.context_menu.is_some() {
            self.menus.push((response.clone(), path.to_string()));
        }
    }

    /// Calls the functions, then shows the context menu of the element right-clicked
    unsafe fn run(self, ui: &mut egui::Ui) -> PyResult<()> {
        for (function, path) in &self.calls {
            run_nested_call(ui, function, (path.clone(),))?;
        }
        let Some(menu) = &self.context_menu else { return Ok(()) };
        let mut result = Ok(());
        for (response, path) in self.menus {
            response.context_menu(|menu_ui| {
                if let Err(err) = run_nested_call(menu_ui, menu, (path.clone(),)) {
                    result = Err(err);
                }
            });
        }
        result
    }
}

/// The name shown for the root of the disk: the name of the volume on macOS, like the Finder
fn root_name(path: &Path) -> String {
    #[cfg(target_os = "macos")]
    if path == Path::new("/") {
        if let Ok(volumes) = std::fs::read_dir("/Volumes") {
            for volume in volumes.flatten() {
                if std::fs::canonicalize(volume.path()).is_ok_and(|p| p == Path::new("/")) {
                    return volume.file_name().to_string_lossy().into_owned();
                }
            }
        }
    }
    // "/" stays "/", "C:\\" shows as "C:"
    let text = path.to_string_lossy();
    let trimmed = text.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() { text.into_owned() } else { trimmed.to_string() }
}

/// The folders from the root (or from `from`) down to the path, with the name each one shows
fn steps(path: &Path, from: Option<&Path>) -> Vec<(PathBuf, String)> {
    let mut steps = vec![];
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        let name = match component {
            Component::Normal(name) => name.to_string_lossy().into_owned(),
            Component::Prefix(_) => continue, // "C:" shows with the root that follows it
            _ => root_name(&current),
        };
        steps.push((current.clone(), name));
    }
    if let Some(from) = from {
        if let Some(i) = steps.iter().position(|(p, _)| p == from) {
            steps.drain(..i);
        }
    }
    steps
}

fn paint_icon(ui: &egui::Ui, painter: &egui::Painter, path: &Path, rect: Rect) {
    match icon_uri(ui.ctx(), &path.to_string_lossy(), rect.width()) {
        Some(uri) => {
            let image = egui::Image::new(uri);
            // painted with the painter given, which may be a layer above the ui
            if let Ok(egui::load::TexturePoll::Ready { texture }) =
                image.load_for_size(ui.ctx(), rect.size())
            {
                painter.image(texture.id, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            } else {
                ui.ctx().request_repaint();
            }
        }
        None => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                if path.is_dir() { "📁" } else { "📄" },
                FontId::proportional(rect.height() * 0.8),
                ui.visuals().text_color(),
            );
        }
    }
}

fn galley(ui: &egui::Ui, text: &str, width: f32, color: Color32, style: TextStyle) -> std::sync::Arc<egui::Galley> {
    egui::WidgetText::from(egui::RichText::new(text).color(color)).into_galley(
        ui,
        Some(TextWrapMode::Truncate),
        width.max(1.0),
        style,
    )
}

/// Paints the chevron of a menu: a small "v"
fn paint_chevron(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let s = 3.5;
    painter.line(
        vec![center + vec2(-s, -s * 0.5), center + vec2(0.0, s * 0.5), center + vec2(s, -s * 0.5)],
        Stroke::new(1.5, color),
    );
}

/// The widest an ancestor can be so the breadcrumb fits: the long names are cut first, down to
/// `min` each. None when everything fits
fn cap(widths: &[f32], available: f32, min: f32) -> Option<f32> {
    if widths.iter().sum::<f32>() <= available {
        return None;
    }
    let (mut low, mut high) = (min, widths.iter().cloned().fold(min, f32::max));
    for _ in 0..30 {
        let mid = (low + high) / 2.0;
        if widths.iter().map(|w| w.min(mid)).sum::<f32>() > available {
            high = mid;
        } else {
            low = mid;
        }
    }
    Some(low)
}

/// Shows a path as the list of its folders from the root, like the path bar of the Finder: the
/// icon and the name of each one, separated by ›. When the room is missing the ancestors are
/// shortened, the longest names first, and the mouse over a shortened one shows it in full. A
/// click on a folder returns its path.
///
/// Args:
///     path (str): the path shown, ~ is the home folder
///
/// Keyword args:
///     root (str): the first folder shown, an ancestor of path. Default is the root of the disk
///
///     max_width (float): the widest it can be. Default is the width left in the ui
///
///     icons (bool): shows the icons of the folders. Default is True
///
///     id (str): id of the breadcrumb, needed if there are several in the same ui
///
///     on_click (callable): called with the path of the element clicked
///
///     on_double_click (callable): called with the path of the element double clicked (after
///         on_click for its first click)
///
///     on_right_click (callable): called with the path of the element right-clicked
///
///     on_middle_click (callable): called with the path of the element clicked with the middle
///         button, e.g. to open it in a new tab
///
///     context_menu (callable): called with the path of the element right-clicked, inside a menu:
///         build the menu in it, like for file_list_view
///
/// Returns the path clicked during this frame, None otherwise: the return value or on_click,
/// whichever reads better.
///
/// Example::
///
///     folder = Str(str(Path.home() / "Documents"))
///
///     def update_func(ctx):
///         clicked = path_breadcrumb(folder.value)
///         if clicked:
///             folder.value = clicked
///
///         # or with functions, and a menu on right click
///         def menu(path):
///             if button_clicked("Copy path"):
///                 ctx.copy_text(path)
///                 close_menu()
///
///         path_breadcrumb(folder.value, on_double_click=open_folder, context_menu=menu)
#[pyfunction]
#[pyo3(signature = (path, **kwargs))]
unsafe fn path_breadcrumb(path: PathBuf, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Option<String>> {
    let path = expand(&path);
    let mut events = Events::read(kwargs)?;
    let root = kwarg::<PathBuf>(kwargs, "root")?.map(|r| expand(&r));
    let with_icons = kwarg::<bool>(kwargs, "icons")?.unwrap_or(true);
    let ui = current_ui(&UI)?;
    let id = ui.make_persistent_id(("path_breadcrumb", kwarg::<String>(kwargs, "id")?));
    let available = kwarg::<f32>(kwargs, "max_width")?
        .unwrap_or(f32::INFINITY)
        .min(ui.available_width());

    let steps = steps(&path, root.as_deref());
    if steps.is_empty() {
        return Ok(None);
    }
    let height = ui.spacing().interact_size.y.max(20.0);
    let side = 16.0;
    let (pad, gap) = (5.0, 4.0);
    let text_color = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let separator = galley(ui, "›", 100.0, weak, TextStyle::Body);
    let sep_w = separator.size().x + 6.0;
    let icon_w = if with_icons { side + gap } else { 0.0 };

    // what each folder takes in full, and the narrowest an ancestor gets: its icon and a few letters
    let full: Vec<f32> = steps
        .iter()
        .map(|(_, name)| pad * 2.0 + icon_w + galley(ui, name, f32::INFINITY, text_color, TextStyle::Body).size().x)
        .collect();
    let min = pad * 2.0 + icon_w + galley(ui, "abc…", f32::INFINITY, text_color, TextStyle::Body).size().x;
    let seps = sep_w * (steps.len() - 1) as f32;
    let last = *full.last().unwrap();
    let ancestors = &full[..full.len() - 1];
    let limit = cap(ancestors, (available - seps - last).max(0.0), min);
    let mut widths: Vec<f32> = ancestors.iter().map(|w| limit.map_or(*w, |c| w.min(c))).collect();
    // the folder itself is shortened only when even the shortened ancestors leave no room for it
    let room = available - seps - widths.iter().sum::<f32>();
    widths.push(last.min(room.max(min)));

    let total = widths.iter().sum::<f32>() + seps;
    let (rect, _) = ui.allocate_exact_size(vec2(total, height), Sense::hover());
    let mut clicked = None;
    let mut x = rect.left();
    for (i, ((step, name), width)) in steps.iter().zip(&widths).enumerate() {
        let item = Rect::from_min_size(pos2(x, rect.top()), vec2(*width, height));
        x += width;
        let response = ui.interact(item, id.with(i), Sense::click());
        events.see(&response, &step.to_string_lossy(), true);
        if response.clicked() {
            clicked = Some(step.to_string_lossy().into_owned());
        }
        let shortened = *width < full[i] - 0.5;
        let is_last = i == steps.len() - 1;
        let color = if is_last { ui.visuals().strong_text_color() } else { text_color };

        // a shortened folder under the mouse is drawn in full above its neighbours
        let (painter, item) = if response.hovered() && shortened {
            let mut wide = Rect::from_min_size(item.min, vec2(full[i], height));
            let clip = ui.clip_rect();
            if wide.right() > clip.right() {
                wide = wide.translate(vec2(clip.right() - wide.right(), 0.0));
            }
            let painter = ui.ctx().layer_painter(LayerId::new(Order::Foreground, id.with("full")));
            painter.rect_filled(wide, 5.0, ui.visuals().window_fill);
            (painter, wide)
        } else {
            (ui.painter().clone(), item)
        };
        if response.hovered() {
            painter.rect_filled(item, 5.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let mut left = item.left() + pad;
        if with_icons {
            paint_icon(ui, &painter, step, Rect::from_center_size(pos2(left + side / 2.0, item.center().y), vec2(side, side)));
            left += side + gap;
        }
        let text = galley(ui, name, item.right() - pad - left, color, TextStyle::Body);
        painter.galley(pos2(left, item.center().y - text.size().y / 2.0), text, color);

        if !is_last {
            ui.painter().galley(
                pos2(x + 3.0, rect.center().y - separator.size().y / 2.0),
                separator.clone(),
                weak,
            );
            x += sep_w;
        }
    }
    events.run(ui)?;
    Ok(clicked)
}

/// Shows the name of a folder as a title that is also a menu of its ancestors, like the title of
/// a Finder window: under the mouse the icon of the folder and the arrow of a menu show, and a
/// click opens the list of the folder and its ancestors up to the root, the folder checked.
/// Choosing one returns its path.
///
/// Args:
///     path (str): the folder shown, ~ is the home folder
///
/// Keyword args:
///     size (float): size of the text. Default is 17
///
///     root (str): the last ancestor of the list. Default is the root of the disk
///
///     id (str): id of the combo, needed if there are several in the same ui
///
///     on_right_click (callable): called with the path of the folder when the title is
///         right-clicked
///
///     on_middle_click (callable): called with the path of the folder when the title is clicked
///         with the middle button
///
///     context_menu (callable): called with the path of the folder on a right click on the title,
///         inside a menu: build the menu in it. A left click opens the list of the ancestors
///
/// Returns the path chosen in the list during this frame, None otherwise.
///
/// Example::
///
///     folder = Str(str(Path.home() / "Documents"))
///
///     def update_func(ctx):
///         chosen = path_combo(folder.value)
///         if chosen:
///             folder.value = chosen
#[pyfunction]
#[pyo3(signature = (path, **kwargs))]
unsafe fn path_combo(path: PathBuf, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Option<String>> {
    let path = expand(&path);
    let mut events = Events::read(kwargs)?;
    let root = kwarg::<PathBuf>(kwargs, "root")?.map(|r| expand(&r));
    let size = kwarg::<f32>(kwargs, "size")?.unwrap_or(17.0);
    let ui = current_ui(&UI)?;
    let id: Id = ui.make_persistent_id(("path_combo", kwarg::<String>(kwargs, "id")?));

    let mut steps = steps(&path, root.as_deref());
    let Some((_, name)) = steps.last().cloned() else { return Ok(None) };
    steps.reverse(); // the folder first, then its parents

    let font = FontId::proportional(size);
    let strong = ui.visuals().strong_text_color();
    let title = ui.painter().layout_no_wrap(name.clone(), font.clone(), strong);
    let side = (size * 1.05).round();
    let (pad, gap, chevron) = (6.0, 6.0, 12.0);
    let height = (title.size().y + 8.0).max(side + 8.0);
    // the room of the icon and of the arrow is always kept, so nothing moves under the mouse
    let width = pad + side + gap + title.size().x + gap + chevron + pad;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    // the left click opens the list: only the other buttons call functions
    events.see(&response, &path.to_string_lossy(), false);

    let popup = egui::Popup::menu(&response).id(id.with("menu"));
    let open = egui::Popup::is_id_open(ui.ctx(), id.with("menu"));
    let painter = ui.painter();
    let text_y = rect.center().y - title.size().y / 2.0;
    if response.hovered() || open {
        painter.rect_filled(rect, 8.0, ui.visuals().widgets.hovered.weak_bg_fill);
        let icon = Rect::from_center_size(pos2(rect.left() + pad + side / 2.0, rect.center().y), vec2(side, side));
        paint_icon(ui, painter, &path, icon);
        painter.galley(pos2(icon.right() + gap, text_y), title.clone(), strong);
        paint_chevron(painter, pos2(rect.right() - pad - chevron / 2.0, rect.center().y), ui.visuals().text_color());
    } else {
        painter.galley(pos2(rect.left() + pad, text_y), title.clone(), strong);
    }

    // the list of the folder and its ancestors
    let row_h = 26.0;
    let body = TextStyle::Body.resolve(ui.style());
    let list_w = steps
        .iter()
        .map(|(_, n)| ui.painter().layout_no_wrap(n.clone(), body.clone(), strong).size().x)
        .fold(0.0, f32::max)
        + 22.0 + 18.0 + 8.0 + 24.0;
    let mut chosen = None;
    popup.width(list_w.max(width)).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, (step, step_name)) in steps.iter().enumerate() {
            let (row, row_response) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
            let hovered = row_response.hovered();
            let color = if hovered { ui.visuals().selection.stroke.color } else { ui.visuals().text_color() };
            if hovered {
                ui.painter().rect_filled(row, 6.0, ui.visuals().selection.bg_fill);
            }
            if i == 0 {
                ui.painter().text(pos2(row.left() + 11.0, row.center().y), Align2::CENTER_CENTER, "✓", body.clone(), color);
            }
            let icon = Rect::from_center_size(pos2(row.left() + 22.0 + 9.0, row.center().y), vec2(18.0, 18.0));
            paint_icon(ui, ui.painter(), step, icon);
            ui.painter().text(pos2(icon.right() + 8.0, row.center().y), Align2::LEFT_CENTER, step_name, body.clone(), color);
            if row_response.clicked() && i > 0 {
                chosen = Some(step.to_string_lossy().into_owned());
            }
        }
    });
    events.run(ui)?;
    Ok(chosen)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(path_breadcrumb, m)?)?;
    m.add_function(wrap_pyfunction!(path_combo, m)?)?;
    Ok(())
}
