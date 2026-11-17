//! Tags: a list of tags with several levels ("projet/web"), each level with its own color.
//!
//! `Tags` is the state (the tags and the colors), `tags_edit` draws it: a chip per tag showing its
//! last level, its full path in the colors of its levels on hover, a field to add one.

use std::collections::{BTreeMap, HashMap};

use eframe::egui::{self, vec2, Color32, Id, Rect, Sense, Stroke, StrokeKind, TextStyle};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::style::extract_color;
use crate::{current_ui, kwarg, UI};

/// The colors given to the levels that have none, chosen from the path so they stay the same
const PALETTE: [u32; 10] = [
    0x3b82f6, 0x10b981, 0xf97316, 0x8b5cf6, 0xef4444, 0x14b8a6, 0xeab308, 0xec4899, 0x6366f1, 0x84cc16,
];

fn hex(value: u32) -> Color32 {
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

/// A color with most of its saturation taken away: the color of a muted tag (e.g. inherited)
fn muted(color: Color32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let gray = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    let mix = |c: u8| (c as f32 + (gray - c as f32) * 0.65).round() as u8;
    Color32::from_rgba_unmultiplied(mix(r), mix(g), mix(b), a)
}

fn to_hex(color: Color32) -> String {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// "a / b//c " -> "a/b/c": the levels without the spaces around them nor the empty ones
fn normalize(tag: &str) -> String {
    tag.split('/').map(str::trim).filter(|level| !level.is_empty()).collect::<Vec<_>>().join("/")
}

/// The paths of the levels of a tag: "a/b/c" -> ["a", "a/b", "a/b/c"]
fn level_paths(tag: &str) -> Vec<String> {
    let mut paths = vec![];
    for (index, _) in tag.match_indices('/') {
        paths.push(tag[..index].to_owned());
    }
    paths.push(tag.to_owned());
    paths
}

fn last_level(tag: &str) -> &str {
    tag.rsplit('/').next().unwrap_or(tag)
}

/// A list of tags and their colors. Create it once, outside the update function, and draw it with
/// tags_edit().
///
/// A tag has one or more levels separated by "/": "projet/web/front". Only its last level is shown
/// ("front"); the mouse over it shows the full path. Each level has its own color, kept by its
/// path: "projet" and "projet/web" can have different colors, and the full path is shown with the
/// color of each of its levels. A level without a color gets one chosen from its path.
///
/// Args:
///     tags (list[str]): the tags at the start
///
///     colors (dict[str, color]): the colors of levels, by path, e.g. {"projet": "#3b82f6",
///         "projet/web": (16, 185, 129)}
///
/// Usage::
///
///     tags = Tags(["projet/web", "perso"], colors={"projet": "#3b82f6"})
///
///     def update_func(ctx):
///         if tags_edit(tags):
///             print(tags.tags)
#[pyclass]
pub(crate) struct Tags {
    tags: Vec<String>,
    colors: HashMap<String, Color32>,
    /// what is typed in the field of the widget
    input: String,
}

impl Tags {
    fn color_of(&self, path: &str) -> Color32 {
        self.colors.get(path).copied().unwrap_or_else(|| {
            let hash = path.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32);
            hex(PALETTE[hash as usize % PALETTE.len()])
        })
    }

    fn insert(&mut self, tag: &str) -> bool {
        let tag = normalize(tag);
        if tag.is_empty() || self.tags.contains(&tag) {
            return false;
        }
        self.tags.push(tag);
        true
    }
}

fn read_colors(colors: &Bound<'_, PyDict>) -> PyResult<HashMap<String, Color32>> {
    let mut map = HashMap::new();
    for (path, color) in colors.iter() {
        let path = normalize(&path.extract::<String>()?);
        if path.is_empty() {
            return Err(PyValueError::new_err("a color is given to an empty tag"));
        }
        map.insert(path, extract_color(&color)?);
    }
    Ok(map)
}

#[pymethods]
impl Tags {
    #[new]
    #[pyo3(signature = (tags=None, colors=None))]
    fn new(tags: Option<Vec<String>>, colors: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let mut state = Tags {
            tags: vec![],
            colors: colors.map(read_colors).transpose()?.unwrap_or_default(),
            input: String::new(),
        };
        for tag in tags.unwrap_or_default() {
            state.insert(&tag);
        }
        Ok(state)
    }

    /// The tags, full paths, in the order they were added. Setting it replaces them.
    #[getter]
    fn get_tags(&self) -> Vec<String> {
        self.tags.clone()
    }

    #[setter]
    fn set_tags(&mut self, tags: Vec<String>) {
        self.tags.clear();
        for tag in tags {
            self.insert(&tag);
        }
    }

    /// The colors given to levels, by path, as "#rrggbb": what to save to restore them. The
    /// levels without a color are not in it. Setting it replaces them.
    #[getter]
    fn get_colors(&self) -> BTreeMap<String, String> {
        self.colors.iter().map(|(path, color)| (path.clone(), to_hex(*color))).collect()
    }

    #[setter]
    fn set_colors(&mut self, colors: &Bound<'_, PyDict>) -> PyResult<()> {
        self.colors = read_colors(colors)?;
        Ok(())
    }

    /// Adds a tag. False when it is empty or already there.
    ///
    /// Args:
    ///     tag (str): the full path, e.g. "projet/web"
    fn add(&mut self, tag: &str) -> bool {
        self.insert(tag)
    }

    /// Removes a tag. False when it was not there.
    ///
    /// Args:
    ///     tag (str): the full path
    fn remove(&mut self, tag: &str) -> bool {
        let tag = normalize(tag);
        let before = self.tags.len();
        self.tags.retain(|t| *t != tag);
        self.tags.len() != before
    }

    /// The color of a level, "#rrggbb": the one given, or the one chosen from its path.
    ///
    /// Args:
    ///     path (str): the path of the level, e.g. "projet" or "projet/web"
    fn color(&self, path: &str) -> String {
        to_hex(self.color_of(&normalize(path)))
    }

    /// Gives a color to a level; None goes back to the one chosen from its path.
    ///
    /// Args:
    ///     path (str): the path of the level, e.g. "projet" or "projet/web"
    ///
    ///     color (tuple | str | None): (r, g, b), (r, g, b, a) or "#rrggbb[aa]"
    #[pyo3(signature = (path, color))]
    fn set_color(&mut self, path: &str, color: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let path = normalize(path);
        match color {
            Some(color) => {
                self.colors.insert(path, extract_color(color)?);
            }
            None => {
                self.colors.remove(&path);
            }
        }
        Ok(())
    }

    fn __len__(&self) -> usize {
        self.tags.len()
    }

    fn __contains__(&self, tag: &str) -> bool {
        self.tags.contains(&normalize(tag))
    }
}

/// The full path of a tag, each level in its color, separated by weak slashes
fn path_ui(ui: &mut egui::Ui, tags: &Tags, tag: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let levels = level_paths(tag);
        for (index, path) in levels.iter().enumerate() {
            if index > 0 {
                ui.weak("/");
            }
            let color = tags.color_of(path);
            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.0, color);
            ui.label(egui::RichText::new(last_level(path)).color(color));
        }
    });
}

/// The name of a level and a row of colors to pick from, "Auto" to go back to the one chosen from
/// its path. Some(the color picked) when one was clicked.
fn color_row(ui: &mut egui::Ui, tags: &Tags, path: &str) -> Option<Option<Color32>> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.label(last_level(path));
        for value in PALETTE {
            let swatch = hex(value);
            let (spot, response) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
            ui.painter().circle_filled(spot.center(), if response.hovered() { 7.5 } else { 6.5 }, swatch);
            if tags.color_of(path) == swatch {
                ui.painter().circle_stroke(spot.center(), 8.0, Stroke::new(1.5, ui.visuals().text_color()));
            }
            if response.clicked() {
                picked = Some(Some(swatch));
            }
        }
        if tags.colors.contains_key(path) && ui.small_button("Auto").clicked() {
            picked = Some(None);
        }
    });
    picked
}

/// The text of the chips: Body, or Small for the small ones (e.g. in a row of a list)
fn chip_style(small: bool) -> TextStyle {
    if small {
        TextStyle::Small
    } else {
        TextStyle::Body
    }
}

/// The background of the chips, of the empty tag and of the list: the one of a window pulled a
/// little toward the text, a grey that is not as black as the window in the dark theme
fn soft_fill(ui: &egui::Ui) -> Color32 {
    let visuals = ui.visuals();
    // lighter in the light theme, where the same amount of text color would look dirty
    let amount = if visuals.dark_mode { 0.14 } else { 0.03 };
    visuals.window_fill.to_opaque().lerp_to_gamma(visuals.text_color(), amount)
}

/// What can be done to a tag from its chip
enum ChipAction {
    Remove,
    Color(String, Option<Color32>),
}

/// The width of the chip of a tag, with the room of its cross when it can be removed
fn chip_width(ui: &egui::Ui, tag: &str, removable: bool, small: bool) -> f32 {
    let font = chip_style(small).resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(last_level(tag).to_owned(), font, Color32::WHITE);
    let height = galley.size().y + if small { 2.0 } else { 6.0 };
    let (pad, dot) = if small { (6.0, 6.0) } else { (8.0, 8.0) };
    let cross = if removable { height.min(14.0) } else { 0.0 };
    pad + dot + 6.0 + galley.size().x + cross + pad
}

/// The chip of a tag: a dot and the last level in a pill of its color, a cross to remove it
#[allow(clippy::too_many_arguments)]
fn chip(
    ui: &mut egui::Ui,
    tags: &Tags,
    tag: &str,
    editable: bool,
    small: bool,
    dim: bool,
    id: Id,
) -> Option<ChipAction> {
    let color = if dim { muted(tags.color_of(tag)) } else { tags.color_of(tag) };
    let font = chip_style(small).resolve(ui.style());
    // the text and the cross in the color of the outline
    // the text and the cross: the color pulled toward the text of the theme, so it reads on the
    // tint of the chip in the light and in the dark theme; the outline keeps the color
    let edge = color.lerp_to_gamma(ui.visuals().strong_text_color(), 0.45);
    let galley = ui.painter().layout_no_wrap(last_level(tag).to_owned(), font, edge);
    let height = galley.size().y + if small { 2.0 } else { 6.0 };
    let (pad, dot) = if small { (6.0, 6.0) } else { (8.0, 8.0) };
    let cross = if editable { height.min(14.0) } else { 0.0 };
    let width = pad + dot + 6.0 + galley.size().x + cross + pad;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let cross_rect =
        Rect::from_center_size(egui::pos2(rect.right() - pad - cross / 2.0 + 2.0, rect.center().y), vec2(14.0, 14.0));
    let cross_response = editable.then(|| ui.interact(cross_rect, id.with("remove"), Sense::click()));

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let hovered = response.hovered();
        // opaque: the tint of the color over the background of the window, so a chip stays readable
        // over a selected row
        let base = soft_fill(ui);
        let fill = base.lerp_to_gamma(color, if hovered { 0.28 } else { 0.18 });
        painter.rect(rect, height / 2.0, fill, Stroke::new(1.2, color), StrokeKind::Inside);
        let dot_center = egui::pos2(rect.left() + pad + dot / 2.0, rect.center().y);
        painter.circle_filled(dot_center, dot / 2.0, color);
        let text_pos = egui::pos2(dot_center.x + dot / 2.0 + 6.0, rect.center().y - galley.size().y / 2.0);
        painter.galley(text_pos, galley, edge);
        if let Some(cross) = &cross_response {
            let c = cross_rect.center();
            let r = 3.5;
            let stroke_color = if cross.hovered() { color } else { edge };
            if cross.hovered() {
                painter.circle_filled(c, 7.0, color.gamma_multiply(0.35));
            }
            let stroke = Stroke::new(1.4, stroke_color);
            painter.line_segment([c + vec2(-r, -r), c + vec2(r, r)], stroke);
            painter.line_segment([c + vec2(-r, r), c + vec2(r, -r)], stroke);
        }
    }

    let mut action = None;
    if cross_response.as_ref().is_some_and(|cross| cross.clicked()) {
        action = Some(ChipAction::Remove);
    }
    let response = response.on_hover_ui(|ui| path_ui(ui, tags, tag));
    if editable {
        response.context_menu(|ui| {
            // a row of colors for each level of the tag: its own one changes only that level
            for path in level_paths(tag) {
                if let Some(color) = color_row(ui, tags, &path) {
                    action = Some(ChipAction::Color(path, color));
                }
            }
            ui.separator();
            if ui.button("Remove").clicked() {
                action = Some(ChipAction::Remove);
                ui.close();
            }
        });
    }
    action
}

/// The badge of the tags that are not shown, "+5": the mouse over it lists them with their full
/// paths, a click shows them all. With expanded, a "−" that shows the first ones again.
fn more_badge(ui: &mut egui::Ui, tags: &Tags, hidden: &[String], expanded: bool, small: bool, id: Id) {
    let text = if expanded { "−".to_owned() } else { format!("+{}", hidden.len()) };
    let font = chip_style(small).resolve(ui.style());
    let height = ui.text_style_height(&chip_style(small)) + if small { 2.0 } else { 6.0 };
    let weak = ui.visuals().weak_text_color();
    let galley = ui.painter().layout_no_wrap(text, font, weak);
    let size = vec2((galley.size().x + 16.0).max(height), height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if response.hovered() {
            ui.visuals().widgets.hovered.weak_bg_fill
        } else {
            ui.visuals().widgets.inactive.weak_bg_fill
        };
        ui.painter().rect(rect, height / 2.0, fill, Stroke::new(1.0, weak.gamma_multiply(0.4)), StrokeKind::Inside);
        ui.painter().galley(rect.center() - galley.size() / 2.0, galley, weak);
    }
    let response = if expanded {
        response.on_hover_text("Show the first ones only")
    } else {
        response.on_hover_ui(|ui| {
            for tag in hidden {
                path_ui(ui, tags, tag);
            }
        })
    };
    if response.clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(id.with("expanded"), !expanded));
    }
}

/// Shows tags as chips, each one with its last level, its full path in the colors of its levels
/// on hover. With editable (the default) a cross removes a tag, a right click changes the color of
/// each of its levels, and a field adds one: "projet/web" then Enter. Backspace in the empty field
/// removes the last tag.
///
/// Args:
///     tags (Tags): the state, changed by the widget
///
/// Keyword args:
///     editable (bool): the cross, the menu and the field, True by default
///
///     hint_text (str): the text of the empty field, "Add a tag" by default
///
///     suggestions (list[str]): tags offered under the field while typing, e.g. all the known ones;
///         those already there are left out
///
///     field_width (float): the width of the field, 120 by default
///
///     on_hover (bool): the chips are read only until the mouse is over the widget, e.g. the cell
///         of a row: then the crosses show and an empty tag, a field in a pill like the chips with a
///         chevron, to type or choose a tag. The pill stays while it has the focus. What is typed
///         is kept by the widget (give each row its own id). The tags in muted have no cross.
///         False by default
///
///     dropdown (bool): the field shows the list of all the suggestions as soon as it has the
///         focus, filtered by what is typed, instead of only while typing. The keys Down and Up
///         move a highlighted row (the list scrolls with it), Enter adds it, Escape clears what is
///         typed. False by default
///
///     compact (bool): only a dot per tag in its color, the full path on hover, e.g. in a row
///         of a list; read only
///
///     dot_size (float): the diameter of the dots of compact, 10 by default
///
///     muted (list[str]): tags drawn with their colors desaturated, e.g. those an item inherits
///
///     small (bool): smaller chips, with the small text, e.g. in a row of a list or of a tree
///
///     fit (bool): shows as many tags as the room of the ui allows and the others in the "+5"
///         badge, e.g. in a cell of a table; the room of the crosses and of the field is kept
///         even when they do not show. Replaces max_visible. False by default
///
///     max_visible (int): with more tags, only the first ones are shown and then "+5": the mouse
///         over it lists the others, a click shows them all (and "−" shows the first ones again)
///
///     id (str): to tell apart two widgets of the same Tags
///
/// Returns:
///     bool: True when a tag was added or removed, or a color changed
///
/// Example::
///
///     tags = Tags(["projet/web", "perso"])
///     known = ["projet/web", "projet/app", "perso", "perso/santé"]
///
///     def update_func(ctx):
///         if tags_edit(tags, suggestions=known):
///             save(tags.tags, tags.colors)
///         tags_edit(tags, compact=True, id="row")  # the dots only
#[pyfunction]
#[pyo3(signature = (tags, **kwargs))]
unsafe fn tags_edit(tags: &Bound<'_, Tags>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<bool> {
    let ui = current_ui(&UI)?;
    let editable = kwarg::<bool>(kwargs, "editable")?.unwrap_or(true);
    let hint = kwarg::<String>(kwargs, "hint_text")?.unwrap_or_else(|| "Add a tag".to_owned());
    let suggestions = kwarg::<Vec<String>>(kwargs, "suggestions")?.unwrap_or_default();
    let field_width = kwarg::<f32>(kwargs, "field_width")?.unwrap_or(120.0);
    let id = match kwarg::<String>(kwargs, "id")? {
        Some(id) => Id::unique(("pyegui-tags", id)),
        None => Id::unique(("pyegui-tags", tags.as_ptr() as usize)),
    };
    // with max_visible, the first tags and a badge for the others, unless it was clicked
    let max_visible = kwarg::<usize>(kwargs, "max_visible")?;
    let small = kwarg::<bool>(kwargs, "small")?.unwrap_or(false);
    let muted_tags: std::collections::HashSet<String> =
        kwarg::<Vec<String>>(kwargs, "muted")?.unwrap_or_default().iter().map(|t| normalize(t)).collect();
    // on_hover: the chips are read only until the mouse is over the widget (or its field has the
    // focus): then the crosses and the empty tag, a field that looks like a tag, show
    let on_hover = kwarg::<bool>(kwargs, "on_hover")?.unwrap_or(false);
    let dropdown = kwarg::<bool>(kwargs, "dropdown")?.unwrap_or(false);
    let field_id = id.with("field");
    let editing = editable
        && (!on_hover
            || ui.rect_contains_pointer(ui.max_rect().intersect(ui.clip_rect()))
            || ui.ctx().memory(|m| m.has_focus(field_id)));
    let expanded = ui.ctx().data(|d| d.get_temp::<bool>(id.with("expanded"))).unwrap_or(false);
    let count = tags.borrow().tags.len();
    // fit: as many chips as the room allows, the others in the "+N" badge. The room of the empty
    // tag and of the crosses is kept even when they do not show, so the chips do not come and go
    // with the mouse
    let fit = kwarg::<bool>(kwargs, "fit")?.unwrap_or(false);
    let max_visible = if fit && !kwarg::<bool>(kwargs, "compact")?.unwrap_or(false) {
        let state = tags.borrow();
        let reserved = if editable { field_width + 16.0 + 6.0 } else { 0.0 };
        let room = (ui.available_width() - reserved).max(0.0);
        let (mut used, mut fitting) = (0.0, 0);
        for (i, tag) in state.tags.iter().enumerate() {
            let width = chip_width(ui, tag, editable, small) + 6.0;
            let badge = if i + 1 < count { 40.0 } else { 0.0 };
            if used + width + badge > room {
                break;
            }
            used += width;
            fitting += 1;
        }
        Some(fitting.max(1))
    } else {
        max_visible
    };
    let shown = match max_visible {
        Some(max) if count > max && !expanded => max,
        _ => count,
    };
    let with_badge = max_visible.is_some_and(|max| count > max);

    if kwarg::<bool>(kwargs, "compact")?.unwrap_or(false) {
        let size = kwarg::<f32>(kwargs, "dot_size")?.unwrap_or(10.0).max(2.0);
        let state = tags.borrow();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(size * 0.4, size * 0.4);
            for tag in &state.tags[..shown] {
                let (spot, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                let color = state.color_of(tag);
                let color = if muted_tags.contains(tag) { muted(color) } else { color };
                ui.painter().circle_filled(spot.center(), size / 2.0, color);
                if response.hovered() {
                    ui.painter().circle_stroke(spot.center(), size / 2.0 + 1.5, Stroke::new(1.0, color.gamma_multiply(0.6)));
                }
                response.on_hover_ui(|ui| path_ui(ui, &state, tag));
            }
            if with_badge {
                more_badge(ui, &state, &state.tags[shown..], expanded, true, id);
            }
        });
        return Ok(false);
    }

    let mut state = tags.borrow_mut();
    let mut changed = false;
    // what is typed: with on_hover it is kept by the widget, so each row of a list has its own
    let input_id = id.with("input");
    let mut input = if on_hover {
        ui.ctx().data_mut(|d| d.get_temp::<String>(input_id)).unwrap_or_default()
    } else {
        std::mem::take(&mut state.input)
    };

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        // what the chips ask for, applied once they are drawn
        let actions: Vec<(String, ChipAction)> = state.tags[..shown]
            .iter()
            .filter_map(|tag| {
                chip(ui, &state, tag, editing && !muted_tags.contains(tag), small, muted_tags.contains(tag), id.with(tag))
                    .map(|action| (tag.clone(), action))
            })
            .collect();
        if with_badge {
            more_badge(ui, &state, &state.tags[shown..], expanded, small, id);
        }
        for (tag, action) in actions {
            match action {
                ChipAction::Remove => state.tags.retain(|t| *t != tag),
                ChipAction::Color(path, Some(color)) => {
                    state.colors.insert(path, color);
                }
                ChipAction::Color(path, None) => {
                    state.colors.remove(&path);
                }
            }
            changed = true;
        }
        if !editing {
            return;
        }

        // the keyboard in the list: arrows move the highlighted row, Enter picks it, Escape clears
        // what is typed. The list is the one of the last frame (it is drawn after the field)
        let highlight_id = id.with("highlight");
        let count_id = id.with("count");
        let listed = ui.ctx().data(|d| d.get_temp::<usize>(count_id)).unwrap_or(0);
        let mut highlight = ui.ctx().data(|d| d.get_temp::<Option<usize>>(highlight_id)).flatten();
        let mut moved = false;
        if listed > 0 && ui.ctx().memory(|m| m.has_focus(field_id)) {
            let none = egui::Modifiers::NONE;
            if ui.input_mut(|i| i.consume_key(none, egui::Key::ArrowDown)) {
                highlight = Some(highlight.map_or(0, |h| (h + 1).min(listed - 1)));
                moved = true;
            }
            if ui.input_mut(|i| i.consume_key(none, egui::Key::ArrowUp)) {
                highlight = Some(highlight.map_or(listed - 1, |h| h.saturating_sub(1)));
                moved = true;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                input.clear();
                highlight = None;
            }
        }
        let typed_before = input.clone();
        let field = if on_hover {
            // the empty tag: a pill like the chips, with the field in it and a chevron that opens
            // the list
            let height = ui.text_style_height(&chip_style(small)) + if small { 2.0 } else { 6.0 };
            let chevron = 16.0;
            let (rect, _) = ui.allocate_exact_size(vec2(field_width + chevron, height), Sense::hover());
            let weak = ui.visuals().weak_text_color();
            let focused = ui.ctx().memory(|m| m.has_focus(field_id));
            ui.painter().rect_filled(rect, height / 2.0, soft_fill(ui));
            let inner = Rect::from_min_max(
                rect.min + vec2(if small { 10.0 } else { 12.0 }, 0.0),
                egui::pos2(rect.right() - chevron, rect.bottom()),
            );
            // the field centered in the pill, as high as its text
            let field = ui
                .scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut input)
                                .id(field_id)
                                .hint_text(hint)
                                .frame(egui::Frame::NONE)
                                .margin(vec2(0.0, 0.0))
                                .desired_width(inner.width())
                                .font(chip_style(small)),
                        )
                    },
                )
                .inner;
            let edge = if focused { ui.visuals().selection.stroke.color } else { ui.visuals().text_color().gamma_multiply(0.55) };
            ui.painter().rect_stroke(rect, height / 2.0, Stroke::new(1.0, edge), StrokeKind::Inside);
            let toggle = ui.interact(
                Rect::from_min_max(egui::pos2(rect.right() - chevron, rect.top()), rect.max),
                id.with("chevron"),
                Sense::click(),
            );
            let c = toggle.rect.center();
            let color = if toggle.hovered() { ui.visuals().text_color() } else { weak };
            let stroke = Stroke::new(1.3, color);
            ui.painter().line_segment([c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0)], stroke);
            ui.painter().line_segment([c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)], stroke);
            if toggle.clicked() {
                field.request_focus();
            }
            field
        } else {
            let mut field = egui::TextEdit::singleline(&mut input)
                .id(field_id)
                .hint_text(hint)
                .desired_width(field_width)
                .font(chip_style(small));
            if small {
                field = field.margin(vec2(4.0, 1.0)); // as high as the small chips
            }
            field.show(ui).response.response
        };
        // after a tag was added, the field that added it takes the focus back (two widgets can
        // show the same Tags, so it is kept by the id of the widget)
        let refocus_id = id.with("refocus");
        if ui.ctx().data_mut(|d| d.remove_temp::<bool>(refocus_id)).unwrap_or(false) {
            field.request_focus();
        }
        if input != typed_before {
            highlight = None; // the list changed
        }
        // the suggestions that match what is typed, under the field
        let typed = normalize(&input).to_lowercase();
        let matching: Vec<String> = if typed.is_empty() && !dropdown {
            vec![]
        } else {
            suggestions
                .iter()
                .map(|s| normalize(s))
                .filter(|s| !state.tags.contains(s) && s.to_lowercase().contains(&typed))
                .take(if dropdown { 200 } else { 8 })
                .collect()
        };
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter {
            if let Some(chosen) = highlight.and_then(|h| matching.get(h)) {
                // the row chosen with the arrows
                changed |= state.insert(chosen);
                input.clear();
                highlight = None;
                ui.ctx().data_mut(|d| d.insert_temp(refocus_id, true));
            } else if !input.trim().is_empty() {
                let typed = std::mem::take(&mut input);
                changed |= state.insert(&typed);
                ui.ctx().data_mut(|d| d.insert_temp(refocus_id, true));
            }
        }
        if field.has_focus() && input.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Backspace)) {
            changed |= state.tags.pop().is_some();
        }

        let popup_id = id.with("suggestions");
        if field.has_focus() && !matching.is_empty() {
            let mut picked = None;
            egui::Area::new(popup_id)
                .order(egui::Order::Foreground)
                .fixed_pos(field.rect.left_bottom() + vec2(0.0, 4.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).fill(soft_fill(ui)).show(ui, |ui| {
                        // the scroll bar takes its own room: it must not cover the long names
                        ui.spacing_mut().scroll.floating = false;
                        ui.set_min_width(field_width + 60.0);
                        // the whole list of a dropdown scrolls
                        egui::ScrollArea::vertical().max_height(220.0).auto_shrink([true, true]).show(ui, |ui| {
                        for (index, suggestion) in matching.iter().enumerate() {
                            // the full path in the colors of its levels, over a row that lights up
                            let background = ui.painter().add(egui::Shape::Noop);
                            let row = ui.horizontal(|ui| path_ui(ui, &state, suggestion)).response;
                            let hit = ui.interact(row.rect, popup_id.with(suggestion), Sense::click());
                            let current = highlight == Some(index);
                            if current && moved {
                                row.scroll_to_me(None);
                            }
                            if current {
                                // the row of the keyboard: drawn from the color of the text, so it
                                // shows on a light and on a dark background
                                let visuals = ui.visuals();
                                let text = visuals.text_color();
                                let rect = row.rect.expand2(vec2(4.0, 2.0));
                                let mut shape =
                                    egui::epaint::RectShape::filled(rect, 4.0, text.gamma_multiply(0.16));
                                shape.stroke = Stroke::new(1.5, text.gamma_multiply(0.75));
                                shape.stroke_kind = StrokeKind::Inside;
                                ui.painter().set(background, egui::Shape::Rect(shape));
                            } else if hit.hovered() {
                                let fill = ui.visuals().widgets.hovered.weak_bg_fill;
                                ui.painter().set(background, egui::Shape::rect_filled(row.rect.expand(2.0), 4.0, fill));
                            }
                            // on the press: on the release the field has lost the focus and the
                            // list is gone
                            if hit.is_pointer_button_down_on() {
                                picked = Some(suggestion.clone());
                            }
                        }
                        });
                    });
                });
            if let Some(suggestion) = picked {
                changed |= state.insert(&suggestion);
                input.clear();
                ui.ctx().data_mut(|d| d.insert_temp(refocus_id, true));
                highlight = None;
            }
        }
        // for the next frame: the rows of the list, the one highlighted
        let shown = if field.has_focus() { matching.len() } else { 0 };
        let highlight = highlight.filter(|h| *h < shown);
        ui.ctx().data_mut(|d| {
            d.insert_temp(count_id, shown);
            d.insert_temp(highlight_id, highlight);
        });
    });
    if on_hover {
        ui.ctx().data_mut(|d| d.insert_temp(input_id, input));
    } else {
        state.input = input;
    }
    Ok(changed)
}

// the tree of the tags

/// The state of a tree of tags: the levels folded shut and the one selected. Create it once,
/// outside the update function, and draw it with tag_tree_view().
///
/// The tree has a node for every level of the tags: "projet/web" and "projet/app" make a node
/// "projet" with two children. Each node shows the last level in the color of its path.
///
/// Args:
///     selected (str): the path of the level selected at the start
///
///     folded (list[str]): the paths of the levels folded shut at the start; the others are open
///
/// Usage::
///
///     tree = TagTreeView()
///
///     def update_func(ctx):
///         clicked = tag_tree_view(tree, tags)
///         if clicked:
///             show_items_tagged(clicked)
#[pyclass]
pub(crate) struct TagTreeView {
    /// The path of the level selected, or None
    #[pyo3(get, set)]
    selected: Option<String>,
    folded: std::collections::HashSet<String>,
}

#[pymethods]
impl TagTreeView {
    #[new]
    #[pyo3(signature = (selected=None, folded=None))]
    fn new(selected: Option<String>, folded: Option<Vec<String>>) -> Self {
        TagTreeView {
            selected: selected.map(|s| normalize(&s)).filter(|s| !s.is_empty()),
            folded: folded.unwrap_or_default().iter().map(|s| normalize(s)).collect(),
        }
    }

    /// The paths of the levels folded shut, sorted: what to save to restore them. Setting it
    /// replaces them.
    #[getter]
    fn get_folded(&self) -> Vec<String> {
        let mut folded: Vec<String> = self.folded.iter().cloned().collect();
        folded.sort();
        folded
    }

    #[setter]
    fn set_folded(&mut self, folded: Vec<String>) {
        self.folded = folded.iter().map(|s| normalize(s)).collect();
    }

    /// Opens every level
    fn expand_all(&mut self) {
        self.folded.clear();
    }

    /// Folds every level that has children, among the levels of tags
    ///
    /// Args:
    ///     tags (Tags): the tags of the tree
    fn collapse_all(&mut self, tags: &Tags) {
        for tag in &tags.tags {
            let levels = level_paths(tag);
            self.folded.extend(levels[..levels.len() - 1].iter().cloned());
        }
    }
}

/// The levels of the tags as a tree: the children of each level ("" for the top), sorted by name
fn tag_children(tags: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for tag in tags {
        let mut parent = String::new();
        for path in level_paths(tag) {
            let siblings = children.entry(parent.clone()).or_default();
            if !siblings.contains(&path) {
                siblings.push(path.clone());
            }
            parent = path;
        }
    }
    for siblings in children.values_mut() {
        siblings.sort_by_key(|path| last_level(path).to_lowercase());
    }
    children
}

/// What the right click on a node of the tree offers
#[derive(Clone, Copy)]
struct Menu {
    editable: bool,
    removable: bool,
}

/// What can be done to a node of the tree
enum NodeAction {
    Select(String),
    Toggle(String),
    Color(String, Option<Color32>),
    Remove(String),
}

/// A row of the tree: the arrow of a level with children, the dot, the name, the count at the right
#[allow(clippy::too_many_arguments)]
fn tree_row(
    ui: &mut egui::Ui,
    tags: &Tags,
    view: &TagTreeView,
    path: &str,
    depth: usize,
    has_children: bool,
    count: Option<i64>,
    menu: Menu,
    actions: &mut Vec<NodeAction>,
) {
    let font = TextStyle::Body.resolve(ui.style());
    let height = ui.text_style_height(&TextStyle::Body) + 8.0;
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let indent = 14.0 * depth as f32;
    let arrow_rect = Rect::from_center_size(egui::pos2(rect.left() + indent + 8.0, rect.center().y), vec2(14.0, height));
    let arrow = has_children.then(|| ui.interact(arrow_rect, response.id.with("arrow"), Sense::click()));
    let color = tags.color_of(path);
    let selected = view.selected.as_deref() == Some(path);
    let folded = view.folded.contains(path);

    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, 5.0, visuals.selection.bg_fill);
        } else if response.hovered() {
            painter.rect_filled(rect, 5.0, visuals.widgets.hovered.weak_bg_fill);
        }
        let text_color = if selected { visuals.selection.stroke.color } else { visuals.text_color() };
        if has_children {
            // a triangle, pointing right when folded, down when open
            let c = arrow_rect.center();
            let r = 3.5;
            let points = if folded {
                vec![c + vec2(-r * 0.6, -r), c + vec2(r * 0.9, 0.0), c + vec2(-r * 0.6, r)]
            } else {
                vec![c + vec2(-r, -r * 0.6), c + vec2(r, -r * 0.6), c + vec2(0.0, r * 0.9)]
            };
            let arrow_color = text_color.gamma_multiply(if arrow.as_ref().is_some_and(|a| a.hovered()) { 1.0 } else { 0.6 });
            painter.add(egui::Shape::convex_polygon(points, arrow_color, Stroke::NONE));
        }
        let dot_center = egui::pos2(rect.left() + indent + 22.0, rect.center().y);
        painter.circle_filled(dot_center, 4.5, color);
        let galley = painter.layout_no_wrap(last_level(path).to_owned(), font.clone(), text_color);
        let text_pos = egui::pos2(dot_center.x + 10.0, rect.center().y - galley.size().y / 2.0);
        let mut right = rect.right() - 8.0;
        if let Some(count) = count {
            let weak = if selected { text_color.gamma_multiply(0.7) } else { visuals.weak_text_color() };
            let count_galley = painter.layout_no_wrap(count.to_string(), font, weak);
            right -= count_galley.size().x;
            painter.galley(egui::pos2(right, rect.center().y - count_galley.size().y / 2.0), count_galley, weak);
            right -= 8.0;
        }
        // the name is cut where the count begins
        painter.with_clip_rect(Rect::from_min_max(rect.min, egui::pos2(right, rect.max.y))).galley(text_pos, galley, text_color);
    }

    if arrow.as_ref().is_some_and(|a| a.clicked()) {
        actions.push(NodeAction::Toggle(path.to_owned()));
    } else if response.double_clicked() && has_children {
        actions.push(NodeAction::Toggle(path.to_owned()));
    } else if response.clicked() {
        actions.push(NodeAction::Select(path.to_owned()));
    }
    let response = response.on_hover_ui(|ui| path_ui(ui, tags, path));
    if menu.editable {
        response.context_menu(|ui| {
            if let Some(color) = color_row(ui, tags, path) {
                actions.push(NodeAction::Color(path.to_owned(), color));
            }
            if !menu.removable {
                return;
            }
            ui.separator();
            let label = if has_children { "Remove with its children" } else { "Remove" };
            if ui.button(label).clicked() {
                actions.push(NodeAction::Remove(path.to_owned()));
                ui.close();
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn tree_rows(
    ui: &mut egui::Ui,
    tags: &Tags,
    view: &TagTreeView,
    children: &BTreeMap<String, Vec<String>>,
    counts: &HashMap<String, i64>,
    parent: &str,
    depth: usize,
    menu: Menu,
    actions: &mut Vec<NodeAction>,
) {
    for path in children.get(parent).into_iter().flatten() {
        let has_children = children.contains_key(path);
        tree_row(ui, tags, view, path, depth, has_children, counts.get(path).copied(), menu, actions);
        if has_children && !view.folded.contains(path) {
            tree_rows(ui, tags, view, children, counts, path, depth + 1, menu, actions);
        }
    }
}

/// Shows the tags as a tree: a node for every level, its last level in the color of its path,
/// the full path on hover. A click selects a level, the arrow (or a double click) folds or opens
/// it. A right click changes the color of the level or removes it from the tags, with the tags
/// under it.
///
/// Args:
///     view (TagTreeView): the state: the folded levels and the selection
///
///     tags (Tags): the tags shown, changed when one is removed or a color changes
///
/// Keyword args:
///     counts (dict[str, int]): a number shown at the right of a level, e.g. the items tagged
///
///     editable (bool): the menu of the right click, True by default
///
///     removable (bool): the Remove entry of that menu, True by default; False keeps only the
///         colors, e.g. when removing a tag would take it off many items
///
/// Returns:
///     str | None: the path of the level clicked during the frame (it is now view.selected)
///
/// Example::
///
///     library = Tags(["projet/web", "projet/app", "perso/santé"])
///     tree = TagTreeView()
///
///     def update_func(ctx):
///         def sidebar():
///             clicked = tag_tree_view(tree, library, counts={"projet": 12, "projet/web": 5})
///             if clicked:
///                 print("show", clicked)
///         left_panel("tags", sidebar)
#[pyfunction]
#[pyo3(signature = (view, tags, **kwargs))]
unsafe fn tag_tree_view(
    view: &Bound<'_, TagTreeView>,
    tags: &Bound<'_, Tags>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<Option<String>> {
    let ui = current_ui(&UI)?;
    let counts = kwarg::<HashMap<String, i64>>(kwargs, "counts")?
        .unwrap_or_default()
        .into_iter()
        .map(|(path, count)| (normalize(&path), count))
        .collect::<HashMap<_, _>>();
    let editable = kwarg::<bool>(kwargs, "editable")?.unwrap_or(true);
    let removable = kwarg::<bool>(kwargs, "removable")?.unwrap_or(true);
    let mut actions = vec![];
    {
        let state = tags.borrow();
        let tree = view.borrow();
        let children = tag_children(&state.tags);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let menu = Menu { editable, removable };
            tree_rows(ui, &state, &tree, &children, &counts, "", 0, menu, &mut actions);
        });
    }

    let mut clicked = None;
    let mut state = tags.borrow_mut();
    let mut tree = view.borrow_mut();
    for action in actions {
        match action {
            NodeAction::Select(path) => {
                tree.selected = Some(path.clone());
                clicked = Some(path);
            }
            NodeAction::Toggle(path) => {
                if !tree.folded.remove(&path) {
                    tree.folded.insert(path);
                }
            }
            NodeAction::Color(path, Some(color)) => {
                state.colors.insert(path, color);
            }
            NodeAction::Color(path, None) => {
                state.colors.remove(&path);
            }
            NodeAction::Remove(path) => {
                let below = format!("{path}/");
                state.tags.retain(|t| *t != path && !t.starts_with(&below));
                if tree.selected.as_deref().is_some_and(|s| s == path || s.starts_with(&below)) {
                    tree.selected = None;
                }
            }
        }
    }
    Ok(clicked)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Tags>()?;
    m.add_class::<TagTreeView>()?;
    m.add_function(wrap_pyfunction!(tag_tree_view, m)?)?;
    m.add_function(wrap_pyfunction!(tags_edit, m)?)?;
    Ok(())
}
