//! File views: FileListView, FileIconView and FileTreeView show folders of the disk, with
//! selection, keyboard, sorting and native icons. Their folders are listed through a FileWatcher,
//! so a change made by any program shows at the next frame.
//!
//! The views are in `pyegui_core::files`; this file holds the Python side: the classes, the
//! keyword args, and the functions of the Python code (badges, extra columns, context menus) that
//! the core calls as closures.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use eframe::egui::{self, Color32};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use pyegui_core::files::{
    default_headers, draw_folder, draw_tree, list_columns, Extra, FolderView, Hooks, Menus, Shape,
    Shared, SortBy, TreeParams, TreeState, Vec2Size, DATE_FORMAT, ICON_SIZES,
};

use crate::columns::{
    filters_py, header_options, parse_type, set_filters, value_from_py, widths_py,
};
use crate::style::extract_color;
use crate::watcher::{default_core, expand, Core, FileWatcher};
use crate::{current_ui, kwarg, run_nested_call, UI};

fn core_of(watcher: Option<PyRef<'_, FileWatcher>>) -> Arc<Mutex<Core>> {
    match watcher {
        Some(w) => w.core.clone(),
        None => default_core(),
    }
}

fn to_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The context menus to open after the view is drawn, once the Python object is free again: the
/// function may read or change the view
unsafe fn run_menus(menus: Menus, menu: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
    let Some(menu) = menu else { return Ok(()) };
    let mut result = Ok(());
    for (response, paths) in menus {
        response.context_menu(|ui| {
            if let Err(err) = run_nested_call(ui, menu, (paths.clone(),)) {
                result = Err(err);
            }
        });
    }
    result
}

/// What the Python code adds to the items of a view: the functions of its keyword args, given to
/// the core as closures. An error of a function is printed and gives no dot, no value, no cell
fn read_hooks<'py>(kwargs: Option<&Bound<'py, PyDict>>) -> PyResult<Hooks<'py>> {
    let mut extras = vec![];
    if let Some(columns) = kwarg::<Vec<Bound<'py, PyTuple>>>(kwargs, "extra_columns")? {
        for column in columns {
            if column.len() != 3 && column.len() != 4 {
                return Err(PyValueError::new_err(
                    "an extra column is (title, width, cell) or (title, width, cell, options)",
                ));
            }
            let (mut ty, mut value, mut icon) = (pyegui_core::columns::DataType::Text, None, None);
            if column.len() == 4 {
                let options = column.get_item(3)?;
                let options = options.cast::<PyDict>()?;
                if let Some(t) = kwarg::<String>(Some(options), "type")? {
                    ty = parse_type(&t)?;
                }
                value = kwarg::<Bound<'py, PyAny>>(Some(options), "value")?;
                if let Some(file) = kwarg::<String>(Some(options), "icon")? {
                    let bytes = std::fs::read(&file).map_err(|e| PyValueError::new_err(format!("icon {file}: {e}")))?;
                    icon = Some(pyegui_core::columns::ColumnIcon { name: file, bytes: bytes.into() });
                }
            }
            let cell: Bound<'py, PyAny> = column.get_item(2)?;
            extras.push(Extra {
                title: column.get_item(0)?.extract()?,
                width: column.get_item(1)?.extract()?,
                icon,
                cell: Box::new(move |ui, path| {
                    if let Err(err) = unsafe { run_nested_call(ui, &cell, (to_str(path),)) } {
                        err.display(cell.py());
                    }
                }),
                ty,
                value: value.map(|function| {
                    Box::new(move |path: &Path| {
                        function
                            .call1((to_str(path),))
                            .map(|r| value_from_py(&r, ty))
                            .unwrap_or_else(|err| {
                                err.display(function.py());
                                pyegui_core::columns::Value::None
                            })
                    }) as Box<dyn Fn(&Path) -> pyegui_core::columns::Value + 'py>
                }),
            });
        }
    }
    let badges = kwarg::<Bound<'py, PyAny>>(kwargs, "badges")?.map(|function| {
        Box::new(move |path: &Path| -> Vec<Color32> {
            let result = function
                .call1((to_str(path),))
                .and_then(|colors| colors.extract::<Vec<Bound<'py, PyAny>>>().map_err(Into::into))
                .and_then(|colors| colors.iter().map(extract_color).collect::<PyResult<Vec<_>>>());
            result.unwrap_or_else(|err| {
                err.display(function.py());
                vec![]
            })
        }) as Box<dyn Fn(&Path) -> Vec<Color32> + 'py>
    });
    Ok(Hooks { badges, extras })
}

fn parse_headers(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<[String; 4]> {
    Ok(match kwarg::<Vec<String>>(kwargs, "headers")? {
        Some(h) if h.len() == 4 => [h[0].clone(), h[1].clone(), h[2].clone(), h[3].clone()],
        Some(_) => return Err(PyValueError::new_err("headers needs 4 titles: name, modified, size and kind")),
        None => default_headers(),
    })
}

fn parse_sort_by(name: &str) -> PyResult<SortBy> {
    SortBy::parse(name).ok_or_else(|| {
        PyValueError::new_err("sort_by must be \"name\", \"modified\", \"size\" or \"kind\"")
    })
}

/// The methods the list and the icon views share
macro_rules! folder_view_methods {
    ($class:ident $(, $($extra:tt)*)?) => {
        #[pymethods]
        impl $class {
            $($($extra)*)?

            #[new]
            #[pyo3(signature = (id, folder=PathBuf::from("."), watcher=None, show_hidden=false, multi_select=true, navigate=true, dirs_first=true, sort_by="name"))]
            #[allow(clippy::too_many_arguments)]
            fn new(
                id: String,
                folder: PathBuf,
                watcher: Option<PyRef<'_, FileWatcher>>,
                show_hidden: bool,
                multi_select: bool,
                navigate: bool,
                dirs_first: bool,
                sort_by: &str,
            ) -> PyResult<Self> {
                Ok($class {
                    view: FolderView::new(id, folder, core_of(watcher), show_hidden, multi_select, navigate, dirs_first, sort_by),
                })
            }

            /// The folder shown. Setting it enters another folder, like go
            #[getter]
            fn get_folder(&self) -> String {
                to_str(self.view.folder())
            }

            #[setter]
            fn set_folder(&mut self, folder: PathBuf) {
                self.view.go(&folder);
            }

            /// Enters a folder, kept in the history of back and forward
            fn go(&mut self, folder: PathBuf) {
                self.view.go(&folder);
            }

            /// Enters the parent folder and selects the folder we come from. Cmd+Up does the same
            fn up(&mut self) {
                self.view.up();
            }

            /// Goes back to the previous folder of the history
            fn back(&mut self) {
                self.view.back();
            }

            /// Goes forward in the history, after back
            fn forward(&mut self) {
                self.view.forward();
            }

            /// True if back has a folder to go to
            #[getter]
            fn can_go_back(&self) -> bool {
                self.view.can_go_back()
            }

            /// True if forward has a folder to go to
            #[getter]
            fn can_go_forward(&self) -> bool {
                self.view.can_go_forward()
            }

            /// True during the frame the view entered another folder by itself: double click, Enter
            /// or Cmd+Up on a folder
            #[getter]
            fn folder_changed(&self) -> bool {
                self.view.folder_changed
            }

            /// Paths of the selected items, in the order they were selected
            #[getter]
            fn get_selected(&self) -> Vec<String> {
                self.view.selected.iter().map(|p| to_str(p)).collect()
            }

            #[setter]
            fn set_selected(&mut self, selected: Vec<PathBuf>) {
                self.view.selected = selected.iter().map(|p| expand(p)).collect();
                self.view.cursor = self.view.selected.last().cloned();
                self.view.anchor = self.view.cursor.clone();
            }

            /// Selects one item and scrolls to it
            fn reveal(&mut self, path: PathBuf) {
                let path = expand(&path);
                self.view.select_only(path.clone());
                self.view.scroll_to = Some(path);
            }

            /// Paths activated during the last frame: double click or Enter on files (on folders too
            /// when navigate is False)
            #[getter]
            fn activated(&self) -> Vec<String> {
                self.view.activated.iter().map(|p| to_str(p)).collect()
            }

            /// The folder of the view under the pointer while files are dragged over it from the Finder
            /// (``hovered_files``) or on the frame they are dropped on it (``dropped_files``), else
            /// None. The folder is drawn framed while files hover it
            #[getter]
            fn drop_target(&self) -> Option<String> {
                self.view.drop_target.as_ref().map(|p| to_str(p))
            }

            /// Paths of the items shown at the last frame, in their order
            #[getter]
            fn items(&self) -> Vec<String> {
                self.view.shown.iter().map(|p| to_str(p)).collect()
            }

            /// Only the items whose name contains this text are shown (without case). Empty shows all.
            /// It is the filter of the column "name"
            #[getter]
            fn get_filter(&self) -> String {
                self.view.columns.name_text().to_string()
            }

            #[setter]
            fn set_filter(&mut self, filter: String) {
                self.view.columns.set_name_text(filter);
            }

            /// Shows the hidden items: a name starting with a dot, or hidden by the system
            #[getter]
            fn get_show_hidden(&self) -> bool {
                self.view.show_hidden
            }

            #[setter]
            fn set_show_hidden(&mut self, show: bool) {
                self.view.show_hidden = show;
            }

            /// The order of the items: "name", "modified", "size", "kind", or the title of a column of your
            /// own that has a value function. A click on a title of the header changes it
            #[getter]
            fn get_sort_by(&self) -> String {
                self.view.columns.sort_key.clone()
            }

            #[setter]
            fn set_sort_by(&mut self, sort_by: String) {
                self.view.columns.sort_key = sort_by;
            }

            /// False reverses the order
            #[getter]
            fn get_ascending(&self) -> bool {
                self.view.columns.ascending
            }

            #[setter]
            fn set_ascending(&mut self, ascending: bool) {
                self.view.columns.ascending = ascending;
            }

            /// The widths the user gave to the columns by dragging their border, by column key ("name",
            /// "modified", "size", "kind" or the title of a column of your own). Save and give them back at
            /// the next run. A column without width has its default one
            #[getter]
            fn get_column_widths<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
                widths_py(&self.view.columns, py)
            }

            #[setter]
            fn set_column_widths(&mut self, widths: HashMap<String, f32>) {
                self.view.columns.set_widths(widths);
            }

            /// The key of the column whose icon was clicked since the last call (the title of a column of your own,
            /// see the "icon" of extra_columns), or None. The call forgets it
            fn icon_clicked(&mut self) -> Option<String> {
                self.view.columns.icon_clicked.take()
            }

            /// The columns pinned at the left (by column key): they stay in place while the others scroll
            /// horizontally. The user pins a column with a right click on its title
            #[getter]
            fn get_pinned_left(&self) -> Vec<String> {
                self.view.columns.pinned_left.clone()
            }

            #[setter]
            fn set_pinned_left(&mut self, columns: Vec<String>) {
                self.view.columns.pinned_left = columns;
            }

            /// The columns pinned at the right (by column key)
            #[getter]
            fn get_pinned_right(&self) -> Vec<String> {
                self.view.columns.pinned_right.clone()
            }

            #[setter]
            fn set_pinned_right(&mut self, columns: Vec<String>) {
                self.view.columns.pinned_right = columns;
            }

            /// The keys of the columns in the order the user gave them by dragging their title. Empty while
            /// the order is the default one. Save and give it back at the next run
            #[getter]
            fn get_column_order(&self) -> Vec<String> {
                self.view.columns.order.clone()
            }

            #[setter]
            fn set_column_order(&mut self, order: Vec<String>) {
                self.view.columns.order = order;
            }

            /// The filters of the columns, by column key: a text for a column of text (the items whose value
            /// contains it, without case), a (min, max) tuple for a number or a date (None for no bound; a
            /// size is "10 KB", a date "2024-05-01"), a list of values for a column of choices. The funnel of
            /// the header sets them too. Assign a dict to replace them all
            #[getter]
            fn get_column_filters<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
                filters_py(&self.view.columns, py)
            }

            #[setter]
            fn set_column_filters(&mut self, filters: &Bound<'_, PyDict>) -> PyResult<()> {
                set_filters(&mut self.view.columns, filters)
            }

            /// Removes the filters of all the columns, the name included
            fn clear_filters(&mut self) {
                self.view.columns.filters.clear();
            }

            /// Asks the functions `value` of the columns of your own again at the next frame, e.g. after the
            /// tags changed. They are otherwise kept for a second
            fn refresh_columns(&mut self) {
                self.view.values.map.clear();
            }

            /// Gives the keyboard focus to the view (True, the default) or takes it away (False) at
            /// the next frame. A click on the view gives it the focus too
            #[pyo3(signature = (focused=true))]
            fn focus(&mut self, focused: bool) {
                self.view.focus = Some(focused);
            }

            /// If True, the view takes back the keyboard focus at every frame when another widget
            /// holds it, except a text field being edited. For an app where the view is the part of
            /// the window the keyboard acts on
            #[getter]
            fn get_keep_focus(&self) -> bool {
                self.view.keep_focus
            }

            #[setter]
            fn set_keep_focus(&mut self, keep: bool) {
                self.view.keep_focus = keep;
            }

            /// Scrolls just enough to show an item, at the next frame
            fn scroll_to(&mut self, path: PathBuf) {
                self.view.scroll_to = Some(expand(&path));
            }

            /// The items shown instead of the items of the folder, e.g. the result of a search or
            /// the files of a tag, from any folders. They stay up to date like a folder. None (the
            /// default) shows the folder; entering a folder sets it back to None
            #[getter]
            fn get_paths(&self) -> Option<Vec<String>> {
                self.view.paths.as_ref().map(|paths| paths.iter().map(|p| to_str(p)).collect())
            }

            #[setter]
            fn set_paths(&mut self, paths: Option<Vec<PathBuf>>) {
                let paths: Option<Vec<PathBuf>> = paths.map(|paths| paths.iter().map(|p| expand(p)).collect());
                if paths != self.view.paths {
                    self.view.paths = paths;
                }
            }
        }
    };
}

/// The state of a list of the files of a folder, with the columns Name, Date Modified, Size and
/// Kind. Create it once, outside the update function, and draw it with file_list_view.
///
/// The folder is watched while it is shown (see FileWatcher): what any program changes in it
/// shows at the next frame. A click selects, Cmd (Ctrl) and Shift extend the selection, a drag on
/// the background draws a rubber band, a double click or Enter opens. The arrows, Home, End, Page
/// Up and Page Down move, Cmd+A selects all, Cmd+Up enters the parent folder, and typing the start
/// of a name selects it.
///
/// The header of the columns is interactive: a click on a title sorts by the column (a second click
/// reverses the order), a drag on a title moves the column, a drag on the border of a title
/// resizes it (a double click gives it its default width back), and the funnel at the right of a
/// title filters it, with a filter that fits the type of the column: a text for the name, a minimum
/// and a maximum for the size, a period for the date, the values to keep for the kind. Everything
/// is also a property of the view (sort_by, ascending, column_widths, column_order,
/// column_filters), to save at the end of the run and give back at the next one.
///
/// Args:
///     id (str): unique id of the view
///
///     folder (str): the folder shown, ~ is the home folder. Default is the current folder
///
///     watcher (FileWatcher): the watcher of the folders. Default is one shared by the views created
///         without one
///
///     show_hidden (bool): shows the hidden items. Default is False
///
///     multi_select (bool): several items can be selected. Default is True
///
///     navigate (bool): a double click or Enter on a folder enters it. Default is True. With False
///         the folder is in activated, like a file
///
///     dirs_first (bool): folders before files. Default is True
///
///     sort_by (str): "name", "modified", "size", "kind" or the title of a column of your own that
///         has a value function. Default is "name"
///
/// Usage::
///

///     files = FileListView("files", "~/Documents")
///
///     def update_func(ctx):
///         if button_clicked("⬆"):
///             files.up()
///         label(files.folder)
///         file_list_view(files)
///         for path in files.activated:
///             ctx.open_url(Path(path).as_uri())
#[pyclass]
pub(crate) struct FileListView {
    view: FolderView,
}

folder_view_methods!(FileListView);

/// The state of a grid of the files of a folder, with large icons and the name under them. Create
/// it once, outside the update function, and draw it with file_icon_view.
///
/// It works like FileListView: the same arguments, selection, rubber band, keyboard (the four
/// arrows move in the grid) and the folder is watched while it is shown.
///
/// Args:
///     id (str): unique id of the view
///
///     folder (str): the folder shown, ~ is the home folder. Default is the current folder
///
///     watcher (FileWatcher): the watcher of the folders. Default is one shared by the views created
///         without one
///
///     show_hidden (bool): shows the hidden items. Default is False
///
///     multi_select (bool): several items can be selected. Default is True
///
///     navigate (bool): a double click or Enter on a folder enters it. Default is True
///
///     dirs_first (bool): folders before files. Default is True
///
///     sort_by (str): "name", "modified", "size", "kind" or the title of a column of your own that
///         has a value function. Default is "name"
///
/// The size of the grid is icon_size, which can change at any time, e.g. from a slider.
///
/// Usage::
///
///     icons = FileIconView("icons", "~/Pictures")
///     size = Float(64)
///
///     def update_func(ctx):
///         slider_float(size, 16, 256, "")
///         icons.icon_size = size.value  # the grid follows the slider
///         file_icon_view(icons)
///         if icons.selected:
///             label(f"{len(icons.selected)} selected")
#[pyclass]
pub(crate) struct FileIconView {
    view: FolderView,
}


folder_view_methods!(FileIconView,
    /// The size of the icons, between 16 and 256: the tiles of the grid grow with it, so there
    /// are fewer of them on a row. Default is 48. Give it a slider to let the user choose, like the
    /// Finder. The icon_size keyword arg of file_icon_view wins over it
    #[getter]
    fn get_icon_size(&self) -> f32 {
        self.view.icon_size
    }

    #[setter]
    fn set_icon_size(&mut self, size: f32) {
        self.view.icon_size = size.clamp(ICON_SIZES.0, ICON_SIZES.1);
    }
);

fn parse_columns(kwargs: Option<&Bound<'_, PyDict>>, extras: usize) -> PyResult<Vec<(pyegui_core::files::Column, f32)>> {
    let names = kwarg::<Vec<String>>(kwargs, "columns")?
        .unwrap_or_else(|| vec!["name".into(), "modified".into(), "size".into(), "kind".into()]);
    for name in &names {
        parse_sort_by(name)?;
    }
    Ok(list_columns(&names, extras))
}

/// Draws a FileListView: the files of its folder as a list with columns. It takes the space left
/// in the ui and scrolls; only the visible rows are drawn, so large folders stay fast.
///
/// Args:
///     view (FileListView): the state of the view
///
/// Keyword args:
///     columns (list[str]): the columns after the name, among "modified", "size" and "kind".
///         Default is all three. A view narrower than its columns scrolls horizontally
///
///     headers (list[str]): the titles of the columns name, modified, size and kind. Default is
///         ["Name", "Date Modified", "Size", "Kind"]
///
///     date_format (str): format of the dates, like strftime, or "human" for the dates in French words
///         ("il y a 5 min", "Hier à 09:10", "12 mars"). Default is "human"
///
///     row_height (float): height of a row. Default is 24
///
///     extra_columns (list[tuple]): columns of your own, after the name: a title, a width and a
///         function called with the path of the row inside the cell, to draw it with the usual
///         widgets. They sort and filter when a fourth item, a dict, gives the type of their data
///         and a function that returns the value of a row: {"type": "text" | "number" | "date" |
///         "choice", "value": function(path) -> value}. A date is seconds since 1970 or a
///         datetime. The dict can also hold "icon": the path of an SVG file drawn at the left of the
///         title, in the color of the title. The values are kept for a second (see refresh_columns).
///         The function runs while the view is drawn: it must not read or change the view itself
///
///     resizable (bool): the border of a column can be dragged. Default is True
///
///     reorderable (bool): the title of a column can be dragged to another place. Default is True
///
///     drag_out (bool): a drag of the selection starts a drag of the files to the Finder (the Explorer,
///         another application), see start_file_drag. Default is True
///
///     filterable (bool): the funnel of the header opens the filter of the column. Default is True
///
///     badges (callable): called with the path of each row shown, returns a list of colors drawn
///         as dots at the right end of the name, e.g. the tags of a file
///
///     context_menu (callable): called with the list of paths concerned on a right click, inside a
///         menu: the selection if the item clicked is in it, else the item clicked, and [] on the
///         background. Build the menu in it
///
/// Example::
///
///     files = FileListView("files", "~")
///
///     def menu(paths):
///         if paths and button_clicked("Show in Finder"):
///             ...
///         if button_clicked("Show hidden files"):
///             files.show_hidden = not files.show_hidden
///
///     def update_func(ctx):
///         file_list_view(files, columns=["modified", "size"], context_menu=menu)
///
/// A column of your own that sorts and filters::
///
///     def stars(path):
///         rating = ratings.get(path, 0)
///         label("★" * rating)
///
///     file_list_view(
///         files,
///         extra_columns=[("Rating", 80, stars, {"type": "number", "value": lambda p: ratings.get(p, 0)})],
///     )
///     files.column_filters = {"size": ("1 MB", None), "kind": ["PDF document"]}
#[pyfunction]
#[pyo3(signature = (view, **kwargs))]
unsafe fn file_list_view(view: &Bound<'_, FileListView>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let headers = parse_headers(kwargs)?;
    let hooks = read_hooks(kwargs)?;
    let shape = Shape::List {
        columns: parse_columns(kwargs, hooks.extras.len())?,
        headers,
        date_format: kwarg::<String>(kwargs, "date_format")?.unwrap_or_else(|| DATE_FORMAT.into()),
        row_height: kwarg::<f32>(kwargs, "row_height")?.unwrap_or(24.0),
    };
    let menu = kwarg::<Bound<'_, PyAny>>(kwargs, "context_menu")?;
    let ui = current_ui(&UI)?;
    let menus = draw_folder(ui, &mut view.borrow_mut().view, shape, &hooks, menu.is_some(), header_options(kwargs, false)?);
    run_menus(menus, menu.as_ref())
}

/// Draws a FileIconView: the files of its folder as a grid of icons. It takes the space left in
/// the ui and scrolls; only the visible tiles are drawn.
///
/// Args:
///     view (FileIconView): the state of the view
///
/// Keyword args:
///     icon_size (float): size of the icons, instead of view.icon_size. Default is view.icon_size
///
///     badges (callable): called with the path of each tile shown, returns a list of colors drawn
///         as dots on the top left corner of the icon, e.g. the tags of a file
///
///     context_menu (callable): called with the list of paths concerned on a right click, inside a
///         menu, like for file_list_view
///
/// Example::
///
///     icons = FileIconView("icons", "~/Desktop")
///
///     def update_func(ctx):
///         file_icon_view(icons)
#[pyfunction]
#[pyo3(signature = (view, **kwargs))]
unsafe fn file_icon_view(view: &Bound<'_, FileIconView>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let size = view.borrow().view.icon_size;
    let hooks = read_hooks(kwargs)?;
    let shape = Shape::Icons {
        icon: kwarg::<f32>(kwargs, "icon_size")?.unwrap_or(size).clamp(ICON_SIZES.0, ICON_SIZES.1),
        tile: Vec2Size { w: 1.0, h: 1.0 },
        per_row: 1,
    };
    let menu = kwarg::<Bound<'_, PyAny>>(kwargs, "context_menu")?;
    let ui = current_ui(&UI)?;
    let menus = draw_folder(ui, &mut view.borrow_mut().view, shape, &hooks, menu.is_some(), header_options(kwargs, false)?);
    run_menus(menus, menu.as_ref())
}

/// The state of a tree of the folders and files under a root folder. Create it once, outside
/// the update function, and draw it with file_tree_view.
///
/// A folder is read when it is opened, and watched while it is open (see FileWatcher): what any
/// program changes in it shows at the next frame. The keyboard works like in TreeView: the
/// arrows move, Left and Right close and open a folder, Enter or a double click on a file
/// activates it.
///
/// Args:
///     id (str): unique id of the view
///
///     root (str): the folder at the top, ~ is the home folder. Default is the current folder
///
///     watcher (FileWatcher): the watcher of the folders. Default is one shared by the views created
///         without one
///
///     show_hidden (bool): shows the hidden items. Default is False
///
///     multi_select (bool): several items can be selected with Cmd (Ctrl) and Shift. Default is True
///
///     show_root (bool): the root folder is the first node and the rest is in it. Default is False:
///         the items of the root are at the top
///
///     dirs_first (bool): folders before files. Default is True
///
///     activatable_dirs (bool): a double click or Enter on a folder activates it, like a file (see
///         activated). Its arrow still opens and closes it. Default is False: a double click opens
///         and closes the folder
///
/// Usage::
///
///     tree = FileTreeView("tree", "~/projects/app")
///
///     def update_func(ctx):
///         file_tree_view(tree)
///         for path in tree.activated:
///             print("open", path)
#[pyclass]
pub(crate) struct FileTreeView {
    view: Shared<TreeState>,
}

impl FileTreeView {
    fn v(&self) -> MutexGuard<'_, TreeState> {
        self.view.borrow()
    }
}

#[pymethods]
impl FileTreeView {

    #[new]
    #[pyo3(signature = (id, root=PathBuf::from("."), watcher=None, show_hidden=false, multi_select=true, show_root=false, dirs_first=true, activatable_dirs=false))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        id: String,
        root: PathBuf,
        watcher: Option<PyRef<'_, FileWatcher>>,
        show_hidden: bool,
        multi_select: bool,
        show_root: bool,
        dirs_first: bool,
        activatable_dirs: bool,
    ) -> Self {
        FileTreeView {
            view: Shared::new(TreeState::new(
                id,
                root,
                core_of(watcher),
                show_hidden,
                multi_select,
                show_root,
                dirs_first,
                activatable_dirs,
            )),
        }
    }

    /// The folder at the top of the tree
    #[getter]
    fn get_root(&self) -> String {
        to_str(&self.v().root)
    }

    #[setter]
    fn set_root(&mut self, root: PathBuf) {
        self.v().root = expand(&root);
    }

    /// Paths of the selected items
    #[getter]
    fn get_selected(&self) -> Vec<String> {
        self.v().selected.clone()
    }

    /// Paths of the rows shown at the last frame, in their order: the items of the open folders
    /// follow their folder, and sort and filters are applied
    #[getter]
    fn items(&self) -> Vec<String> {
        self.v().shown.clone()
    }

    #[setter]
    fn set_selected(&mut self, selected: Vec<PathBuf>) {
        self.v().select_paths(&selected);
    }

    /// Paths activated during the last frame by a double click or Enter: files, and folders when
    /// activatable_dirs is True
    #[getter]
    fn activated(&self) -> Vec<String> {
        self.v().activated.clone()
    }

    /// The folder of the tree under the pointer while files are dragged over it from the Finder or
    /// from the window itself, or on the frame they are dropped on it, else None. The folder is
    /// drawn framed while files hover it
    #[getter]
    fn drop_target(&self) -> Option<String> {
        self.v().drop_target.clone()
    }

    /// Opens or closes a folder of the tree
    fn set_open(&mut self, path: PathBuf, open: bool) {
        self.v().set_open(&path, open);
    }

    /// True if the folder is open in the tree
    fn is_open(&self, path: PathBuf) -> bool {
        self.v().is_open(&path)
    }

    /// The paths of the open folders, to save and give back at the next run
    #[getter]
    fn get_open_folders(&self) -> Vec<String> {
        let mut open: Vec<String> = self.v().open.iter().cloned().collect();
        open.sort();
        open
    }

    #[setter]
    fn set_open_folders(&mut self, folders: Vec<PathBuf>) {
        let mut v = self.v();
        let current: Vec<String> = v.open.iter().cloned().collect();
        for path in current {
            v.set_open(Path::new(&path), false);
        }
        for path in folders {
            v.set_open(&path, true);
        }
    }

    /// Opens the folders down to a path, selects it and scrolls to it
    fn reveal(&mut self, path: PathBuf) {
        self.v().reveal(&path);
    }

    /// Only the files whose name contains this text are shown (without case), the folders always.
    /// It is the filter of the column "name"
    #[getter]
    fn get_filter(&self) -> String {
        self.v().columns.name_text().to_string()
    }

    #[setter]
    fn set_filter(&mut self, filter: String) {
        self.v().columns.set_name_text(filter);
    }

    /// Shows the hidden items: a name starting with a dot, or hidden by the system
    #[getter]
    fn get_show_hidden(&self) -> bool {
        self.v().show_hidden
    }

    #[setter]
    fn set_show_hidden(&mut self, show: bool) {
        self.v().show_hidden = show;
    }

    /// The order of the items: "name", "modified", "size", "kind", or the title of a column of your
    /// own that has a value function. A click on a title of the header changes it
    #[getter]
    fn get_sort_by(&self) -> String {
        self.v().columns.sort_key.clone()
    }

    #[setter]
    fn set_sort_by(&mut self, sort_by: String) {
        self.v().columns.sort_key = sort_by;
    }

    /// False reverses the order
    #[getter]
    fn get_ascending(&self) -> bool {
        self.v().columns.ascending
    }

    #[setter]
    fn set_ascending(&mut self, ascending: bool) {
        self.v().columns.ascending = ascending;
    }

    /// The widths the user gave to the columns by dragging their border, by column key ("name",
    /// "modified", "size", "kind" or the title of a column of your own). Save and give them back at
    /// the next run. A column without width has its default one
    #[getter]
    fn get_column_widths<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        widths_py(&self.v().columns, py)
    }

    #[setter]
    fn set_column_widths(&mut self, widths: HashMap<String, f32>) {
        self.v().columns.set_widths(widths);
    }

    /// The key of the column whose icon was clicked since the last call (the title of a column of your own, see
    /// the "icon" of extra_columns), or None. The call forgets it
    fn icon_clicked(&mut self) -> Option<String> {
        self.v().columns.icon_clicked.take()
    }

    /// The columns pinned at the left (by column key): they stay in place while the others scroll
    /// horizontally. The user pins a column with a right click on its title
    #[getter]
    fn get_pinned_left(&self) -> Vec<String> {
        self.v().columns.pinned_left.clone()
    }

    #[setter]
    fn set_pinned_left(&mut self, columns: Vec<String>) {
        self.v().columns.pinned_left = columns;
    }

    /// The columns pinned at the right (by column key)
    #[getter]
    fn get_pinned_right(&self) -> Vec<String> {
        self.v().columns.pinned_right.clone()
    }

    #[setter]
    fn set_pinned_right(&mut self, columns: Vec<String>) {
        self.v().columns.pinned_right = columns;
    }

    /// The keys of the columns in the order the user gave them by dragging their title. Empty while
    /// the order is the default one. Save and give it back at the next run
    #[getter]
    fn get_column_order(&self) -> Vec<String> {
        self.v().columns.order.clone()
    }

    #[setter]
    fn set_column_order(&mut self, order: Vec<String>) {
        self.v().columns.order = order;
    }

    /// The filters of the columns, by column key: a text for a column of text (the items whose value
    /// contains it, without case), a (min, max) tuple for a number or a date (None for no bound; a
    /// size is "10 KB", a date "2024-05-01"), a list of values for a column of choices. The funnel of
    /// the header sets them too. Assign a dict to replace them all
    #[getter]
    fn get_column_filters<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        filters_py(&self.v().columns, py)
    }

    #[setter]
    fn set_column_filters(&mut self, filters: &Bound<'_, PyDict>) -> PyResult<()> {
        set_filters(&mut self.v().columns, filters)
    }

    /// Removes the filters of all the columns, the name included
    fn clear_filters(&mut self) {
        self.v().columns.filters.clear();
    }

    /// Asks the functions `value` of the columns of your own again at the next frame, e.g. after the
    /// tags changed. They are otherwise kept for a second
    fn refresh_columns(&mut self) {
        self.v().values.map.clear();
    }

    /// Gives the keyboard focus to the tree (True, the default) or takes it away (False) at the
    /// next frame
    #[pyo3(signature = (focused=true))]
    fn focus(&mut self, focused: bool) {
        self.v().focus = Some(focused);
    }

    /// If True, the tree takes back the keyboard focus at every frame when another widget holds
    /// it, except a text field being edited
    #[getter]
    fn get_keep_focus(&self) -> bool {
        self.v().keep_focus
    }

    #[setter]
    fn set_keep_focus(&mut self, keep: bool) {
        self.v().keep_focus = keep;
    }

    /// Scrolls just enough to show an item, at the next frame. It has to be shown: its folders open
    fn scroll_to(&mut self, path: PathBuf) {
        self.v().scroll_to = Some(to_str(&expand(&path)));
    }

    /// The items at the top of the tree instead of the items of the root, e.g. the result of a
    /// search or the files of a tag, from any folders; their folders still open. None (the
    /// default) shows the root
    #[getter]
    fn get_paths(&self) -> Option<Vec<String>> {
        self.v().paths.as_ref().map(|paths| paths.iter().map(|p| to_str(p)).collect())
    }

    #[setter]
    fn set_paths(&mut self, paths: Option<Vec<PathBuf>>) {
        self.v().paths = paths.map(|paths| paths.iter().map(|p| expand(p)).collect());
    }
}

/// Draws a FileTreeView: the folders and files under its root, as a tree. The folders are read
/// when they are opened and watched while they are open.
///
/// Args:
///     view (FileTreeView): the state of the view
///
/// Keyword args:
///     context_menu (callable): called with the list of paths concerned when an item is
///         right-clicked, inside a menu: the selection if the item clicked is in it, else the item
///         clicked. Build the menu in it
///
///     badges (callable): called with the path of each item, returns a list of colors drawn as
///         dots at the right end of its row, e.g. the tags of a file
///
///     columns (list[str]): columns of the view at the right end of the rows, among "modified",
///         "size" and "kind". Default is none, the tree is then only names
///
///     headers (list[str]): the titles of the columns name, modified, size and kind. Default is
///         ["Name", "Date Modified", "Size", "Kind"]
///
///     date_format (str): format of the dates, like strftime, or "human" for the dates in French words
///         ("il y a 5 min", "Hier à 09:10", "12 mars"). Default is "human"
///
///     extra_columns (list[tuple]): columns of your own at the right end of the rows: a title
///         (shown above the tree), a width, and a function called with the path of each row to draw
///         its cell, e.g. ("Tags", 160, lambda path: tags_edit(...)). A fourth item, a dict, makes
///         the column sort and filter: {"type": "text" | "number" | "date" | "choice", "value":
///         function(path) -> value, "icon": path of an SVG file}. A date is seconds since 1970 or a
///         datetime. The icon is drawn at the left of the title, in its color
///
///     header (bool): draws the header of the columns above the tree, pinned at the top while the
///         tree scrolls. Default is True when there are columns
///
///     resizable (bool): the border of a column can be dragged. Default is True
///
///     reorderable (bool): the title of a column can be dragged to another place. Default is True
///
///     drag_out (bool): a drag of the selection starts a drag of the files to the Finder (the Explorer,
///         another application), see start_file_drag. Default is True
///
///     pinnable (bool): a right click on a title offers to pin the column at the left (after the
///         name) or at the right: the other columns then scroll horizontally. Default is True
///
///     filterable (bool): the funnel of the header opens the filter of the column, which fits its
///         type: a text, a minimum and a maximum, a period, or the values to keep. The folders
///         stay, only the files are filtered. Default is True
///
/// A click on a title sorts the items of each folder by the column, a second click reverses the
/// order. See sort_by, ascending, column_widths, column_order and column_filters of the view to
/// read, save and set them.
///
/// Example::
///
///     tree = FileTreeView("tree", "~/projects")
///
///     def update_func(ctx):
///         left_panel("side", lambda: scroll_area_both(lambda: file_tree_view(tree)))
///         label(f"selected: {tree.selected}")
#[pyfunction]
#[pyo3(signature = (view, **kwargs))]
unsafe fn file_tree_view(view: &Bound<'_, FileTreeView>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let menu = kwarg::<Bound<'_, PyAny>>(kwargs, "context_menu")?;
    let hooks = read_hooks(kwargs)?;
    let mut builtin = vec![];
    for name in kwarg::<Vec<String>>(kwargs, "columns")?.unwrap_or_default() {
        let sort = parse_sort_by(&name)?;
        if sort != SortBy::Name && !builtin.contains(&sort) {
            builtin.push(sort);
        }
    }
    let params = TreeParams {
        opts: header_options(kwargs, false)?,
        headers: parse_headers(kwargs)?,
        date_format: kwarg::<String>(kwargs, "date_format")?.unwrap_or_else(|| DATE_FORMAT.into()),
        builtin,
        header: kwarg::<bool>(kwargs, "header")?.unwrap_or(true),
    };
    let ui = current_ui(&UI)?;
    // the view is free while the tree is drawn: the context menu can read and change it
    let result = std::cell::RefCell::new(Ok(()));
    let run = |menu_ui: &mut egui::Ui, ids: &Vec<String>| {
        if let Some(function) = &menu {
            if let Err(err) = run_nested_call(menu_ui, function, (ids.clone(),)) {
                *result.borrow_mut() = Err(err);
            }
        }
    };
    let shared = view.borrow().view.clone();
    draw_tree(ui, &shared, &hooks, &params, menu.is_some().then_some(&run as &dyn Fn(&mut egui::Ui, &Vec<String>)));
    result.into_inner()
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<FileListView>()?;
    m.add_class::<FileIconView>()?;
    m.add_class::<FileTreeView>()?;
    m.add_function(wrap_pyfunction!(file_list_view, m)?)?;
    m.add_function(wrap_pyfunction!(file_icon_view, m)?)?;
    m.add_function(wrap_pyfunction!(file_tree_view, m)?)?;
    Ok(())
}
