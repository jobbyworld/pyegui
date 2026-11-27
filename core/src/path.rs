//! Widgets that show where a path is: `breadcrumb` (the folders from the root, the ancestors
//! shortened when room is missing) and `combo` (the name of the folder, a menu of its ancestors).
//! Shared by the Python extension (`path_breadcrumb`, `path_combo`) and the Rust applications.

use std::path::{Component, Path, PathBuf};

use egui::{
    self, pos2, vec2, Align2, Color32, FontId, Id, LayerId, Order, Rect, Sense, Stroke, TextStyle, TextWrapMode,
};

use crate::icons::icon_uri;
use crate::watcher::expand;

pub struct BreadcrumbOptions {
    /// the first folder shown, an ancestor of the path (default: the root of the disk)
    pub root: Option<PathBuf>,
    /// the widest it can be
    pub max_width: f32,
    pub icons: bool,
    /// needed when there are several in the same ui
    pub id: String,
}

impl Default for BreadcrumbOptions {
    fn default() -> Self {
        BreadcrumbOptions { root: None, max_width: f32::INFINITY, icons: true, id: String::new() }
    }
}

pub struct ComboOptions {
    pub size: f32,
    pub root: Option<PathBuf>,
    pub max_width: Option<f32>,
    pub id: String,
    /// the icon of the folder and the arrow of the menu are always shown (not only under the mouse)
    pub always_icon: bool,
}

impl Default for ComboOptions {
    fn default() -> Self {
        ComboOptions { size: 17.0, root: None, max_width: None, id: String::new(), always_icon: false }
    }
}

/// What a path widget returns: the path clicked (or chosen in the list of the combo), and the
/// response of each of its elements, to read the other buttons and show a context menu
#[derive(Default)]
pub struct Elements {
    pub clicked: Option<String>,
    pub hits: Vec<(String, egui::Response)>,
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
/// shortened, the longest names first. A click on a folder returns its path.
pub fn breadcrumb(ui: &mut egui::Ui, path: &Path, options: &BreadcrumbOptions) -> Elements {
    let path = expand(path);
    let root = options.root.as_ref().map(|r| expand(r));
    let with_icons = options.icons;
    let id = ui.make_persistent_id(("path_breadcrumb", options.id.clone()));
    let available = options.max_width.min(ui.available_width());

    let steps = steps(&path, root.as_deref());
    if steps.is_empty() {
        return Elements::default();
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

    // never wider than the room given: the ancestors that do not fit even shortened are left out,
    // from the root, and the folder itself is cut last. A breadcrumb wider than max_width would
    // push what follows it out of the row, and a row that measures itself would never settle
    let mut first = 0;
    let mut total = widths.iter().sum::<f32>() + seps;
    while total > available && first < steps.len() - 1 {
        total -= widths[first] + sep_w;
        first += 1;
        let room = available - (total - widths[widths.len() - 1]);
        let end = widths.len() - 1;
        widths[end] = last.min(room.max(min));
        total = widths[first..].iter().sum::<f32>() + sep_w * (steps.len() - 1 - first) as f32;
    }
    if total > available {
        let end = widths.len() - 1;
        widths[end] = (widths[end] - (total - available)).max(0.0);
        total = available.max(0.0);
    }

    let (rect, _) = ui.allocate_exact_size(vec2(total, height), Sense::hover());
    let mut out = Elements::default();
    let mut x = rect.left();
    for (i, ((step, name), width)) in steps.iter().zip(&widths).enumerate().skip(first) {
        let item = Rect::from_min_size(pos2(x, rect.top()), vec2(*width, height));
        x += width;
        let response = ui.interact(item, id.with(i), Sense::click());
        if response.clicked() {
            out.clicked = Some(step.to_string_lossy().into_owned());
        }
        out.hits.push((step.to_string_lossy().into_owned(), response.clone()));
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
    out
}

/// Shows the name of a folder as a title that is also a menu of its ancestors, like the title of
/// a Finder window: a click opens the list of the folder and its ancestors up to the root.
pub fn combo(ui: &mut egui::Ui, path: &Path, options: &ComboOptions) -> Elements {
    let path = expand(path);
    let root = options.root.as_ref().map(|r| expand(r));
    let size = options.size;
    let id: Id = ui.make_persistent_id(("path_combo", options.id.clone()));

    let mut steps = steps(&path, root.as_deref());
    let Some((_, name)) = steps.last().cloned() else { return Elements::default() };
    steps.reverse(); // the folder first, then its parents

    let font = FontId::proportional(size);
    let strong = ui.visuals().strong_text_color();
    let side = (size * 1.05).round();
    let (pad, gap, chevron) = (6.0, 6.0, 12.0);
    let room = options.max_width.unwrap_or_else(|| ui.available_width());
    let text_room = (room - (pad + side + gap + gap + chevron + pad)).max(24.0);
    let mut job = egui::text::LayoutJob::simple_singleline(name.clone(), font.clone(), strong);
    job.wrap.max_width = text_room;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    let title = ui.painter().layout_job(job);
    let cut = title.rows.first().map_or(false, |r| r.ends_with_newline) || title.size().x >= text_room - 0.5;
    let height = (title.size().y + 8.0).max(side + 8.0);
    // the room of the icon and of the arrow is always kept, so nothing moves under the mouse
    let width = pad + side + gap + title.size().x + gap + chevron + pad;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let response = if cut { response.on_hover_text(name.clone()) } else { response };
    let mut out = Elements::default();
    out.hits.push((path.to_string_lossy().into_owned(), response.clone()));

    let popup = egui::Popup::menu(&response).id(id.with("menu"));
    let open = egui::Popup::is_id_open(ui.ctx(), id.with("menu"));
    let painter = ui.painter();
    let text_y = rect.center().y - title.size().y / 2.0;
    if response.hovered() || open || options.always_icon {
        if response.hovered() || open {
            painter.rect_filled(rect, 8.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
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
                ui.painter().rect_filled(row, crate::style::fit_radius(ui.visuals(), 6.0), ui.visuals().selection.bg_fill);
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
    out.clicked = chosen;
    out
}

