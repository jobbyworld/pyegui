use eframe::egui;
use egui::{Color32, CornerRadius, Id, Response, Role, Sense, Stroke, TextStyle, Ui, Vec2, Visuals, WidgetInfo};
use egui_tiles::{
    Container, ResizeState, SimplificationOptions, TabState, Tile, TileId, Tiles, UiResponse,
};
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList};
use std::collections::HashMap;

use std::sync::{Arc, Mutex};

use crate::style::{kwarg_color, kwarg_stroke};
use crate::{current_ui, kwarg, run_nested_call, run_nested_update_func, UI};

/// A pane of a Dock, shown as a tab.
///
/// Args:
///     title (str): title of the tab
///
///     update_func (Callable[[], None]): draws the contents of the pane
///
///     closable (bool): show a close button on the tab. Default is False
///
///     hide_tab (bool): hide the tab bar while the pane is alone in its tabs, like hide_single_tab_bar
///         but for this pane only. The tab is back as soon as another pane joins it. Default is False
///
/// Usage::
///
///     DockPane("Editor", editor_ui, closable=True)
#[pyclass]
pub(crate) struct DockPane {
    #[pyo3(get, set)]
    title: String,
    #[pyo3(get, set)]
    update_func: Py<PyAny>,
    #[pyo3(get, set)]
    closable: bool,
    #[pyo3(get, set)]
    hide_tab: bool,
}

#[pymethods]
impl DockPane {
    #[new]
    #[pyo3(signature = (title, update_func, closable=false, hide_tab=false))]
    fn new(title: String, update_func: Py<PyAny>, closable: bool, hide_tab: bool) -> Self {
        DockPane {
            title,
            update_func,
            closable,
            hide_tab,
        }
    }
}

/// A container of a Dock, holding DockPanes and other DockContainers.
///
/// Args:
///     kind (str): "tabs", "horizontal", "vertical" or "grid"
///
///     children (list[DockPane | DockContainer]): contents of the container
///
/// Usage::
///
///     DockContainer("horizontal", [DockPane("Files", files_ui), DockPane("Editor", editor_ui)])
#[pyclass]
pub(crate) struct DockContainer {
    #[pyo3(get, set)]
    kind: String,
    #[pyo3(get, set)]
    children: Vec<Py<PyAny>>,
}

#[pymethods]
impl DockContainer {
    #[new]
    fn new(kind: String, children: Vec<Py<PyAny>>) -> Self {
        DockContainer { kind, children }
    }
}

struct Pane {
    title: String,
    update_func: Py<PyAny>,
    closable: bool,
    hide_tab: bool,
}

impl Pane {
    fn clone_ref(&self, py: Python<'_>) -> Self {
        Pane {
            title: self.title.clone(),
            update_func: self.update_func.clone_ref(py),
            closable: self.closable,
            hide_tab: self.hide_tab,
        }
    }

    fn from_py(pane: &DockPane, py: Python<'_>) -> Self {
        Pane {
            title: pane.title.clone(),
            update_func: pane.update_func.clone_ref(py),
            closable: pane.closable,
            hide_tab: pane.hide_tab,
        }
    }
}

fn insert_tile(tiles: &mut Tiles<Pane>, node: &Bound<'_, PyAny>) -> PyResult<TileId> {
    if let Ok(pane) = node.cast::<DockPane>() {
        return Ok(tiles.insert_pane(Pane::from_py(&pane.borrow(), node.py())));
    }

    let container = node
        .cast::<DockContainer>()
        .map_err(|_| PyTypeError::new_err("Dock layout must contain only DockPane and DockContainer"))?
        .borrow();

    let children = container
        .children
        .iter()
        .map(|child| insert_tile(tiles, child.bind(node.py())))
        .collect::<PyResult<Vec<_>>>()?;

    match container.kind.as_str() {
        "tabs" => Ok(tiles.insert_tab_tile(children)),
        "horizontal" => Ok(tiles.insert_horizontal_tile(children)),
        "vertical" => Ok(tiles.insert_vertical_tile(children)),
        "grid" => Ok(tiles.insert_grid_tile(children)),
        kind => Err(PyValueError::new_err(format!(
            "Unknown container kind '{}', expected tabs, horizontal, vertical or grid",
            kind
        ))),
    }
}

/// The layout of a tile as plain Python values, for Dock.save(). None for a tile that is gone
fn save_tile<'py>(py: Python<'py>, tiles: &Tiles<Pane>, tile_id: TileId) -> PyResult<Option<Bound<'py, PyDict>>> {
    let node = PyDict::new(py);
    match tiles.get(tile_id) {
        Some(Tile::Pane(pane)) => node.set_item("pane", &pane.title)?,
        Some(Tile::Container(container)) => {
            let children = PyList::empty(py);
            let mut kept = vec![];
            for child in container.children() {
                if let Some(saved) = save_tile(py, tiles, *child)? {
                    children.append(saved)?;
                    kept.push(*child);
                }
            }
            node.set_item("children", children)?;
            match container {
                Container::Tabs(tabs) => {
                    node.set_item("kind", "tabs")?;
                    let active = tabs.active.and_then(|active| kept.iter().position(|child| *child == active));
                    node.set_item("active", active)?;
                }
                Container::Linear(linear) => {
                    let horizontal = matches!(linear.dir, egui_tiles::LinearDir::Horizontal);
                    node.set_item("kind", if horizontal { "horizontal" } else { "vertical" })?;
                    let shares: Vec<f32> = kept.iter().map(|child| linear.shares[*child]).collect();
                    node.set_item("shares", shares)?;
                }
                Container::Grid(_) => node.set_item("kind", "grid")?,
            }
        }
        None => return Ok(None),
    }
    Ok(Some(node))
}

/// Rebuilds a tile saved by save_tile from the panes available by title. A pane that is not
/// available is left out, and so is a container left without children
fn load_tile(
    node: &Bound<'_, PyAny>,
    tiles: &mut Tiles<Pane>,
    available: &mut HashMap<String, Pane>,
) -> PyResult<Option<TileId>> {
    let node = node.cast::<PyDict>()?;
    if let Some(title) = node.get_item("pane")? {
        let title: String = title.extract()?;
        return Ok(available.remove(&title).map(|pane| tiles.insert_pane(pane)));
    }
    let kind: String = node
        .get_item("kind")?
        .ok_or_else(|| PyValueError::new_err("a container of the layout has no kind"))?
        .extract()?;
    let children = node
        .get_item("children")?
        .ok_or_else(|| PyValueError::new_err("a container of the layout has no children"))?;
    let mut ids = vec![];
    let mut saved_index = vec![]; // where each kept child was among the saved ones
    for (index, child) in children.cast::<PyList>()?.iter().enumerate() {
        if let Some(id) = load_tile(&child, tiles, available)? {
            ids.push(id);
            saved_index.push(index);
        }
    }
    if ids.is_empty() {
        return Ok(None);
    }
    let id = match kind.as_str() {
        "tabs" => tiles.insert_tab_tile(ids.clone()),
        "horizontal" => tiles.insert_horizontal_tile(ids.clone()),
        "vertical" => tiles.insert_vertical_tile(ids.clone()),
        "grid" => tiles.insert_grid_tile(ids.clone()),
        kind => return Err(PyValueError::new_err(format!("Unknown container kind '{}'", kind))),
    };
    match tiles.get_mut(id) {
        Some(Tile::Container(Container::Tabs(tabs))) => {
            let active = node.get_item("active")?.and_then(|a| a.extract::<usize>().ok());
            if let Some(position) = active.and_then(|a| saved_index.iter().position(|i| *i == a)) {
                tabs.set_active(ids[position]);
            }
        }
        Some(Tile::Container(Container::Linear(linear))) => {
            let shares = node.get_item("shares")?.and_then(|s| s.extract::<Vec<f32>>().ok());
            for (position, child) in ids.iter().enumerate() {
                let share = shares.as_ref().and_then(|s| s.get(saved_index[position])).copied();
                if let Some(share) = share.filter(|share| share.is_finite() && *share > 0.0) {
                    linear.shares.set_share(*child, share);
                }
            }
        }
        _ => {}
    }
    Ok(Some(id))
}

/// A change asked from a tab bar callback, applied once the dock is drawn
enum TabBarAction {
    Add(Pane),
    Activate(String),
    Close(String),
}

/// The changes asked from the tab bar callbacks, with the tabs container they apply to
type TabBarActions = Arc<Mutex<Vec<(TileId, TabBarAction)>>>;

/// A tab bar of a Dock, given to the tab_bar_left, tab_bar_trailing and tab_bar_right callbacks.
///
/// Usage::
///
///     def new_tab_button(tab_bar):
///         if button_clicked("+"):
///             tab_bar.add_pane(DockPane(f"Untitled {len(tab_bar.titles) + 1}", editor_ui, closable=True))
///
///     dock = Dock(root, tab_bar_trailing=new_tab_button)
///
///     # a menu listing the tabs
///     def tabs_menu(tab_bar):
///         def items():
///             for title in tab_bar.titles:
///                 if button_clicked(title):
///                     tab_bar.set_active(title)
///             separator()
///             if button_clicked("Close the others"):
///                 for title in tab_bar.titles:
///                     if title != tab_bar.active_title:
///                         tab_bar.close(title)
///         menu_button("☰", items)
///
///     dock = Dock(root, tab_bar_left=tabs_menu)
///
/// The changes are applied once the dock is drawn, so they show up on the next frame.
#[pyclass]
pub(crate) struct DockTabBar {
    tile_id: TileId,
    titles: Vec<String>,
    active_title: Option<String>,
    actions: TabBarActions,
}

impl DockTabBar {
    fn push(&self, action: TabBarAction) -> PyResult<()> {
        self.actions
            .lock()
            .map_err(|_| PyRuntimeError::new_err("dock lock is poisoned"))?
            .push((self.tile_id, action));
        Ok(())
    }
}

#[pymethods]
impl DockTabBar {
    /// Adds a pane to these tabs and makes it the active tab
    fn add_pane(&self, py: Python<'_>, pane: PyRef<'_, DockPane>) -> PyResult<()> {
        self.push(TabBarAction::Add(Pane::from_py(&pane, py)))
    }

    /// Makes the first tab with this title the active tab
    fn set_active(&self, title: String) -> PyResult<()> {
        self.check_title(&title)?;
        self.push(TabBarAction::Activate(title))
    }

    /// Closes the first tab with this title. Only the panes created with closable=True are closed
    fn close(&self, title: String) -> PyResult<()> {
        self.check_title(&title)?;
        self.push(TabBarAction::Close(title))
    }

    /// Titles of the tabs, in display order
    #[getter]
    fn titles(&self) -> Vec<String> {
        self.titles.clone()
    }

    /// Title of the active tab, or None
    #[getter]
    fn active_title(&self) -> Option<String> {
        self.active_title.clone()
    }
}

impl DockTabBar {
    fn check_title(&self, title: &str) -> PyResult<()> {
        if self.titles.iter().any(|t| t == title) {
            Ok(())
        } else {
            Err(PyValueError::new_err(format!(
                "No tab '{}' in this tab bar, the tabs are {:?}",
                title, self.titles
            )))
        }
    }
}

/// The Python callbacks drawing in the tab bars
#[derive(Default)]
struct TabBarCallbacks {
    left: Option<Py<PyAny>>,
    trailing: Option<Py<PyAny>>,
    right: Option<Py<PyAny>>,
}

const CALLBACK_KEYS: [&str; 3] = ["tab_bar_left", "tab_bar_trailing", "tab_bar_right"];

impl TabBarCallbacks {
    /// Takes the callbacks out of kwargs, so the rest are style options
    fn take<'py>(kwargs: Option<&Bound<'py, PyDict>>) -> PyResult<(Self, Option<Bound<'py, PyDict>>)> {
        let Some(kwargs) = kwargs else {
            return Ok((Self::default(), None));
        };
        let rest = kwargs.copy()?;
        let take = |key: &str| -> PyResult<Option<Py<PyAny>>> {
            let value = rest.get_item(key)?.filter(|value| !value.is_none());
            rest.del_item(key).ok();
            match value {
                Some(value) if !value.is_callable() => Err(PyTypeError::new_err(format!(
                    "{} must be a function taking a DockTabBar",
                    key
                ))),
                value => Ok(value.map(Bound::unbind)),
            }
        };
        let callbacks = TabBarCallbacks {
            left: take(CALLBACK_KEYS[0])?,
            trailing: take(CALLBACK_KEYS[1])?,
            right: take(CALLBACK_KEYS[2])?,
        };
        Ok((callbacks, Some(rest)))
    }

    fn clone_ref(&self, py: Python<'_>) -> Self {
        TabBarCallbacks {
            left: self.left.as_ref().map(|f| f.clone_ref(py)),
            trailing: self.trailing.as_ref().map(|f| f.clone_ref(py)),
            right: self.right.as_ref().map(|f| f.clone_ref(py)),
        }
    }
}

/// Look of the tab bar and of the gaps of a Dock. Unset options keep the egui_tiles defaults
#[derive(Clone, Default)]
struct DockStyle {
    glass: bool,
    tab_bar_height: Option<f32>,
    tab_bar_color: Option<Color32>,
    hide_single_tab_bar: bool,
    /// pill tabs instead of the classic tabs joined to their pane
    pill: bool,
    active_tab_fill: Option<Color32>,
    inactive_tab_fill: Option<Color32>,
    hovered_tab_fill: Option<Color32>,
    tab_spacing: Option<f32>,
    active_tab_text_color: Option<Color32>,
    inactive_tab_text_color: Option<Color32>,
    active_tab_stroke: Option<Stroke>,
    inactive_tab_stroke: Option<Stroke>,
    tab_title_spacing: Option<f32>,
    gap_width: Option<f32>,
    resize_stroke: Option<Stroke>,
    drag_preview_color: Option<Color32>,
    drag_preview_stroke: Option<Stroke>,
    pane_stroke: Option<Stroke>,
    pane_margin: Option<f32>,
}

const STYLE_KEYS: [&str; 20] = [
    "glass",
    "tab_bar_height",
    "tab_bar_color",
    "hide_single_tab_bar",
    "tab_style",
    "active_tab_fill",
    "inactive_tab_fill",
    "hovered_tab_fill",
    "tab_spacing",
    "active_tab_text_color",
    "inactive_tab_text_color",
    "active_tab_stroke",
    "inactive_tab_stroke",
    "tab_title_spacing",
    "gap_width",
    "resize_stroke",
    "drag_preview_color",
    "drag_preview_stroke",
    "pane_stroke",
    "pane_margin",
];

impl DockStyle {
    fn tab_spacing(&self) -> f32 {
        self.tab_spacing
            .unwrap_or(if self.glass || self.pill { 4.0 } else { 2.0 })
    }

    /// A light outline contrasting with the theme
    fn border(visuals: &Visuals, alpha: u8) -> Color32 {
        if visuals.dark_mode {
            Color32::from_white_alpha(alpha)
        } else {
            Color32::from_black_alpha(alpha)
        }
    }

    fn active_tab_stroke(&self, visuals: &Visuals) -> Stroke {
        self.active_tab_stroke.unwrap_or_else(|| match crate::layout::dock_rules() {
            Some(rules) => rules.outline,
            None => Stroke::new(1.0, Self::border(visuals, 45)),
        })
    }

    fn pane_stroke(&self, visuals: &Visuals) -> Stroke {
        self.pane_stroke.unwrap_or(self.active_tab_stroke(visuals))
    }

    /// The tint of the inactive tabs, which sets them apart from the active one
    fn inactive_tint(visuals: &Visuals, hovered: bool) -> Color32 {
        let alpha = match (visuals.dark_mode, hovered) {
            (true, false) => 70,
            (true, true) => 35,
            (false, false) => 20,
            (false, true) => 10,
        };
        Color32::from_black_alpha(alpha)
    }

    /// Updates the options given in kwargs, the others are kept
    fn update(&mut self, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        if let Some(kwargs) = kwargs {
            for key in kwargs.keys() {
                let key = key.extract::<String>()?;
                if !STYLE_KEYS.contains(&key.as_str()) {
                    return Err(PyTypeError::new_err(format!(
                        "Unknown dock option '{}', expected one of {}, {}",
                        key,
                        STYLE_KEYS.join(", "),
                        CALLBACK_KEYS.join(", ")
                    )));
                }
            }
        }

        macro_rules! set {
            ($field:ident, $value:expr) => {
                if let Some(value) = $value {
                    self.$field = value;
                }
            };
            (opt $field:ident, $value:expr) => {
                if let Some(value) = $value {
                    self.$field = Some(value);
                }
            };
        }

        set!(glass, kwarg::<bool>(kwargs, "glass")?);
        set!(opt tab_bar_height, kwarg::<f32>(kwargs, "tab_bar_height")?);
        set!(opt tab_bar_color, kwarg_color(kwargs, "tab_bar_color")?);
        set!(hide_single_tab_bar, kwarg::<bool>(kwargs, "hide_single_tab_bar")?);
        match kwarg::<String>(kwargs, "tab_style")?.as_deref() {
            None => {}
            Some("classic") => self.pill = false,
            Some("pill") => self.pill = true,
            Some(other) => {
                return Err(PyValueError::new_err(format!(
                    "Unknown tab style '{}', expected classic or pill",
                    other
                )))
            }
        }
        set!(opt active_tab_fill, kwarg_color(kwargs, "active_tab_fill")?);
        set!(opt inactive_tab_fill, kwarg_color(kwargs, "inactive_tab_fill")?);
        set!(opt hovered_tab_fill, kwarg_color(kwargs, "hovered_tab_fill")?);
        set!(opt tab_spacing, kwarg::<f32>(kwargs, "tab_spacing")?);
        set!(opt active_tab_text_color, kwarg_color(kwargs, "active_tab_text_color")?);
        set!(opt inactive_tab_text_color, kwarg_color(kwargs, "inactive_tab_text_color")?);
        set!(opt active_tab_stroke, kwarg_stroke(kwargs, "active_tab_stroke")?);
        set!(opt inactive_tab_stroke, kwarg_stroke(kwargs, "inactive_tab_stroke")?);
        set!(opt tab_title_spacing, kwarg::<f32>(kwargs, "tab_title_spacing")?);
        set!(opt gap_width, kwarg::<f32>(kwargs, "gap_width")?);
        set!(opt resize_stroke, kwarg_stroke(kwargs, "resize_stroke")?);
        set!(opt drag_preview_color, kwarg_color(kwargs, "drag_preview_color")?);
        set!(opt drag_preview_stroke, kwarg_stroke(kwargs, "drag_preview_stroke")?);
        set!(opt pane_stroke, kwarg_stroke(kwargs, "pane_stroke")?);
        set!(opt pane_margin, kwarg::<f32>(kwargs, "pane_margin")?);
        Ok(())
    }
}

/// Dock stores a layout of panes that the user can resize, rearrange by dragging tabs, and close.
///
/// Args:
///     root (DockPane | DockContainer): initial layout
///
///     id (str): unique id of the dock. Default is "dock"
///
/// Keyword Args:
///     glass (bool): sizes matching ctx.set_glass_style(): taller tab bar, more space between the tabs,
///         wider see-through gaps between the panes. The other options override it
///
///     tab_bar_height (float): height of the tab bar. Default is 24
///
///     tab_bar_color (color): background of the tab bar. Default is transparent
///
///     hide_single_tab_bar (bool): hide the tab bar of tabs holding a single pane. Default is False
///
///     tab_style (str): "classic" tabs joined to their pane, or "pill" tabs rounded on every corner,
///         the active one filled with the selection color. Default is "classic"
///
///     active_tab_fill (color): background of the active tab. Default is transparent, so the active tab
///         has the same background as its pane
///
///     inactive_tab_fill (color): background of the other tabs. Default is a dark tint setting them apart
///
///     hovered_tab_fill (color): background of an inactive tab under the mouse. Default is a lighter tint
///
///     tab_spacing (float): space between two tabs. Default is 2, 4 with glass
///
/// The top corners of the tabs are rounded like the widgets of the theme, see the rounding and
/// widget_rounding options of ctx.set_style().
///
///     active_tab_text_color (color): title color of the active tab
///
///     inactive_tab_text_color (color): title color of the other tabs. Default is dimmed
///
///     active_tab_stroke (tuple[float, color]): outline of the active tab, open at the bottom. Default is a
///         thin translucent line, dark on light theme and light on dark theme
///
///     inactive_tab_stroke (tuple[float, color]): outline of the other tabs. Default is a fainter line
///
///     tab_title_spacing (float): space on the left and right of the tab titles. Default is 8
///
///     gap_width (float): gap between the panes of horizontal, vertical and grid containers. Default is 1
///
///     resize_stroke (tuple[float, color]): line drawn in the gaps between panes
///
///     drag_preview_color (color): fill of the area where a dragged tab will be dropped
///
///     drag_preview_stroke (tuple[float, color]): outline of the area where a dragged tab will be dropped
///
///     pane_stroke (tuple[float, color]): outline of the panes, rounded like the tabs and open under the
///         active tab so the tab and its pane form one shape. Default is the outline of the active tab
///
///     pane_margin (float): space between the outline of the panes and their contents. Default is 6
///
/// Colors are (r, g, b), (r, g, b, a) or "#rrggbb[aa]".
///
/// The tab bars can hold widgets, drawn by functions taking a DockTabBar. They are called for every
/// tab bar of the dock:
///
///     tab_bar_left (Callable[[DockTabBar], None]): widgets on the left of the tabs, e.g. a menu
///
///     tab_bar_trailing (Callable[[DockTabBar], None]): widgets right after the last tab, e.g. a "+" button
///
///     tab_bar_right (Callable[[DockTabBar], None]): widgets on the right of the tab bar, added right to left
///
/// Usage::
///
///     dock = Dock(DockContainer("horizontal", [
///         DockPane("Files", files_ui),
///         DockContainer("tabs", [DockPane("Editor", editor_ui), DockPane("Preview", preview_ui)]),
///     ]))
///
///     def update_func(ctx):
///         dock_area(dock)
///
///     # a "+" button opening a tab in the tab bar it belongs to
///     def new_tab_button(tab_bar):
///         if button_clicked("+"):
///             tab_bar.add_pane(DockPane("New", new_ui, closable=True))
///
///     dock = Dock(root, tab_bar_trailing=new_tab_button)
///
///     # glass sizes, with pink inactive tabs
///     dock = Dock(root, glass=True, inactive_tab_fill="#ec489933", tab_bar_height=30)
#[pyclass]
pub(crate) struct Dock {
    // taken out while the dock is shown, so panes can call add_pane
    tree: Option<egui_tiles::Tree<Pane>>,
    pending: Vec<Pane>,
    style: DockStyle,
    callbacks: TabBarCallbacks,
}

fn add_pane_to_tree(tree: &mut egui_tiles::Tree<Pane>, pane: Pane) {
    let pane_id = tree.tiles.insert_pane(pane);

    // prefer the first tabs, so new panes open as tabs instead of splitting the layout
    let target = tree
        .root
        .and_then(|root| find_tabs(&tree.tiles, root))
        .or(tree
            .root
            .filter(|root| matches!(tree.tiles.get(*root), Some(Tile::Container(_)))));

    match target {
        Some(target) => {
            if let Some(Tile::Container(container)) = tree.tiles.get_mut(target) {
                container.add_child(pane_id);

                if let Container::Tabs(tabs) = container {
                    tabs.set_active(pane_id);
                }
            }
        }
        None => {
            // the root is a single pane or the dock is empty
            let children = tree.root.into_iter().chain([pane_id]).collect();
            tree.root = Some(tree.tiles.insert_tab_tile(children));
        }
    }
}

/// Adds a pane to the given tabs and makes it active, or to the dock if the tabs are gone
fn add_pane_to_tabs(tree: &mut egui_tiles::Tree<Pane>, tabs_id: TileId, pane: Pane) {
    if !matches!(tree.tiles.get(tabs_id), Some(Tile::Container(Container::Tabs(_)))) {
        return add_pane_to_tree(tree, pane);
    }
    let pane_id = tree.tiles.insert_pane(pane);
    if let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(tabs_id) {
        tabs.add_child(pane_id);
        tabs.set_active(pane_id);
    }
}

/// Points of the outline of a tab: left side, rounded top and right side, open at the bottom
fn tab_outline(rect: egui::Rect, radius: f32) -> Vec<egui::Pos2> {
    use std::f32::consts::FRAC_PI_2;

    let radius = radius.min(rect.width() / 2.0).min(rect.height());
    let steps = (radius.ceil() as usize).clamp(1, 8);
    let mut points = vec![rect.left_bottom()];
    let mut corner = |center: egui::Pos2, start: f32| {
        for i in 0..=steps {
            let angle = start + FRAC_PI_2 * i as f32 / steps as f32;
            points.push(center + radius * egui::vec2(angle.cos(), angle.sin()));
        }
    };
    // angles grow clockwise on screen, as y points down
    corner(rect.left_top() + egui::vec2(radius, radius), 2.0 * FRAC_PI_2);
    corner(rect.right_top() + egui::vec2(-radius, radius), 3.0 * FRAC_PI_2);
    points.push(rect.right_bottom());
    points
}

/// Points of the outline of a pane: rounded corners, open at the top under its tab.
/// A top corner touched by the tab stays square, so the sides of the tab continue into the pane
fn pane_outline(rect: egui::Rect, tab: Option<egui::Rect>, radius: f32, width: f32) -> Vec<egui::Pos2> {
    use egui::pos2;
    use std::f32::consts::FRAC_PI_2;

    let inner = rect.shrink(width / 2.0);
    let radius = radius.min(inner.width() / 2.0).min(inner.height() / 2.0).max(0.0);
    let steps = (radius.ceil() as usize).clamp(1, 8);
    let mut points = vec![];

    // y points down, so the angles of the arcs decrease to go counterclockwise on screen
    let arc = |points: &mut Vec<egui::Pos2>, center: egui::Pos2, start: f32| {
        for i in 0..=steps {
            let angle = start - FRAC_PI_2 * i as f32 / steps as f32;
            points.push(center + radius * egui::vec2(angle.cos(), angle.sin()));
        }
    };

    // without a tab the outline is closed: it starts and ends in the middle of the top side
    let (gap_left, gap_right) = match tab {
        Some(tab) => (
            (tab.left() + width / 2.0).clamp(inner.left(), inner.right()),
            (tab.right() - width / 2.0).clamp(inner.left(), inner.right()),
        ),
        None => (inner.center().x, inner.center().x),
    };
    let round_left = gap_left - inner.left() >= radius;
    let round_right = inner.right() - gap_right >= radius;

    if tab.is_some() {
        // joins the bottom of the left side of the tab
        points.push(pos2(gap_left, rect.top() - width / 2.0));
    }
    points.push(pos2(gap_left, inner.top()));
    if round_left {
        arc(&mut points, inner.left_top() + egui::vec2(radius, radius), 3.0 * FRAC_PI_2);
    } else {
        points.push(inner.left_top());
    }
    arc(&mut points, inner.left_bottom() + egui::vec2(radius, -radius), 2.0 * FRAC_PI_2);
    arc(&mut points, inner.right_bottom() + egui::vec2(-radius, -radius), FRAC_PI_2);
    if round_right {
        arc(&mut points, inner.right_top() + egui::vec2(-radius, radius), 0.0);
    } else {
        points.push(inner.right_top());
    }
    points.push(pos2(gap_right, inner.top()));
    if tab.is_some() {
        // joins the bottom of the right side of the tab
        points.push(pos2(gap_right, rect.top() - width / 2.0));
    }
    points.dedup_by(|a, b| a.distance(*b) < 0.01);
    points
}

/// Applies a change asked from a tab bar callback
fn apply_tab_bar_action(tree: &mut egui_tiles::Tree<Pane>, tabs_id: TileId, action: TabBarAction) {
    let title = match action {
        TabBarAction::Add(pane) => return add_pane_to_tabs(tree, tabs_id, pane),
        TabBarAction::Activate(ref title) | TabBarAction::Close(ref title) => title.clone(),
    };
    let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get(tabs_id) else {
        return;
    };
    // the tab may have been moved or closed since the callback ran
    let Some(pane_id) = tabs.children.iter().copied().find(|id| {
        matches!(tree.tiles.get(*id), Some(Tile::Pane(pane)) if pane.title == title)
    }) else {
        return;
    };

    match action {
        TabBarAction::Activate(_) => {
            if let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(tabs_id) {
                tabs.set_active(pane_id);
            }
        }
        TabBarAction::Close(_) => {
            if matches!(tree.tiles.get(pane_id), Some(Tile::Pane(pane)) if pane.closable) {
                // like the close button of the tab: the tree drops the closed tile on the next frame
                tree.tiles.remove(pane_id);
            }
        }
        TabBarAction::Add(_) => unreachable!(),
    }
}

/// Finds the first tabs container, depth first
fn find_tabs(tiles: &Tiles<Pane>, tile_id: TileId) -> Option<TileId> {
    match tiles.get(tile_id) {
        Some(Tile::Container(Container::Tabs(_))) => Some(tile_id),
        Some(Tile::Container(container)) => container
            .children()
            .find_map(|child| find_tabs(tiles, *child)),
        _ => None,
    }
}

#[pymethods]
impl Dock {
    #[new]
    #[pyo3(signature = (root, id="dock", **kwargs))]
    fn new(root: &Bound<'_, PyAny>, id: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let mut tiles = Tiles::default();
        let root = insert_tile(&mut tiles, root)?;
        let (callbacks, kwargs) = TabBarCallbacks::take(kwargs)?;
        let mut style = DockStyle::default();
        style.update(kwargs.as_ref())?;

        Ok(Dock {
            tree: Some(egui_tiles::Tree::new(egui::Id::unique(id), root, tiles)),
            pending: vec![],
            style,
            callbacks,
        })
    }

    /// Changes the look of the dock. Takes the same keyword arguments as Dock, the options
    /// that are not given are kept. Call reset_style() to go back to the defaults.
    ///
    /// Usage::
    ///
    ///     dock.set_style(tab_bar_height=30, active_tab_fill=(59, 130, 246, 120))
    #[pyo3(signature = (**kwargs))]
    fn set_style(&mut self, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        self.style.update(kwargs)
    }

    /// Goes back to the default look of the dock
    fn reset_style(&mut self) {
        self.style = DockStyle::default();
    }

    /// Changes the functions drawing in the tab bars, see Dock. None removes a function, the
    /// functions that are not given are kept.
    ///
    /// Usage::
    ///
    ///     dock.set_tab_bar(trailing=new_tab_button, right=None)
    #[pyo3(signature = (**kwargs))]
    fn set_tab_bar(&mut self, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        let Some(kwargs) = kwargs else {
            return Ok(());
        };
        for (key, value) in kwargs.iter() {
            let key = key.extract::<String>()?;
            let slot = match key.as_str() {
                "left" => &mut self.callbacks.left,
                "trailing" => &mut self.callbacks.trailing,
                "right" => &mut self.callbacks.right,
                _ => {
                    return Err(PyTypeError::new_err(format!(
                        "Unknown tab bar slot '{}', expected left, trailing or right",
                        key
                    )))
                }
            };
            if !value.is_none() && !value.is_callable() {
                return Err(PyTypeError::new_err(format!(
                    "{} must be a function taking a DockTabBar",
                    key
                )));
            }
            *slot = (!value.is_none()).then(|| value.unbind());
        }
        Ok(())
    }

    /// Adds a pane to the first tabs of the dock, or to the root container, and makes it the active tab
    fn add_pane(&mut self, py: Python<'_>, pane: PyRef<'_, DockPane>) {
        let pane = Pane::from_py(&pane, py);

        match &mut self.tree {
            Some(tree) => add_pane_to_tree(tree, pane),
            None => self.pending.push(pane),
        }
    }

    /// Makes the first pane with this title the active tab. Returns False if there is no such pane
    ///
    /// Usage::
    ///
    ///     if selectable_label_clicked(dock.active_title == "Tasks", "Tasks"):
    ///         dock.set_active("Tasks")
    fn set_active(&mut self, title: &str) -> PyResult<bool> {
        let tree = self
            .tree
            .as_mut()
            .ok_or(PyRuntimeError::new_err("set_active cannot be called inside a pane of the same dock"))?;
        let mut found = false;
        tree.make_active(|_, tile| {
            let matches = !found && matches!(tile, Tile::Pane(pane) if pane.title == title);
            found |= matches;
            matches
        });
        Ok(found)
    }

    /// Title of the active pane: the visible pane of the first tabs, or None
    #[getter]
    fn active_title(&self) -> Option<String> {
        let tree = self.tree.as_ref()?;
        tree.active_tiles()
            .into_iter()
            .find_map(|id| match tree.tiles.get(id) {
                Some(Tile::Pane(pane)) => Some(pane.title.clone()),
                _ => None,
            })
    }

    /// Returns the layout of the dock as plain Python values (dicts, lists, strings, numbers): which
    /// panes are open, how they are grouped in tabs and splits, the active tab of each group and the
    /// size of each part. It is made to be given to ctx.save_state() and later to load(). Returns
    /// None if called inside a pane of the same dock.
    ///
    /// Usage::
    ///
    ///     ctx.save_state({"dock": dock.save()})
    fn save<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyDict>>> {
        let Some(tree) = &self.tree else { return Ok(None) };
        match tree.root {
            Some(root) => save_tile(py, &tree.tiles, root),
            None => Ok(None),
        }
    }

    /// Restores a layout given by save(). Returns True if the layout was applied, and False,
    /// leaving the dock as it is, if it is empty, damaged or from an unrelated dock: an old
    /// saved state never raises.
    ///
    /// The panes are found by title among the ones of the dock. A pane saved in the layout but
    /// that does not exist any more is left out. A pane that was closed when the layout was saved
    /// is not in it, so it is closed again; to be able to open it, give it in panes: the layout
    /// can then also bring back a pane that is not in the dock yet.
    ///
    /// Args:
    ///     layout (dict): what save() returned
    ///
    ///     panes (list[DockPane]): more panes the layout can use, for the ones that are not in
    ///         the dock now
    ///
    /// Usage::
    ///
    ///     saved = ctx.load_state() or {}
    ///     if "dock" in saved:
    ///         dock.load(saved["dock"], panes=all_panes)
    #[pyo3(signature = (layout, panes=None))]
    fn load(
        &mut self,
        py: Python<'_>,
        layout: &Bound<'_, PyAny>,
        panes: Option<Vec<PyRef<'_, DockPane>>>,
    ) -> PyResult<bool> {
        let tree = self
            .tree
            .as_mut()
            .ok_or(PyRuntimeError::new_err("load cannot be called inside a pane of the same dock"))?;
        let mut available: HashMap<String, Pane> = HashMap::new();
        for (_, tile) in tree.tiles.iter() {
            if let Tile::Pane(pane) = tile {
                available.entry(pane.title.clone()).or_insert_with(|| pane.clone_ref(py));
            }
        }
        for pane in &self.pending {
            available.entry(pane.title.clone()).or_insert_with(|| pane.clone_ref(py));
        }
        for pane in panes.iter().flatten() {
            available
                .entry(pane.title.clone())
                .or_insert_with(|| Pane::from_py(pane, py));
        }

        let mut tiles = Tiles::default();
        match load_tile(layout, &mut tiles, &mut available) {
            Ok(Some(root)) => {
                *tree = egui_tiles::Tree::new(tree.id(), root, tiles);
                self.pending.clear();
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(err) => {
                log::warn!("Cannot load the layout of the dock: {}", err);
                Ok(false)
            }
        }
    }

    /// Titles of the panes currently in the dock, in display order
    #[getter]
    fn titles(&self) -> Vec<String> {
        let mut titles = vec![];

        if let Some(tree) = &self.tree {
            if let Some(root) = tree.root {
                collect_titles(&tree.tiles, root, &mut titles);
            }
        }
        titles.extend(self.pending.iter().map(|pane| pane.title.clone()));
        titles
    }
}

/// Collects the titles of the panes in display order
fn collect_titles(tiles: &Tiles<Pane>, tile_id: TileId, titles: &mut Vec<String>) {
    match tiles.get(tile_id) {
        Some(Tile::Pane(pane)) => titles.push(pane.title.clone()),
        Some(Tile::Container(container)) => {
            for child in container.children() {
                collect_titles(tiles, *child, titles);
            }
        }
        None => {}
    }
}

struct Behavior<'py> {
    py: Python<'py>,
    result: PyResult<()>,
    style: DockStyle,
    callbacks: TabBarCallbacks,
    actions: TabBarActions,
    /// Panes drawn this frame
    panes: std::collections::HashSet<TileId>,
    /// Panes whose tab bar is hidden while they are alone in their tabs
    hidden_alone: std::collections::HashSet<TileId>,
    /// Rect of the tab of the active panes, where their outline is open
    active_tabs: std::collections::HashMap<TileId, egui::Rect>,
}

impl Behavior<'_> {
    /// Calls a tab bar callback with the DockTabBar of these tabs
    fn tab_bar_ui(
        &mut self,
        callback: Option<&Py<PyAny>>,
        tiles: &Tiles<Pane>,
        ui: &mut Ui,
        tile_id: TileId,
        tabs: &egui_tiles::Tabs,
    ) {
        let Some(callback) = callback else {
            return;
        };
        let title = |id: &TileId| match tiles.get(*id) {
            Some(Tile::Pane(pane)) => Some(pane.title.clone()),
            _ => None,
        };
        let tab_bar = DockTabBar {
            tile_id,
            titles: tabs.children.iter().filter_map(title).collect(),
            active_title: tabs.active.as_ref().and_then(title),
            actions: self.actions.clone(),
        };
        let result = Bound::new(self.py, tab_bar).and_then(|tab_bar| unsafe {
            run_nested_call(ui, callback.bind(self.py), (tab_bar,))
        });
        if let Err(err) = result {
            self.result = Err(err);
        }
    }
}

impl egui_tiles::Behavior<Pane> for Behavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        pane.title.as_str().into()
    }

    fn pane_ui(&mut self, ui: &mut egui::Ui, tile_id: TileId, pane: &mut Pane) -> UiResponse {
        self.panes.insert(tile_id);
        let update_func = pane.update_func.bind(self.py).clone();
        let margin = self
            .style
            .pane_margin
            .unwrap_or_else(|| crate::layout::dock_rules().map_or(6.0, |rules| rules.inner_margin));

        if let Some(rules) = crate::layout::dock_rules() {
            // the pane is a box of the rules: filled with the opacity of the region, with its corners
            let fill = rules.sheet_fill(ui.visuals());
            ui.painter().rect_filled(ui.max_rect(), rules.rounding, fill);
        }
        let result = egui::Frame::new()
            .inner_margin(margin)
            .show(ui, |ui| unsafe { run_nested_update_func(ui, update_func) })
            .inner;
        if let Err(err) = result {
            self.result = Err(err);
        }
        UiResponse::None
    }

    fn is_tab_closable(&self, tiles: &Tiles<Pane>, tile_id: TileId) -> bool {
        matches!(tiles.get(tile_id), Some(Tile::Pane(pane)) if pane.closable)
    }

    /// Same as the default tab_ui of egui_tiles, with rounded top corners, a hovered fill and
    /// spacing between the tabs
    fn tab_ui(
        &mut self,
        tiles: &mut Tiles<Pane>,
        ui: &mut Ui,
        id: Id,
        tile_id: TileId,
        state: &TabState,
    ) -> Response {
        let text = self.tab_title_for_tile(tiles, tile_id);
        let close_btn_size = Vec2::splat(self.close_button_outer_size());
        let close_btn_left_padding = 4.0;
        let font_id = TextStyle::Button.resolve(ui.style());
        let galley = text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, font_id);

        let x_margin = self.tab_title_spacing(ui.visuals());
        let spacing = self.style.tab_spacing();

        let button_width = galley.size().x
            + 2.0 * x_margin
            + f32::from(state.closable) * (close_btn_left_padding + close_btn_size.x);
        let (_, outer_rect) = ui.allocate_space(egui::vec2(button_width + spacing, ui.available_height()));
        // the spacing is on the right, so the first tab stays aligned with the panes
        let tab_rect = egui::Rect::from_min_size(
            outer_rect.min,
            egui::vec2(button_width, outer_rect.height()),
        );

        let draggable = self.is_tile_draggable(tiles, tile_id);
        let sense = if draggable {
            Sense::click_and_drag()
        } else {
            Sense::click()
        };
        let tab_response = ui.interact(tab_rect, id, sense);
        let tab_response = if draggable {
            tab_response.on_hover_cursor(self.tab_hover_cursor_icon())
        } else {
            tab_response
        };
        tab_response.widget_info(|| {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), state.active, galley.text())
        });

        if state.active && !self.style.pill {
            // the outline of the pane is open under its tab, clipped to the visible part of the bar
            self.active_tabs
                .insert(tile_id, tab_rect.intersect(ui.clip_rect()));
        }

        // show a gap when dragged
        if ui.is_rect_visible(tab_rect) && !state.is_being_dragged {
            let mut bg_color = self.tab_bg_color(ui.visuals(), tiles, tile_id, state);
            if !state.active && tab_response.hovered() {
                bg_color = self
                    .style
                    .hovered_tab_fill
                    .unwrap_or(if self.style.pill {
                        ui.visuals().widgets.hovered.weak_bg_fill
                    } else {
                        DockStyle::inactive_tint(ui.visuals(), true)
                    });
            }
            let stroke = self.tab_outline_stroke(ui.visuals(), tiles, tile_id, state);
            if self.style.pill {
                // a capsule floating in the bar
                let pill = tab_rect.shrink2(egui::vec2(0.0, 3.0));
                let radius = (pill.height() / 2.0).min(255.0) as u8;
                ui.painter().rect(
                    pill,
                    CornerRadius::same(radius),
                    bg_color,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            } else {
                // the radius of the theme, set with ctx.set_style(rounding=...)
                let radius = ui.visuals().widgets.inactive.corner_radius.nw;
                let corners = CornerRadius {
                    nw: radius,
                    ne: radius,
                    sw: 0,
                    se: 0,
                };
                ui.painter().rect_filled(tab_rect, corners, bg_color);
                if stroke.width > 0.0 {
                    // open at the bottom, so the tab connects with its pane
                    ui.painter().add(egui::Shape::line(
                        tab_outline(tab_rect.shrink(stroke.width / 2.0), f32::from(radius)),
                        stroke,
                    ));
                }
            }

            let text_color = self.tab_text_color(ui.visuals(), tiles, tile_id, state);
            let text_position = egui::Align2::LEFT_CENTER
                .align_size_within_rect(galley.size(), tab_rect.shrink(x_margin))
                .min;
            ui.painter().galley(text_position, galley, text_color);

            if state.closable {
                let close_btn_rect = egui::Align2::RIGHT_CENTER
                    .align_size_within_rect(close_btn_size, tab_rect.shrink(x_margin));
                let close_btn_id = ui.auto_id_with("tab_close_btn");
                let close_btn_response = ui
                    .interact(close_btn_rect, close_btn_id, Sense::click_and_drag())
                    .on_hover_cursor(egui::CursorIcon::Default);
                close_btn_response
                    .widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), "Close"));

                let visuals = ui.style().interact(&close_btn_response);
                let rect = close_btn_rect
                    .shrink(self.close_button_inner_margin())
                    .expand(visuals.expansion);
                let stroke = visuals.fg_stroke;
                ui.painter()
                    .line_segment([rect.left_top(), rect.right_bottom()], stroke);
                ui.painter()
                    .line_segment([rect.right_top(), rect.left_bottom()], stroke);

                if (close_btn_response.clicked()
                    || tab_response.clicked_by(egui::PointerButton::Middle))
                    && self.on_tab_close(tiles, tile_id)
                {
                    tiles.remove(tile_id);
                }
            }
        }

        self.on_tab_button(tiles, tile_id, tab_response)
    }

    fn tab_bar_left_ui(
        &mut self,
        tiles: &Tiles<Pane>,
        ui: &mut Ui,
        tile_id: TileId,
        tabs: &egui_tiles::Tabs,
    ) {
        let callback = self.callbacks.left.as_ref().map(|f| f.clone_ref(self.py));
        self.tab_bar_ui(callback.as_ref(), tiles, ui, tile_id, tabs);
    }

    fn tab_bar_trailing_ui(
        &mut self,
        tiles: &Tiles<Pane>,
        ui: &mut Ui,
        tile_id: TileId,
        tabs: &egui_tiles::Tabs,
    ) {
        let callback = self.callbacks.trailing.as_ref().map(|f| f.clone_ref(self.py));
        self.tab_bar_ui(callback.as_ref(), tiles, ui, tile_id, tabs);
    }

    fn top_bar_right_ui(
        &mut self,
        tiles: &Tiles<Pane>,
        ui: &mut Ui,
        tile_id: TileId,
        tabs: &egui_tiles::Tabs,
        _scroll_offset: &mut f32,
    ) {
        let callback = self.callbacks.right.as_ref().map(|f| f.clone_ref(self.py));
        self.tab_bar_ui(callback.as_ref(), tiles, ui, tile_id, tabs);
    }

    fn paint_on_top_of_tile(
        &self,
        painter: &egui::Painter,
        style: &egui::Style,
        tile_id: TileId,
        rect: egui::Rect,
    ) {
        if !self.panes.contains(&tile_id) {
            return;
        }
        let stroke = self.style.pane_stroke(&style.visuals);
        if stroke.width <= 0.0 {
            return;
        }
        let radius = crate::layout::dock_rules()
            .map_or_else(|| f32::from(style.visuals.widgets.inactive.corner_radius.sw), |rules| rules.rounding);
        let tab = self
            .active_tabs
            .get(&tile_id)
            .filter(|tab| tab.width() > 0.0 && tab.max.y <= rect.min.y + 0.5);
        // reach the bottom of the tab outline, drawn in the tab bar right above the pane
        let painter = egui::Painter::new(
            painter.ctx().clone(),
            painter.layer_id(),
            rect.expand2(egui::vec2(0.0, stroke.width)),
        );
        painter.add(egui::Shape::line(
            pane_outline(rect, tab.copied(), radius, stroke.width),
            stroke,
        ));
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        self.style.tab_bar_height.unwrap_or(if self.style.glass { 28.0 } else { 24.0 })
    }

    fn is_tab_bar_visible(&self, _tile_id: TileId, tabs: &egui_tiles::Tabs) -> bool {
        if tabs.children.len() == 1 && self.hidden_alone.contains(&tabs.children[0]) {
            return false;
        }
        !self.style.hide_single_tab_bar || tabs.children.len() > 1
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        self.style.gap_width.unwrap_or_else(|| match crate::layout::dock_rules() {
            Some(rules) => rules.gap,
            None => {
                if self.style.glass {
                    6.0
                } else {
                    1.0
                }
            }
        })
    }

    fn resize_stroke(&self, style: &egui::Style, resize_state: ResizeState) -> Stroke {
        match (resize_state, self.style.resize_stroke) {
            (ResizeState::Idle, Some(stroke)) => stroke,
            // the gap stays see-through, the line only shows when hovered or dragged
            (ResizeState::Idle, None) if self.style.glass || crate::layout::dock_rules().is_some() => Stroke::NONE,
            (ResizeState::Idle, None) => Stroke::new(
                self.gap_width(style),
                style.visuals.widgets.noninteractive.bg_stroke.color,
            ),
            (ResizeState::Hovering, _) => style.visuals.widgets.hovered.fg_stroke,
            (ResizeState::Dragging, _) => style.visuals.widgets.active.fg_stroke,
        }
    }

    fn tab_title_spacing(&self, _visuals: &Visuals) -> f32 {
        self.style.tab_title_spacing.unwrap_or(if self.style.glass { 12.0 } else { 8.0 })
    }

    fn tab_bar_color(&self, _visuals: &Visuals) -> Color32 {
        // transparent, so the active tab shows the same background as its pane
        self.style.tab_bar_color.unwrap_or(Color32::TRANSPARENT)
    }

    fn tab_bg_color(
        &self,
        visuals: &Visuals,
        _tiles: &Tiles<Pane>,
        _tile_id: TileId,
        state: &TabState,
    ) -> Color32 {
        match (state.active, self.style.pill) {
            // pill: the active tab is filled with the selection color, the others are bare
            (true, true) => self.style.active_tab_fill.unwrap_or(visuals.selection.bg_fill),
            (false, true) => self.style.inactive_tab_fill.unwrap_or(Color32::TRANSPARENT),
            // classic: the active tab has the background of its pane, the others are tinted
            (true, false) => self.style.active_tab_fill.unwrap_or(Color32::TRANSPARENT),
            (false, false) => self
                .style
                .inactive_tab_fill
                .unwrap_or(DockStyle::inactive_tint(visuals, false)),
        }
    }

    fn tab_outline_stroke(
        &self,
        visuals: &Visuals,
        _tiles: &Tiles<Pane>,
        _tile_id: TileId,
        state: &TabState,
    ) -> Stroke {
        // pill tabs have no outline unless one is given
        if self.style.pill {
            return if state.active {
                self.style.active_tab_stroke.unwrap_or(Stroke::NONE)
            } else {
                self.style.inactive_tab_stroke.unwrap_or(Stroke::NONE)
            };
        }
        // fainter on inactive tabs
        if state.active {
            self.style.active_tab_stroke(visuals)
        } else {
            self.style
                .inactive_tab_stroke
                .unwrap_or_else(|| match crate::layout::dock_rules() {
                    Some(rules) => rules.outline,
                    None => Stroke::new(1.0, DockStyle::border(visuals, 25)),
                })
        }
    }

    fn tab_text_color(
        &self,
        visuals: &Visuals,
        _tiles: &Tiles<Pane>,
        _tile_id: TileId,
        state: &TabState,
    ) -> Color32 {
        if state.active && self.style.pill {
            // readable on the selection color
            self.style
                .active_tab_text_color
                .unwrap_or(visuals.selection.stroke.color)
        } else if state.active {
            self.style
                .active_tab_text_color
                .unwrap_or(visuals.widgets.active.text_color())
        } else {
            self.style
                .inactive_tab_text_color
                .unwrap_or(visuals.widgets.noninteractive.text_color().gamma_multiply(0.75))
        }
    }

    fn drag_preview_stroke(&self, visuals: &Visuals) -> Stroke {
        self.style.drag_preview_stroke.unwrap_or(visuals.selection.stroke)
    }

    fn drag_preview_color(&self, visuals: &Visuals) -> Color32 {
        self.style
            .drag_preview_color
            .unwrap_or(visuals.selection.stroke.color.gamma_multiply(0.5))
    }

    fn simplification_options(&self) -> SimplificationOptions {
        // every pane has a tab, so it can be dragged somewhere else
        SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..SimplificationOptions::default()
        }
    }
}

/// Shows a Dock, filling the available space.
///
/// Example::
///
///     dock = Dock(DockContainer("tabs", [DockPane("A", lambda: label("a")), DockPane("B", lambda: label("b"))]))
///
///     def update_func(ctx):
///         dock_area(dock)
#[pyfunction]
unsafe fn dock_area(py: Python<'_>, dock: Py<Dock>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let (mut tree, style, callbacks) = {
        let mut dock = dock.borrow_mut(py);
        let tree = dock.tree.take().ok_or(PyRuntimeError::new_err(
            "dock_area cannot be called inside a pane of the same dock",
        ))?;
        (tree, dock.style.clone(), dock.callbacks.clone_ref(py))
    };

    let hidden_alone = tree
        .tiles
        .iter()
        .filter(|(_, tile)| matches!(tile, Tile::Pane(pane) if pane.hide_tab))
        .map(|(id, _)| *id)
        .collect();
    let actions = TabBarActions::default();
    let mut behavior = Behavior {
        py,
        result: Ok(()),
        style,
        callbacks,
        actions: actions.clone(),
        panes: Default::default(),
        hidden_alone,
        active_tabs: Default::default(),
    };
    tree.ui(&mut behavior, ui);

    let actions = std::mem::take(&mut *actions.lock().unwrap_or_else(|e| e.into_inner()));
    for (tabs_id, action) in actions {
        apply_tab_bar_action(&mut tree, tabs_id, action);
    }

    let mut dock = dock.borrow_mut(py);
    for pane in std::mem::take(&mut dock.pending) {
        add_pane_to_tree(&mut tree, pane);
    }
    dock.tree = Some(tree);

    behavior.result
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<DockPane>()?;
    m.add_class::<DockContainer>()?;
    m.add_class::<DockTabBar>()?;
    m.add_class::<Dock>()?;
    m.add_function(wrap_pyfunction!(dock_area, m)?)?;
    Ok(())
}
