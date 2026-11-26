//! Tags: a list of tags with several levels ("projet/web"), each level with its own color.
//!
//! `Tags` is the state (the tags and the colors), `tags_edit` draws it: a chip per tag showing its
//! last level, its full path in the colors of its levels on hover, a field to add one. The state and
//! the drawing are in `pyegui_core::tags`, shared with the Rust applications.

use std::collections::{BTreeMap, HashMap};
use std::ops::{Deref, DerefMut};

use eframe::egui::{self, vec2, Color32, Id, Rect, Sense, Stroke, StrokeKind, TextStyle};
use pyegui_core::tags::{color_row, last_level, level_paths, normalize, paint_tag_icon, path_ui, to_hex, Tags as CoreTags, TagsOptions};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::style::extract_color;
use crate::{current_ui, kwarg, UI};

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
pub(crate) struct Tags(CoreTags);

impl Deref for Tags {
    type Target = CoreTags;
    fn deref(&self) -> &CoreTags {
        &self.0
    }
}

impl DerefMut for Tags {
    fn deref_mut(&mut self) -> &mut CoreTags {
        &mut self.0
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
        Ok(Tags(CoreTags::new(tags.unwrap_or_default(), colors.map(read_colors).transpose()?.unwrap_or_default())))
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
///     pick (bool): the dropdown lists all the suggestions with a check box (checked for the tags that are on)
///         and a click puts a tag on or takes it off while the list stays open, to choose several. False by default
///
///     list_height (float): the preferred height of the list of suggestions, which scrolls beyond it.
///         Default is 220
///
///     compact (bool): only a dot per tag in its color, the full path on hover, e.g. in a row
///         of a list; read only
///
///     dot_size (float): the diameter of the dots of compact, 10 by default
///
///     icon (bool): the tag icon in the color of the tag at the left of a chip, instead of a dot.
///         False by default
///
///     tree (bool): a flat cell: the tags as chips with no field and no outline, an arrow while the mouse is over it;
///         a click opens the tree of `suggestions` (the paths of all the tags, in the order of the tree), an indented
///         row per level with a check box. A field filters them and Enter puts the typed tag on (created when new).
///         The `muted` tags are shown but not changed. False by default
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
    let id = match kwarg::<String>(kwargs, "id")? {
        Some(id) => Id::unique(("pyegui-tags", id)),
        None => Id::unique(("pyegui-tags", tags.as_ptr() as usize)),
    };
    let mut o = TagsOptions::new(id);
    o.editable = kwarg::<bool>(kwargs, "editable")?.unwrap_or(true);
    o.hint_text = kwarg::<String>(kwargs, "hint_text")?.unwrap_or_else(|| "Add a tag".to_owned());
    o.suggestions = kwarg::<Vec<String>>(kwargs, "suggestions")?.unwrap_or_default();
    o.field_width = kwarg::<f32>(kwargs, "field_width")?.unwrap_or(120.0);
    o.max_visible = kwarg::<usize>(kwargs, "max_visible")?;
    o.small = kwarg::<bool>(kwargs, "small")?.unwrap_or(false);
    o.muted = kwarg::<Vec<String>>(kwargs, "muted")?.unwrap_or_default().into_iter().collect();
    o.on_hover = kwarg::<bool>(kwargs, "on_hover")?.unwrap_or(false);
    o.dropdown = kwarg::<bool>(kwargs, "dropdown")?.unwrap_or(false);
    o.pick = kwarg::<bool>(kwargs, "pick")?.unwrap_or(false);
    o.list_height = kwarg::<f32>(kwargs, "list_height")?.unwrap_or(220.0);
    o.fit = kwarg::<bool>(kwargs, "fit")?.unwrap_or(false);
    o.compact = kwarg::<bool>(kwargs, "compact")?.unwrap_or(false);
    o.dot_size = kwarg::<f32>(kwargs, "dot_size")?.unwrap_or(10.0);
    o.icon = kwarg::<bool>(kwargs, "icon")?.unwrap_or(false);
    o.tree = kwarg::<bool>(kwargs, "tree")?.unwrap_or(false);
    let mut state = tags.borrow_mut();
    Ok(pyegui_core::tags::tags_edit(ui, &mut state.0, &o))
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
    /// the level being moved with the mouse
    dragging: Option<String>,
    /// the level whose trash was clicked once: a second click deletes it
    armed: Option<String>,
    /// the levels deleted with the trash, until take_deleted()
    deleted: Vec<String>,
    /// the move asked for with the mouse, until take_moved()
    moved: Option<(String, Option<String>, Option<String>)>,
    /// the state of the check boxes by level ("on", "off" or "mixed"), given at each frame
    states: HashMap<String, String>,
}

#[pymethods]
impl TagTreeView {
    #[new]
    #[pyo3(signature = (selected=None, folded=None))]
    fn new(selected: Option<String>, folded: Option<Vec<String>>) -> Self {
        TagTreeView {
            selected: selected.map(|s| normalize(&s)).filter(|s| !s.is_empty()),
            folded: folded.unwrap_or_default().iter().map(|s| normalize(s)).collect(),
            dragging: None,
            armed: None,
            deleted: vec![],
            moved: None,
            states: HashMap::new(),
        }
    }

    /// The levels deleted with the trash since the last call (see `deletable` of tag_tree_view): the app
    /// removes them from its own data, they are not removed from the Tags.
    fn take_deleted(&mut self) -> Vec<String> {
        std::mem::take(&mut self.deleted)
    }

    /// The move asked for with the mouse since the last call (see `movable` of tag_tree_view), as
    /// (path, parent, before): the level goes under `parent` (None: at the top), before the level `before`
    /// of that parent (None: last). The app moves it in its own data.
    fn take_moved(&mut self) -> Option<(String, Option<String>, Option<String>)> {
        self.moved.take()
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
fn tag_children(tags: &[String], keep_order: bool) -> BTreeMap<String, Vec<String>> {
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
    if !keep_order {
        for siblings in children.values_mut() {
            siblings.sort_by_key(|path| last_level(path).to_lowercase());
        }
    }
    children
}

/// What the right click on a node of the tree offers
#[derive(Clone, Copy)]
struct Menu {
    editable: bool,
    removable: bool,
    /// a trash at the right of the row under the pointer: a first click arms it, a second one deletes the level
    deletable: bool,
    /// the rows can be dragged to move a level
    movable: bool,
    /// the tag icon in the color of the level instead of a dot
    icon: bool,
    /// a check box per row (see `checks` of tag_tree_view)
    check: bool,
}

/// What can be done to a node of the tree
enum NodeAction {
    Select(String),
    Toggle(String),
    Color(String, Option<Color32>),
    Remove(String),
    DragStart(String),
    Arm(String),
    Disarm,
    Delete(String),
}

/// The rows drawn, in order, with their depth: where a level being moved can be dropped
type Rows = Vec<(String, Rect, usize)>;

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
    rows: &mut Rows,
) {
    let font = TextStyle::Body.resolve(ui.style());
    let height = ui.text_style_height(&TextStyle::Body) + 8.0;
    let sense = if menu.movable { Sense::click_and_drag() } else { Sense::click() };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), sense);
    rows.push((path.to_owned(), rect, depth));
    if response.drag_started() && menu.movable {
        actions.push(NodeAction::DragStart(path.to_owned()));
    }
    // the trash: seen while the row is under the pointer (or armed), not while a level is being moved
    let armed = view.armed.as_deref() == Some(path);
    let moving = view.dragging.is_some();
    let trash_rect = Rect::from_center_size(egui::pos2(rect.right() - 16.0, rect.center().y), vec2(24.0, 22.0));
    let show_trash = menu.deletable && ((ui.rect_contains_pointer(rect) && !moving) || armed);
    let trash = show_trash.then(|| ui.interact(trash_rect, response.id.with("trash"), Sense::click()));
    let over_trash = trash.as_ref().is_some_and(|t| t.hovered());
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
        let pre = if menu.check { 20.0 } else { 0.0 };
        if menu.check {
            let state = view.states.get(path).map(String::as_str).unwrap_or("off");
            let boxed = Rect::from_center_size(egui::pos2(rect.left() + indent + 22.0, rect.center().y), vec2(14.0, 14.0));
            if state == "off" {
                painter.rect_stroke(boxed, 3.0, Stroke::new(1.3, text_color.gamma_multiply(0.7)), egui::StrokeKind::Inside);
            } else {
                painter.rect_filled(boxed, 3.0, visuals.hyperlink_color);
                let (c, white) = (boxed.center(), Stroke::new(1.6, Color32::WHITE));
                if state == "mixed" {
                    painter.line_segment([c + vec2(-3.5, 0.0), c + vec2(3.5, 0.0)], white);
                } else {
                    painter.line_segment([c + vec2(-3.5, 0.5), c + vec2(-1.0, 3.0)], white);
                    painter.line_segment([c + vec2(-1.0, 3.0), c + vec2(3.8, -3.0)], white);
                }
            }
        }
        let dot_center = egui::pos2(rect.left() + indent + 22.0 + pre, rect.center().y);
        if menu.icon {
            paint_tag_icon(ui, dot_center, 14.0, color);
        } else {
            painter.circle_filled(dot_center, 4.5, color);
        }
        let galley = painter.layout_no_wrap(last_level(path).to_owned(), font.clone(), text_color);
        let text_pos = egui::pos2(dot_center.x + if menu.icon { 12.0 } else { 10.0 }, rect.center().y - galley.size().y / 2.0);
        let mut right = rect.right() - 8.0;
        if let Some(count) = count.filter(|_| !show_trash) {
            let weak = if selected { text_color.gamma_multiply(0.7) } else { visuals.weak_text_color() };
            let count_galley = painter.layout_no_wrap(count.to_string(), font, weak);
            right -= count_galley.size().x;
            painter.galley(egui::pos2(right, rect.center().y - count_galley.size().y / 2.0), count_galley, weak);
            right -= 8.0;
        }
        // the name is cut where the count (or the trash) begins
        if show_trash {
            right = trash_rect.left() - 4.0;
        }
        painter.with_clip_rect(Rect::from_min_max(rect.min, egui::pos2(right, rect.max.y))).galley(text_pos, galley, text_color);
        if view.dragging.as_deref() == Some(path) {
            painter.rect_filled(rect, 5.0, visuals.panel_fill.gamma_multiply(0.6));
        }
        if show_trash {
            let red = Color32::from_rgb(230, 60, 60);
            let tint = if armed { red } else { visuals.weak_text_color() };
            if armed || over_trash {
                let fill = if armed { Color32::from_rgba_unmultiplied(230, 60, 60, 36) } else { Color32::from_rgba_unmultiplied(127, 127, 127, 40) };
                painter.rect_filled(trash_rect, 6.0, fill);
            }
            // a trash can drawn with lines: a lid, a handle and a body
            let c = trash_rect.center();
            let stroke = Stroke::new(1.3, tint);
            painter.line_segment([c + vec2(-5.0, -3.5), c + vec2(5.0, -3.5)], stroke);
            painter.line_segment([c + vec2(-1.8, -3.5), c + vec2(-1.8, -5.5)], stroke);
            painter.line_segment([c + vec2(1.8, -3.5), c + vec2(1.8, -5.5)], stroke);
            painter.line_segment([c + vec2(-1.8, -5.5), c + vec2(1.8, -5.5)], stroke);
            painter.line_segment([c + vec2(-4.0, -3.5), c + vec2(-3.3, 5.5)], stroke);
            painter.line_segment([c + vec2(4.0, -3.5), c + vec2(3.3, 5.5)], stroke);
            painter.line_segment([c + vec2(-3.3, 5.5), c + vec2(3.3, 5.5)], stroke);
        }
    }
    if let Some(trash) = &trash {
        if trash.clicked() {
            actions.push(if armed { NodeAction::Delete(path.to_owned()) } else { NodeAction::Arm(path.to_owned()) });
        }
        let tip = if armed { "Click again to delete" } else { "Delete" };
        trash.clone().on_hover_text(tip);
    }
    if armed && !over_trash {
        actions.push(NodeAction::Disarm);
    }

    if arrow.as_ref().is_some_and(|a| a.clicked()) {
        actions.push(NodeAction::Toggle(path.to_owned()));
    } else if response.double_clicked() && has_children {
        actions.push(NodeAction::Toggle(path.to_owned()));
    } else if response.clicked() && !over_trash {
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
    rows: &mut Rows,
) {
    for path in children.get(parent).into_iter().flatten() {
        let has_children = children.contains_key(path);
        tree_row(ui, tags, view, path, depth, has_children, counts.get(path).copied(), menu, actions, rows);
        if has_children && !view.folded.contains(path) {
            tree_rows(ui, tags, view, children, counts, path, depth + 1, menu, actions, rows);
        }
    }
}

/// The parent of a path: "a/b/c" -> Some("a/b"), "a" -> None
fn parent_of(path: &str) -> Option<String> {
    path.rsplit_once('/').map(|(parent, _)| parent.to_owned())
}

/// Moving a level with the mouse: dropped on the middle of another it goes under it, on its upper or lower edge
/// it goes before or after it (same level), below the last row it goes to the top. Draws where it goes and
/// returns the move once the button is let go
fn dragging(ui: &mut egui::Ui, view: &mut TagTreeView, rows: &Rows, area: Rect) {
    let Some(moving) = view.dragging.clone() else { return };
    let Some(from) = rows.iter().position(|(p, _, _)| *p == moving) else {
        view.dragging = None;
        return;
    };
    let depth = rows[from].2;
    // the level and what is under it cannot receive it
    let end = rows[from + 1..].iter().position(|(_, _, d)| *d <= depth).map_or(rows.len(), |n| from + 1 + n);
    let inside = |i: usize| i >= from && i < end;
    enum Drop {
        Into(usize),
        Before(usize),
        After(usize),
        TopEnd,
    }
    let pointer = ui.ctx().pointer_latest_pos();
    let mut drop = None;
    if let Some(p) = pointer.filter(|p| area.x_range().contains(p.x)) {
        if let Some((i, (_, rect, _))) = rows.iter().enumerate().find(|(_, (_, r, _))| r.y_range().contains(p.y)) {
            if !inside(i) {
                let f = (p.y - rect.top()) / rect.height();
                drop = Some(if f < 0.25 { Drop::Before(i) } else if f > 0.75 { Drop::After(i) } else { Drop::Into(i) });
            }
        } else if rows.last().is_some_and(|(_, r, _)| p.y > r.bottom() && p.y < r.bottom() + 60.0) {
            drop = Some(Drop::TopEnd);
        }
    }
    if ui.ctx().input(|i| i.pointer.primary_down()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let stroke = Stroke::new(2.0, ui.visuals().selection.bg_fill);
        let indent = |i: usize| 6.0 + 14.0 * rows[i].2 as f32;
        match &drop {
            Some(Drop::Into(i)) => {
                ui.painter().rect_stroke(rows[*i].1, 5.0, stroke, StrokeKind::Inside);
            }
            Some(Drop::Before(i)) => {
                let r = rows[*i].1;
                ui.painter().line_segment([egui::pos2(r.left() + indent(*i), r.top()), egui::pos2(r.right() - 6.0, r.top())], stroke);
            }
            Some(Drop::After(i)) => {
                let r = rows[*i].1;
                ui.painter().line_segment([egui::pos2(r.left() + indent(*i), r.bottom()), egui::pos2(r.right() - 6.0, r.bottom())], stroke);
            }
            Some(Drop::TopEnd) => {
                if let Some((_, r, _)) = rows.last() {
                    ui.painter().line_segment([egui::pos2(r.left() + 6.0, r.bottom() + 2.0), egui::pos2(r.right() - 6.0, r.bottom() + 2.0)], stroke);
                }
            }
            None => {}
        }
        if let Some(p) = pointer {
            // the name follows the pointer
            let layer = egui::LayerId::new(egui::Order::Tooltip, Id::unique("tag-tree-ghost"));
            let font = TextStyle::Body.resolve(ui.style());
            ui.ctx().layer_painter(layer).text(p + vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, last_level(&moving), font, ui.visuals().text_color());
        }
        return;
    }
    // let go: the level goes where the line was
    view.dragging = None;
    let Some(drop) = drop else { return };
    // the next level of the same parent, after a level and what is under it
    let after = |i: usize| {
        let stop = rows[i + 1..].iter().position(|(_, _, d)| *d <= rows[i].2).map_or(rows.len(), |n| i + 1 + n);
        rows.get(stop).filter(|(p, _, d)| *d == rows[i].2 && parent_of(p) == parent_of(&rows[i].0)).map(|(p, _, _)| p.clone())
    };
    let (parent, before) = match drop {
        Drop::Into(i) => (Some(rows[i].0.clone()), None),
        Drop::Before(i) => (parent_of(&rows[i].0), Some(rows[i].0.clone())),
        Drop::After(i) => (parent_of(&rows[i].0), after(i)),
        Drop::TopEnd => (None, None),
    };
    if parent_of(&moving) != parent || before.is_some() || parent.is_some() {
        view.moved = Some((moving, parent, before));
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
///     deletable (bool): a trash at the right of the row under the pointer: a first click turns it red,
///         a second one deletes the level (it is given by `view.take_deleted()`, the app removes it
///         from its data). False by default
///
///     movable (bool): the rows can be dragged: dropped on the middle of another level it goes under it,
///         on its upper or lower edge it goes before or after it. The move is given by
///         `view.take_moved()` as (path, parent, before), the app moves it in its data. False by default
///
///     keep_order (bool): the levels stay in the order of the tags instead of being sorted by name,
///         so that a move before or after another level shows. False by default
///
///     icon (bool): the tag icon in the color of the level instead of a dot. False by default
///
///     checks (dict[str, str]): a check box at the left of every row, in the state given by the path of the level:
///         "on", "off" or "mixed" (a level that is not in the dict is off). The tree then picks tags, e.g. for the
///         selected items: a click on a row is returned like a selection, the app puts the tag on or takes it off.
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
    let deletable = kwarg::<bool>(kwargs, "deletable")?.unwrap_or(false);
    let movable = kwarg::<bool>(kwargs, "movable")?.unwrap_or(false);
    let keep_order = kwarg::<bool>(kwargs, "keep_order")?.unwrap_or(false);
    let icon = kwarg::<bool>(kwargs, "icon")?.unwrap_or(false);
    let checks = kwarg::<HashMap<String, String>>(kwargs, "checks")?;
    let check = checks.is_some();
    view.borrow_mut().states = checks.unwrap_or_default().into_iter().map(|(path, state)| (normalize(&path), state)).collect();
    let mut actions = vec![];
    let mut rows: Rows = vec![];
    let area;
    {
        let state = tags.borrow();
        let tree = view.borrow();
        let children = tag_children(&state.tags, keep_order);
        let inner = ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let menu = Menu { editable, removable, deletable, movable, icon, check };
            tree_rows(ui, &state, &tree, &children, &counts, "", 0, menu, &mut actions, &mut rows);
        });
        area = inner.response.rect;
    }
    dragging(ui, &mut view.borrow_mut(), &rows, area);

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
            NodeAction::DragStart(path) => {
                tree.dragging = Some(path);
            }
            NodeAction::Arm(path) => {
                tree.armed = Some(path);
            }
            NodeAction::Disarm => {
                tree.armed = None;
            }
            NodeAction::Delete(path) => {
                tree.armed = None;
                tree.deleted.push(path);
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
