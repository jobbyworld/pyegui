use eframe::egui;
use egui_ltreeview::{Action, DirPosition, NodeBuilder, TreeViewBuilder, TreeViewState};
use pyo3::prelude::*;

use crate::{current_ui, UI};

/// A node of a tree view. It is a directory if children is a list, even an empty one, and a leaf otherwise.
///
/// Ids must be unique in the tree.
///
/// Args:
///     id (int): unique id of the node
///
///     label (str): the text of the node
///
///     children (list[TreeNode]): the nodes of a directory, None for a leaf
///
///     icon (str): uri of an image shown before the label, like for image(), e.g. from file_icon
///
/// Usage::
///
///     nodes = [
///         TreeNode(0, "src", [TreeNode(1, "main.py"), TreeNode(2, "utils.py")]),
///         TreeNode(3, "README.md", icon=file_icon("README.md", 16)),
///     ]
#[pyclass]
pub(crate) struct TreeNode {
    #[pyo3(get, set)]
    id: i64,
    #[pyo3(get, set)]
    label: String,
    #[pyo3(get, set)]
    children: Option<Vec<Py<TreeNode>>>,
    #[pyo3(get, set)]
    icon: Option<String>,
}

#[pymethods]
impl TreeNode {
    #[new]
    #[pyo3(signature = (id, label, children=None, icon=None))]
    fn new(id: i64, label: String, children: Option<Vec<Py<TreeNode>>>, icon: Option<String>) -> Self {
        TreeNode {
            id,
            label,
            children,
            icon,
        }
    }
}

/// TreeView stores the state of a tree view: selected nodes, opened directories and user actions.
///
/// Args:
///     id (str): unique id of the tree view
///
///     multi_select (bool): allow selecting several nodes with ctrl and shift. Default is False
///
///     drag_and_drop (bool): allow moving nodes by dragging them. Default is False
///
///     default_open (bool): directories are open the first time they are shown. Default is True.
///         Use False to load the children of a directory only when the user opens it, see is_open
///
/// Usage::
///
///     tree = TreeView("files")
///
///     def update_func(ctx):
///         tree_view(tree, nodes)
///         label(f"selected: {tree.selected}")
///         if tree.activated:
///             print("double clicked:", tree.activated)
#[pyclass]
pub(crate) struct TreeView {
    id: String,
    multi_select: bool,
    drag_and_drop: bool,
    default_open: bool,
    state: TreeViewState<i64>,
    activated: Vec<i64>,
    moved: Option<(Vec<i64>, i64, &'static str, Option<i64>)>,
}

#[pymethods]
impl TreeView {
    #[new]
    #[pyo3(signature = (id, multi_select=false, drag_and_drop=false, default_open=true))]
    fn new(id: String, multi_select: bool, drag_and_drop: bool, default_open: bool) -> Self {
        TreeView {
            id,
            multi_select,
            drag_and_drop,
            default_open,
            state: TreeViewState::default(),
            activated: vec![],
            moved: None,
        }
    }

    /// Ids of the selected nodes
    #[getter]
    fn get_selected(&self) -> Vec<i64> {
        self.state.selected().clone()
    }

    #[setter]
    fn set_selected(&mut self, selected: Vec<i64>) {
        self.state.set_selected(selected);
    }

    /// Ids of the nodes activated by a double click or enter during the last frame
    #[getter]
    fn activated(&self) -> Vec<i64> {
        self.activated.clone()
    }

    /// Nodes dropped during the last frame, if drag_and_drop is enabled.
    ///
    /// It is None or a tuple (source_ids, target_id, position, position_id), where position is
    /// "first", "last", "before" or "after" and position_id is the id of the node the sources
    /// are placed before or after. You have to move the nodes yourself.
    #[getter]
    fn moved(&self) -> Option<(Vec<i64>, i64, &'static str, Option<i64>)> {
        self.moved.clone()
    }

    /// Opens or closes a directory
    fn set_open(&mut self, id: i64, open: bool) {
        self.state.set_openness(id, open);
    }

    /// True if the directory is open. Useful to load the children of a directory only when it is
    /// opened: give it an empty list of children until then
    fn is_open(&self, id: i64) -> bool {
        self.state.is_open(&id).unwrap_or(false)
    }
}

/// Shows the image at icon before the label of the node
fn with_icon<'a>(config: NodeBuilder<'a, i64>, icon: Option<String>) -> NodeBuilder<'a, i64> {
    match icon {
        Some(uri) => config.icon(move |ui| {
            let side = ui.text_style_height(&egui::TextStyle::Body);
            ui.add(egui::Image::new(uri.as_str()).fit_to_exact_size(egui::vec2(side, side)));
            ui.add_space(4.0);
        }),
        None => config,
    }
}

fn add_nodes(
    py: Python<'_>,
    builder: &mut TreeViewBuilder<'_, i64>,
    nodes: &[Py<TreeNode>],
    default_open: bool,
) {
    for node in nodes {
        let node = node.borrow(py);

        match &node.children {
            Some(children) => {
                builder.node(with_icon(
                    NodeBuilder::dir(node.id)
                        .default_open(default_open)
                        .label(node.label.as_str()),
                    node.icon.clone(),
                ));
                add_nodes(py, builder, children, default_open);
                builder.close_dir();
            }
            None => {
                builder.node(with_icon(
                    NodeBuilder::leaf(node.id).label(node.label.as_str()),
                    node.icon.clone(),
                ));
            }
        }
    }
}

/// Shows a tree of nodes. Directories can be opened and nodes selected, with keyboard navigation.
///
/// Example::
///
///     tree = TreeView("files")
///     nodes = [
///         TreeNode(0, "src", [TreeNode(1, "main.py")]),
///         TreeNode(2, "README.md"),
///     ]
///
///     def update_func(ctx):
///         tree_view(tree, nodes)
#[pyfunction]
unsafe fn tree_view(py: Python<'_>, tree: &mut TreeView, nodes: Vec<Py<TreeNode>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let id = ui.make_persistent_id(&tree.id);

    let (_, actions) = egui_ltreeview::TreeView::new(id)
        .allow_multi_selection(tree.multi_select)
        .allow_drag_and_drop(tree.drag_and_drop)
        .show_state(ui, &mut tree.state, |builder| {
            add_nodes(py, builder, &nodes, tree.default_open)
        });

    tree.activated.clear();
    tree.moved = None;

    for action in actions {
        match action {
            Action::Activate(activate) => tree.activated = activate.selected,
            Action::Move(dnd) => {
                let (position, position_id) = match dnd.position {
                    DirPosition::First => ("first", None),
                    DirPosition::Last => ("last", None),
                    DirPosition::Before(id) => ("before", Some(id)),
                    DirPosition::After(id) => ("after", Some(id)),
                };
                tree.moved = Some((dnd.source, dnd.target, position, position_id));
            }
            _ => {}
        }
    }

    Ok(())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TreeNode>()?;
    m.add_class::<TreeView>()?;
    m.add_function(wrap_pyfunction!(tree_view, m)?)?;
    Ok(())
}
