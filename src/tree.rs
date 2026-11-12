use eframe::egui;
use egui_ltreeview::{Action, DirPosition, NodeBuilder, TreeViewBuilder, TreeViewState};
use pyo3::prelude::*;

use crate::{current_ui, run_nested_call, UI};

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
///     badges (list[tuple]): colors (r, g, b, a) of dots drawn at the right end of the row, e.g.
///         the tags of a file
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
    #[pyo3(get, set)]
    badges: Option<Vec<(u8, u8, u8, u8)>>,
}

#[pymethods]
impl TreeNode {
    #[new]
    #[pyo3(signature = (id, label, children=None, icon=None, badges=None))]
    fn new(
        id: i64,
        label: String,
        children: Option<Vec<Py<TreeNode>>>,
        icon: Option<String>,
        badges: Option<Vec<(u8, u8, u8, u8)>>,
    ) -> Self {
        TreeNode {
            id,
            label,
            children,
            icon,
            badges,
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
///     activatable_dirs (bool): a double click or Enter on a directory activates it, like a leaf,
///         see activated. Its arrow still opens and closes it. Default is False: a double click
///         opens and closes the directory
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
    activatable_dirs: bool,
    state: TreeViewState<i64>,
    activated: Vec<i64>,
    moved: Option<(Vec<i64>, i64, &'static str, Option<i64>)>,
    focus: Option<bool>,
    keep_focus: bool,
}

#[pymethods]
impl TreeView {
    #[new]
    #[pyo3(signature = (id, multi_select=false, drag_and_drop=false, default_open=true, activatable_dirs=false))]
    fn new(
        id: String,
        multi_select: bool,
        drag_and_drop: bool,
        default_open: bool,
        activatable_dirs: bool,
    ) -> Self {
        TreeView {
            id,
            multi_select,
            drag_and_drop,
            default_open,
            activatable_dirs,
            state: TreeViewState::default(),
            activated: vec![],
            moved: None,
            focus: None,
            keep_focus: false,
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

    /// Gives the keyboard focus to the tree (True, the default) or takes it away (False) at the
    /// next tree_view. With the focus the arrows, Enter and the other keys work without a click,
    /// so an app that manages its own focus can hand it to the tree, or to another part of the
    /// window
    #[pyo3(signature = (focused=true))]
    fn focus(&mut self, focused: bool) {
        self.focus = Some(focused);
    }

    /// If True, the tree takes back the keyboard focus at every tree_view when another widget
    /// holds it, except a text field being edited. For an app where the tree is the part of the
    /// window the keyboard acts on: a click on a button or a blank area does not cut the arrows
    #[getter]
    fn get_keep_focus(&self) -> bool {
        self.keep_focus
    }

    #[setter]
    fn set_keep_focus(&mut self, keep: bool) {
        self.keep_focus = keep;
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

/// Draws the badges of the node as dots at the right end of its row
fn with_badges<'a>(config: NodeBuilder<'a, i64>, badges: Option<Vec<(u8, u8, u8, u8)>>) -> NodeBuilder<'a, i64> {
    match badges {
        Some(colors) if !colors.is_empty() => config.accessory(move |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (r, g, b, a) in &colors {
                ui.colored_label(egui::Color32::from_rgba_unmultiplied(*r, *g, *b, *a), "●");
            }
        }),
        _ => config,
    }
}

/// The position of a node among the rows shown: the children of the closed directories are not
fn row_index(py: Python<'_>, tree: &TreeView, nodes: &[Py<TreeNode>], id: i64, row: &mut usize) -> Option<usize> {
    for node in nodes {
        let node = node.borrow(py);
        if node.id == id {
            return Some(*row);
        }
        *row += 1;
        if let Some(children) = &node.children {
            if tree.state.is_open(&node.id).unwrap_or(tree.default_open) {
                if let Some(found) = row_index(py, tree, children, id, row) {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn add_nodes(
    py: Python<'_>,
    builder: &mut TreeViewBuilder<'_, i64>,
    nodes: &[Py<TreeNode>],
    default_open: bool,
    activatable_dirs: bool,
) {
    for node in nodes {
        let node = node.borrow(py);

        match &node.children {
            Some(children) => {
                builder.node(with_badges(
                    with_icon(
                        NodeBuilder::dir(node.id)
                            .default_open(default_open)
                            .activatable(activatable_dirs)
                            .label(node.label.as_str()),
                        node.icon.clone(),
                    ),
                    node.badges.clone(),
                ));
                add_nodes(py, builder, children, default_open, activatable_dirs);
                builder.close_dir();
            }
            None => {
                builder.node(with_badges(
                    with_icon(
                        NodeBuilder::leaf(node.id).label(node.label.as_str()),
                        node.icon.clone(),
                    ),
                    node.badges.clone(),
                ));
            }
        }
    }
}

/// Shows a tree of nodes. Directories can be opened and nodes selected, with keyboard navigation.
///
/// Args:
///     tree (TreeView): the state of the tree
///
///     nodes (list[TreeNode]): the nodes
///
///     scroll_to (int): id of a node to scroll to, just enough to show it. Give it only during
///         the frame the tree has to scroll. The tree has to be the first thing of a scroll area
///
///     context_menu (callable): called with the list of ids concerned when a node is right-clicked,
///         inside a menu: the selected nodes if the node clicked is one of them, else only the
///         node clicked. Build the menu in it, or call close_menu() to act without a menu
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
///         # a menu on right click
///         tree_view(tree, nodes, context_menu=lambda ids: button_clicked(f"{len(ids)} selected"))
#[pyfunction]
#[pyo3(signature = (tree, nodes, scroll_to=None, context_menu=None))]
unsafe fn tree_view(
    py: Python<'_>,
    tree: &mut TreeView,
    nodes: Vec<Py<TreeNode>>,
    scroll_to: Option<i64>,
    context_menu: Option<Bound<'_, PyAny>>,
) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    if let Some(id) = scroll_to {
        // the rows out of the window are not drawn, so the row is placed from its index
        if let Some(index) = row_index(py, tree, &nodes, id, &mut 0) {
            let height = ui.spacing().interact_size.y;
            let top = ui.cursor().min + egui::vec2(0.0, index as f32 * (height + ui.spacing().item_spacing.y));
            ui.scroll_to_rect(egui::Rect::from_min_size(top, egui::vec2(1.0, height)), None);
        }
    }
    let id = ui.make_persistent_id(&tree.id);
    match tree.focus.take() {
        Some(true) => ui.memory_mut(|m| m.request_focus(id)),
        Some(false) => ui.memory_mut(|m| m.surrender_focus(id)),
        None if tree.keep_focus && !ui.ctx().text_edit_focused() => {
            ui.memory_mut(|m| m.request_focus(id))
        }
        None => {}
    }

    let mut menu_result = Ok(());
    let mut view = egui_ltreeview::TreeView::new(id)
        .allow_multi_selection(tree.multi_select)
        .allow_drag_and_drop(tree.drag_and_drop);
    if let Some(menu_fun) = &context_menu {
        // the menu is drawn in its own ui: the Python function runs on top of the ui stack
        view = view.fallback_context_menu(|menu_ui, ids| {
            menu_result = run_nested_call(menu_ui, menu_fun, (ids.clone(),));
        });
    }
    let (_, actions) = view.show_state(ui, &mut tree.state, |builder| {
        add_nodes(py, builder, &nodes, tree.default_open, tree.activatable_dirs)
    });
    menu_result?;

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
