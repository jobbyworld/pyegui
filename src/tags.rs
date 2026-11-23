//! Tags: a list of tags with several levels ("projet/web"), each level with its own color.
//!
//! `Tags` is the state (the tags and the colors), `tags_edit` draws it: a chip per tag showing its
//! last level, its full path in the colors of its levels on hover, a field to add one. The state and
//! the drawing are in `pyegui_core::tags`, shared with the Rust applications.

use std::collections::{BTreeMap, HashMap};
use std::ops::{Deref, DerefMut};

use eframe::egui::{self, vec2, Color32, Id, Rect, Sense, Stroke, TextStyle};
use pyegui_core::tags::{color_row, last_level, level_paths, normalize, path_ui, to_hex, Tags as CoreTags, TagsOptions};
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
    o.fit = kwarg::<bool>(kwargs, "fit")?.unwrap_or(false);
    o.compact = kwarg::<bool>(kwargs, "compact")?.unwrap_or(false);
    o.dot_size = kwarg::<f32>(kwargs, "dot_size")?.unwrap_or(10.0);
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
