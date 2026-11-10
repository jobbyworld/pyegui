use eframe::egui;
use egui_tiles::{Container, SimplificationOptions, Tile, TileId, Tiles, UiResponse};
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyAny;

use crate::{current_ui, run_nested_update_func, UI};

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

/// Dock stores a layout of panes that the user can resize, rearrange by dragging tabs, and close.
///
/// Args:
///     root (DockPane | DockContainer): initial layout
///
///     id (str): unique id of the dock. Default is "dock"
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
#[pyclass]
pub(crate) struct Dock {
    // taken out while the dock is shown, so panes can call add_pane
    tree: Option<egui_tiles::Tree<Pane>>,
    pending: Vec<Pane>,
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
    #[pyo3(signature = (root, id="dock"))]
    fn new(root: &Bound<'_, PyAny>, id: &str) -> PyResult<Self> {
        let mut tiles = Tiles::default();
        let root = insert_tile(&mut tiles, root)?;

        Ok(Dock {
            tree: Some(egui_tiles::Tree::new(egui::Id::unique(id), root, tiles)),
            pending: vec![],
        })
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
    let mut tree = dock
        .borrow_mut(py)
        .tree
        .take()
        .ok_or(PyRuntimeError::new_err("dock_area cannot be called inside a pane of the same dock"))?;

    let mut behavior = Behavior { py, result: Ok(()) };
    tree.ui(&mut behavior, ui);

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
    m.add_class::<Dock>()?;
    m.add_function(wrap_pyfunction!(dock_area, m)?)?;
    Ok(())
}
