//! Tags: a list of tags with several levels ("projet/web"), each level with its own color.
//!
//! `Tags` is the state (the tags and the colors), `tags_edit` draws it: a chip per tag showing its
//! last level, its full path in the colors of its levels on hover, a field to add one. The state and
//! the drawing are in `pyegui_core::tags`, shared with the Rust applications.

use std::collections::{BTreeMap, HashMap};
use std::ops::{Deref, DerefMut};

use eframe::egui::{Color32, Id};
use pyegui_core::tag_tree::{self, CheckState, TagRow, TagTreeEvent, TagTreeOptions, TagTreeState};
use pyegui_core::tags::{color_row, last_level, level_paths, normalize, path_ui, to_hex, Tags as CoreTags, TagsOptions};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::style::{extract_color, kwarg_color};
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
    /// the folded levels, the level being moved and the armed trash (the state of the tree of the core)
    state: TagTreeState<String>,
    /// the levels deleted with the trash, until take_deleted()
    deleted: Vec<String>,
    /// the move asked for with the mouse, until take_moved()
    moved: Option<(String, Option<String>, Option<String>)>,
}

#[pymethods]
impl TagTreeView {
    #[new]
    #[pyo3(signature = (selected=None, folded=None))]
    fn new(selected: Option<String>, folded: Option<Vec<String>>) -> Self {
        let mut state = TagTreeState::default();
        state.folded = folded.unwrap_or_default().iter().map(|s| normalize(s)).collect();
        TagTreeView { selected: selected.map(|s| normalize(&s)).filter(|s| !s.is_empty()), state, deleted: vec![], moved: None }
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
        let mut folded: Vec<String> = self.state.folded.iter().cloned().collect();
        folded.sort();
        folded
    }

    #[setter]
    fn set_folded(&mut self, folded: Vec<String>) {
        self.state.folded = folded.iter().map(|s| normalize(s)).collect();
    }

    /// Opens every level
    fn expand_all(&mut self) {
        self.state.folded.clear();
    }

    /// Folds every level that has children, among the levels of tags
    ///
    /// Args:
    ///     tags (Tags): the tags of the tree
    fn collapse_all(&mut self, tags: &Tags) {
        for tag in &tags.tags {
            let levels = level_paths(tag);
            self.state.folded.extend(levels[..levels.len() - 1].iter().cloned());
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

/// The rows of the tree, flat in the order of the tree, for `pyegui_core::tag_tree`
fn flat_rows(
    tags: &Tags,
    children: &BTreeMap<String, Vec<String>>,
    counts: &HashMap<String, i64>,
    checks: &HashMap<String, String>,
    parent: &str,
    depth: usize,
    out: &mut Vec<TagRow<String>>,
) {
    for path in children.get(parent).into_iter().flatten() {
        let has_children = children.contains_key(path);
        out.push(
            TagRow::new(path.clone(), last_level(path), depth, tags.color_of(path))
                .children(has_children)
                .count(counts.get(path).copied())
                .check(checks.get(path).map_or(CheckState::Off, |s| CheckState::parse(s))),
        );
        if has_children {
            flat_rows(tags, children, counts, checks, path, depth + 1, out);
        }
    }
}

/// Shows the tags as a tree: a node for every level, its last level in the color of its path,
/// the full path on hover. A click selects a level, the chevron folds or opens it (the `grid.chevron*`
/// tokens, like the other trees). A right click changes the color of the level or removes it from the
/// tags, with the tags under it. The tree is the one of finder-rs (`pyegui_core::tag_tree`): the same rows,
/// trash, check boxes and moves in both Finders.
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
///     row_height (float): the height of a row. Default is 26
///
///     indent (float): the indentation of a level. Default is 14
///
///     rounding (float): the rounding of the fills of the rows. Default is the rounding of the widgets
///
///     selected_fill (color), hover_fill (color): the fills of the selected row and of the row under the pointer.
///         Default is the selection color and the hovered fill of the theme
///
///     accent (color): the check boxes that are on and the line where a level is dropped. Default is the selection
///
///     danger (color): the armed trash. Default is the error color of the theme
///
///     delete_tip (str), armed_tip (str): the tooltips of the trash. Default is French, like the Finder
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
///             clicked = tag_tree_view(tree, library, counts={"projet": 12, "projet/web": 5}, icon=True)
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
    let keep_order = kwarg::<bool>(kwargs, "keep_order")?.unwrap_or(false);
    let checks = kwarg::<HashMap<String, String>>(kwargs, "checks")?;
    let defaults = TagTreeOptions::default();
    let options = TagTreeOptions {
        id: Id::unique(("pyegui-tag-tree", view.as_ptr() as usize)),
        deletable: kwarg::<bool>(kwargs, "deletable")?.unwrap_or(false),
        movable: kwarg::<bool>(kwargs, "movable")?.unwrap_or(false),
        icon: kwarg::<bool>(kwargs, "icon")?.unwrap_or(false),
        checks: checks.is_some(),
        fold_on_double_click: true,
        row_height: kwarg::<f32>(kwargs, "row_height")?.unwrap_or(defaults.row_height),
        indent: kwarg::<f32>(kwargs, "indent")?.unwrap_or(defaults.indent),
        rounding: kwarg::<f32>(kwargs, "rounding")?,
        selected_fill: kwarg_color(kwargs, "selected_fill")?,
        hover_fill: kwarg_color(kwargs, "hover_fill")?,
        accent: kwarg_color(kwargs, "accent")?,
        danger: kwarg_color(kwargs, "danger")?,
        delete_tip: kwarg::<String>(kwargs, "delete_tip")?.unwrap_or(defaults.delete_tip),
        armed_tip: kwarg::<String>(kwargs, "armed_tip")?.unwrap_or(defaults.armed_tip),
    };
    let checks: HashMap<String, String> = checks.unwrap_or_default().into_iter().map(|(path, state)| (normalize(&path), state)).collect();

    let mut actions: Vec<(String, Option<Color32>)> = vec![];
    let mut removed: Option<String> = None;
    let out = {
        let state = tags.borrow();
        let mut tree = view.borrow_mut();
        let children = tag_children(&state.tags, keep_order);
        let mut rows = vec![];
        flat_rows(&state, &children, &counts, &checks, "", 0, &mut rows);
        let selected = tree.selected.clone();
        let out = tag_tree::show(ui, &rows, selected.as_ref(), &mut tree.state, &options);
        for (path, response) in &out.rows {
            let has_children = children.contains_key(path);
            let response = response.clone().on_hover_ui(|ui| path_ui(ui, &state, path));
            if editable {
                response.context_menu(|ui| {
                    if let Some(color) = color_row(ui, &state, path) {
                        actions.push((path.clone(), color));
                    }
                    if !removable {
                        return;
                    }
                    ui.separator();
                    let label = if has_children { "Remove with its children" } else { "Remove" };
                    if ui.button(label).clicked() {
                        removed = Some(path.clone());
                        ui.close();
                    }
                });
            }
        }
        out.events
    };

    let mut clicked = None;
    let mut state = tags.borrow_mut();
    let mut tree = view.borrow_mut();
    for event in out {
        match event {
            TagTreeEvent::Select(path) => {
                tree.selected = Some(path.clone());
                clicked = Some(path);
            }
            TagTreeEvent::Toggle(_) => {}
            TagTreeEvent::Delete(path) => tree.deleted.push(path),
            TagTreeEvent::Move { key, parent, before } => tree.moved = Some((key, parent, before)),
        }
    }
    for (path, color) in actions {
        match color {
            Some(color) => state.colors.insert(path, color),
            None => state.colors.remove(&path),
        };
    }
    if let Some(path) = removed {
        let below = format!("{path}/");
        state.tags.retain(|t| *t != path && !t.starts_with(&below));
        if tree.selected.as_deref().is_some_and(|s| s == path || s.starts_with(&below)) {
            tree.selected = None;
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
