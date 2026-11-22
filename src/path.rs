//! Widgets that show where a path is: path_breadcrumb (the folders from the root, the ancestors
//! shortened when room is missing) and path_combo (the name of the folder, a menu of its ancestors)

use std::path::PathBuf;

use eframe::egui;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use pyegui_core::path::{breadcrumb, combo, BreadcrumbOptions, ComboOptions};
use crate::{current_ui, kwarg, run_nested_call, UI};

/// The functions called on the mouse events of a path widget, read from its keyword args
struct Events<'py> {
    on_click: Option<Bound<'py, PyAny>>,
    on_double_click: Option<Bound<'py, PyAny>>,
    on_right_click: Option<Bound<'py, PyAny>>,
    on_middle_click: Option<Bound<'py, PyAny>>,
    context_menu: Option<Bound<'py, PyAny>>,
    /// what happened during this frame: called once the widget is drawn
    calls: Vec<(Bound<'py, PyAny>, String)>,
    menus: Vec<(egui::Response, String)>,
}

impl<'py> Events<'py> {
    fn read(kwargs: Option<&Bound<'py, PyDict>>) -> PyResult<Self> {
        Ok(Events {
            on_click: kwarg(kwargs, "on_click")?,
            on_double_click: kwarg(kwargs, "on_double_click")?,
            on_right_click: kwarg(kwargs, "on_right_click")?,
            on_middle_click: kwarg(kwargs, "on_middle_click")?,
            context_menu: kwarg(kwargs, "context_menu")?,
            calls: vec![],
            menus: vec![],
        })
    }

    /// Notes what the mouse did to the element of the path
    fn see(&mut self, response: &egui::Response, path: &str, left: bool) {
        let pairs = [
            (left && response.clicked(), &self.on_click),
            (left && response.double_clicked(), &self.on_double_click),
            (response.secondary_clicked(), &self.on_right_click),
            (response.middle_clicked(), &self.on_middle_click),
        ];
        for (happened, function) in pairs {
            if let (true, Some(function)) = (happened, function) {
                self.calls.push((function.clone(), path.to_string()));
            }
        }
        if self.context_menu.is_some() {
            self.menus.push((response.clone(), path.to_string()));
        }
    }

    /// Calls the functions, then shows the context menu of the element right-clicked
    unsafe fn run(self, ui: &mut egui::Ui) -> PyResult<()> {
        for (function, path) in &self.calls {
            run_nested_call(ui, function, (path.clone(),))?;
        }
        let Some(menu) = &self.context_menu else { return Ok(()) };
        let mut result = Ok(());
        for (response, path) in self.menus {
            response.context_menu(|menu_ui| {
                if let Err(err) = run_nested_call(menu_ui, menu, (path.clone(),)) {
                    result = Err(err);
                }
            });
        }
        result
    }
}

/// Shows a path as the list of its folders from the root, like the path bar of the Finder: the
/// icon and the name of each one, separated by ›. When the room is missing the ancestors are
/// shortened, the longest names first, and the mouse over a shortened one shows it in full. A
/// click on a folder returns its path.
///
/// Args:
///     path (str): the path shown, ~ is the home folder
///
/// Keyword args:
///     root (str): the first folder shown, an ancestor of path. Default is the root of the disk
///
///     max_width (float): the widest it can be. Default is the width left in the ui. The
///         ancestors are shortened first, then those that still do not fit are left out from the
///         root, so the breadcrumb never takes more. Give it the width left minus what follows it in
///         the row, as drawn (measured, not guessed, and with the spacing of the row): a row that
///         measures itself and gets an under-estimate pushes its right side out, sees that it does
///         not fit, widens, fits again, and redraws at every frame
///
///     icons (bool): shows the icons of the folders. Default is True
///
///     id (str): id of the breadcrumb, needed if there are several in the same ui
///
///     on_click (callable): called with the path of the element clicked
///
///     on_double_click (callable): called with the path of the element double clicked (after
///         on_click for its first click)
///
///     on_right_click (callable): called with the path of the element right-clicked
///
///     on_middle_click (callable): called with the path of the element clicked with the middle
///         button, e.g. to open it in a new tab
///
///     context_menu (callable): called with the path of the element right-clicked, inside a menu:
///         build the menu in it, like for file_list_view
///
/// Returns the path clicked during this frame, None otherwise: the return value or on_click,
/// whichever reads better.
///
/// Example::
///
///     folder = Str(str(Path.home() / "Documents"))
///
///     def update_func(ctx):
///         clicked = path_breadcrumb(folder.value)
///         if clicked:
///             folder.value = clicked
///
///         # or with functions, and a menu on right click
///         def menu(path):
///             if button_clicked("Copy path"):
///                 ctx.copy_text(path)
///                 close_menu()
///
///         path_breadcrumb(folder.value, on_double_click=open_folder, context_menu=menu)
#[pyfunction]
#[pyo3(signature = (path, **kwargs))]
unsafe fn path_breadcrumb(path: PathBuf, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Option<String>> {
    let mut events = Events::read(kwargs)?;
    let options = BreadcrumbOptions {
        root: kwarg::<PathBuf>(kwargs, "root")?,
        icons: kwarg::<bool>(kwargs, "icons")?.unwrap_or(true),
        max_width: kwarg::<f32>(kwargs, "max_width")?.unwrap_or(f32::INFINITY),
        id: kwarg::<String>(kwargs, "id")?.unwrap_or_default(),
    };
    let ui = current_ui(&UI)?;
    let out = breadcrumb(ui, &path, &options);
    for (path, response) in &out.hits {
        events.see(response, path, true);
    }
    events.run(ui)?;
    Ok(out.clicked)
}

/// Shows the name of a folder as a title that is also a menu of its ancestors, like the title of
/// a Finder window: under the mouse the icon of the folder and the arrow of a menu show, and a
/// click opens the list of the folder and its ancestors up to the root, the folder checked.
/// Choosing one returns its path.
///
/// Args:
///     path (str): the folder shown, ~ is the home folder
///
/// Keyword args:
///     size (float): size of the text. Default is 17
///
///     root (str): the last ancestor of the list. Default is the root of the disk
///
///     id (str): id of the combo, needed if there are several in the same ui
///
///     max_width (float): the widest the title may be: a longer name ends with an ellipsis (the
///         whole name is in the tooltip). Default is the room left in the row
///
///     always_icon (bool): shows the icon of the folder and the arrow of the menu all the time, not
///         only under the mouse. Default is False
///
///     on_right_click (callable): called with the path of the folder when the title is
///         right-clicked
///
///     on_middle_click (callable): called with the path of the folder when the title is clicked
///         with the middle button
///
///     context_menu (callable): called with the path of the folder on a right click on the title,
///         inside a menu: build the menu in it. A left click opens the list of the ancestors
///
/// Returns the path chosen in the list during this frame, None otherwise.
///
/// Example::
///
///     folder = Str(str(Path.home() / "Documents"))
///
///     def update_func(ctx):
///         chosen = path_combo(folder.value)
///         if chosen:
///             folder.value = chosen
#[pyfunction]
#[pyo3(signature = (path, **kwargs))]
unsafe fn path_combo(path: PathBuf, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Option<String>> {
    let mut events = Events::read(kwargs)?;
    let options = ComboOptions {
        size: kwarg::<f32>(kwargs, "size")?.unwrap_or(17.0),
        root: kwarg::<PathBuf>(kwargs, "root")?,
        max_width: kwarg::<f32>(kwargs, "max_width")?,
        id: kwarg::<String>(kwargs, "id")?.unwrap_or_default(),
        always_icon: kwarg::<bool>(kwargs, "always_icon")?.unwrap_or(false),
    };
    let ui = current_ui(&UI)?;
    let out = combo(ui, &path, &options);
    // the left click opens the list: only the other buttons call functions
    for (path, response) in &out.hits {
        events.see(response, path, false);
    }
    events.run(ui)?;
    Ok(out.clicked)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(path_breadcrumb, m)?)?;
    m.add_function(wrap_pyfunction!(path_combo, m)?)?;
    Ok(())
}
