use eframe::egui;
use egui::{Color32, CornerRadius, Id, Response, Role, Sense, Stroke, TextStyle, Ui, Vec2, Visuals, WidgetInfo};
use egui_tiles::{
    Container, ResizeState, SimplificationOptions, TabState, Tile, TileId, Tiles, UiResponse,
};
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};

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
}

#[pymethods]
impl DockPane {
    #[new]
    #[pyo3(signature = (title, update_func, closable=false))]
    fn new(title: String, update_func: Py<PyAny>, closable: bool) -> Self {
        DockPane {
            title,
            update_func,
            closable,
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
}

impl Pane {
    fn from_py(pane: &DockPane, py: Python<'_>) -> Self {
        Pane {
            title: pane.title.clone(),
            update_func: pane.update_func.clone_ref(py),
            closable: pane.closable,
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

/// Panes added from the tab bar callbacks, with the tabs container they go to
type AddedPanes = Arc<Mutex<Vec<(TileId, Pane)>>>;

/// A tab bar of a Dock, given to the tab_bar_left, tab_bar_trailing and tab_bar_right callbacks.
///
/// Usage::
///
///     def new_tab_button(tab_bar):
///         if button_clicked("+"):
///             tab_bar.add_pane(DockPane(f"Untitled {len(tab_bar.titles) + 1}", editor_ui, closable=True))
///
///     dock = Dock(root, tab_bar_trailing=new_tab_button)
#[pyclass]
pub(crate) struct DockTabBar {
    tile_id: TileId,
    titles: Vec<String>,
    active_title: Option<String>,
    added: AddedPanes,
}

#[pymethods]
impl DockTabBar {
    /// Adds a pane to these tabs and makes it the active tab. The pane shows up on the next frame
    fn add_pane(&self, py: Python<'_>, pane: PyRef<'_, DockPane>) -> PyResult<()> {
        self.added
            .lock()
            .map_err(|_| PyRuntimeError::new_err("dock lock is poisoned"))?
            .push((self.tile_id, Pane::from_py(&pane, py)));
        Ok(())
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
}

const STYLE_KEYS: [&str; 17] = [
    "glass",
    "tab_bar_height",
    "tab_bar_color",
    "hide_single_tab_bar",
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
];

impl DockStyle {
    fn tab_spacing(&self) -> f32 {
        self.tab_spacing.unwrap_or(if self.glass { 4.0 } else { 2.0 })
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
///     active_tab_stroke (tuple[float, color]): outline of the active tab. Default is none
///
///     inactive_tab_stroke (tuple[float, color]): outline of the other tabs. Default is none
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
    added: AddedPanes,
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
            added: self.added.clone(),
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

    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile_id: TileId, pane: &mut Pane) -> UiResponse {
        let update_func = pane.update_func.bind(self.py).clone();

        if let Err(err) = unsafe { run_nested_update_func(ui, update_func) } {
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

        // show a gap when dragged
        if ui.is_rect_visible(tab_rect) && !state.is_being_dragged {
            let mut bg_color = self.tab_bg_color(ui.visuals(), tiles, tile_id, state);
            if !state.active && tab_response.hovered() {
                bg_color = self
                    .style
                    .hovered_tab_fill
                    .unwrap_or(DockStyle::inactive_tint(ui.visuals(), true));
            }
            let stroke = self.tab_outline_stroke(ui.visuals(), tiles, tile_id, state);
            // the radius of the theme, set with ctx.set_style(rounding=...)
            let radius = ui.visuals().widgets.inactive.corner_radius.nw;
            let corners = CornerRadius {
                nw: radius,
                ne: radius,
                sw: 0,
                se: 0,
            };
            ui.painter().rect(
                tab_rect.shrink(0.5),
                corners,
                bg_color,
                stroke,
                egui::StrokeKind::Inside,
            );

            if state.active && stroke.width > 0.0 {
                // open the outline at the bottom, so the tab connects with its pane
                ui.painter().hline(
                    tab_rect.shrink2(egui::vec2(stroke.width, 0.0)).x_range(),
                    tab_rect.bottom() - stroke.width / 2.0,
                    Stroke::new(stroke.width + 1.0, ui.visuals().panel_fill),
                );
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

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        self.style.tab_bar_height.unwrap_or(if self.style.glass { 28.0 } else { 24.0 })
    }

    fn is_tab_bar_visible(&self, _tile_id: TileId, tabs: &egui_tiles::Tabs) -> bool {
        !self.style.hide_single_tab_bar || tabs.children.len() > 1
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        self.style.gap_width.unwrap_or(if self.style.glass { 6.0 } else { 1.0 })
    }

    fn resize_stroke(&self, style: &egui::Style, resize_state: ResizeState) -> Stroke {
        match (resize_state, self.style.resize_stroke) {
            (ResizeState::Idle, Some(stroke)) => stroke,
            // the gap stays see-through, the line only shows when hovered or dragged
            (ResizeState::Idle, None) if self.style.glass => Stroke::NONE,
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
        if state.active {
            self.style.active_tab_fill.unwrap_or(Color32::TRANSPARENT)
        } else {
            self.style
                .inactive_tab_fill
                .unwrap_or(DockStyle::inactive_tint(visuals, false))
        }
    }

    fn tab_outline_stroke(
        &self,
        _visuals: &Visuals,
        _tiles: &Tiles<Pane>,
        _tile_id: TileId,
        state: &TabState,
    ) -> Stroke {
        if state.active {
            self.style.active_tab_stroke.unwrap_or(Stroke::NONE)
        } else {
            self.style.inactive_tab_stroke.unwrap_or(Stroke::NONE)
        }
    }

    fn tab_text_color(
        &self,
        visuals: &Visuals,
        _tiles: &Tiles<Pane>,
        _tile_id: TileId,
        state: &TabState,
    ) -> Color32 {
        if state.active {
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

    let added = AddedPanes::default();
    let mut behavior = Behavior {
        py,
        result: Ok(()),
        style,
        callbacks,
        added: added.clone(),
    };
    tree.ui(&mut behavior, ui);

    let added = std::mem::take(&mut *added.lock().unwrap_or_else(|e| e.into_inner()));
    for (tabs_id, pane) in added {
        add_pane_to_tabs(&mut tree, tabs_id, pane);
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
