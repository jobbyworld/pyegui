use eframe::egui;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};

use crate::style::{kwarg_color, kwarg_stroke};
use crate::{current_ui, kwarg, run_nested_call, UI};
use pyegui_core::tabs::{tab_view as draw_tab_view, TabSet, TabViewOptions};

/// The tabs of a `tab_view`: their titles and the active one. A state object: create it once, outside the update
/// function; `tab_view(tabs, content)` draws it.
///
/// Args:
///     titles (list[str]): the titles of the tabs. Default is none
///     active (int): the index of the active tab. Default is 0
///
/// Example::
///
///     tabs = TabView(["Files", "Editor"])
///
///     def update_func(ctx):
///         tab_view(tabs, lambda i: label(f"content of {tabs.titles[i]}"), closable=True, add_button=True)
///         if tabs.take_added():
///             tabs.add(f"Tab {len(tabs.titles) + 1}")
#[pyclass(unsendable, module = "pyegui")]
pub struct TabView {
    titles: Vec<String>,
    active: usize,
    closed: Option<String>,
    added: bool,
}

#[pymethods]
impl TabView {
    #[new]
    #[pyo3(signature = (titles=None, active=0))]
    fn new(titles: Option<Vec<String>>, active: usize) -> Self {
        let titles = titles.unwrap_or_default();
        let active = active.min(titles.len().saturating_sub(1));
        TabView { titles, active, closed: None, added: false }
    }

    /// The titles of the tabs, in order
    #[getter]
    fn titles(&self) -> Vec<String> {
        self.titles.clone()
    }

    /// The index of the active tab (read and write)
    #[getter]
    fn active(&self) -> usize {
        self.active
    }

    #[setter]
    fn set_active(&mut self, index: usize) -> PyResult<()> {
        if index >= self.titles.len() && !self.titles.is_empty() {
            return Err(PyValueError::new_err("no such tab"));
        }
        self.active = index;
        Ok(())
    }

    /// The title of the active tab, or None when there is no tab
    #[getter]
    fn active_title(&self) -> Option<String> {
        self.titles.get(self.active).cloned()
    }

    /// Adds a tab at the end and makes it the active one
    fn add(&mut self, title: String) {
        self.titles.push(title);
        self.active = self.titles.len() - 1;
    }

    /// Removes the first tab with this title. The active tab stays the same tab, or the one before it if it was removed
    fn remove(&mut self, title: &str) {
        if let Some(i) = self.titles.iter().position(|t| t == title) {
            self.remove_at(i);
        }
    }

    /// Makes the tab with this title the active one
    fn set_active_title(&mut self, title: &str) {
        if let Some(i) = self.titles.iter().position(|t| t == title) {
            self.active = i;
        }
    }

    /// The title of the tab closed with its cross (or the middle button) since the last call, or None. The tab is
    /// already removed
    fn take_closed(&mut self) -> Option<String> {
        self.closed.take()
    }

    /// True once after the "+" button was clicked. Add the tab with `add`
    fn take_added(&mut self) -> bool {
        std::mem::take(&mut self.added)
    }
}

impl TabView {
    fn remove_at(&mut self, i: usize) {
        self.titles.remove(i);
        if i < self.active || (i == self.active && self.active > 0 && self.active >= self.titles.len()) {
            self.active -= 1;
        }
    }
}

fn tab_set_from_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<TabSet> {
    Ok(TabSet {
        bar_fill: kwarg_color(kwargs, "tab_bar_color")?,
        active_fill: kwarg_color(kwargs, "active_tab_fill")?,
        inactive_fill: kwarg_color(kwargs, "inactive_tab_fill")?,
        hovered_fill: kwarg_color(kwargs, "hovered_tab_fill")?,
        active_text: kwarg_color(kwargs, "active_tab_text_color")?,
        inactive_text: kwarg_color(kwargs, "inactive_tab_text_color")?,
        active_stroke: kwarg_stroke(kwargs, "active_tab_stroke")?,
        inactive_stroke: kwarg_stroke(kwargs, "inactive_tab_stroke")?,
        spacing: kwarg(kwargs, "tab_spacing")?,
        title_spacing: kwarg(kwargs, "tab_title_spacing")?,
        bar_height: kwarg(kwargs, "tab_bar_height")?,
    })
}

/// A bar of tabs with, under it, the pane with the content of the active tab.
///
/// The look is the one of the tabs of a `Dock`: both read the same style tokens (`tabs.*` for the classic tabs,
/// `pill_tabs.*` for the pill ones, see `Context.set_style_tokens`), and the options of the call win over them.
///
/// Args:
///     tabs (TabView): the titles and the active tab
///     update_fun (callable): draws the content of the active tab; it is given the index of the tab
///
/// Keyword args:
///     tab_style (str): "classic" tabs joined to their pane, or "pill" tabs rounded on every corner. Default is classic
///     closable (bool): a cross on every tab, and the middle button closes it (`TabView.take_closed`)
///     add_button (bool): a "+" after the last tab (`TabView.take_added`)
///     margin (float): space between the pane and its content. Default is 8
///     tab_bar_height (float), tab_spacing (float), tab_title_spacing (float): sizes of the bar
///     tab_bar_color, active_tab_fill, inactive_tab_fill, hovered_tab_fill (color): backgrounds
///     active_tab_text_color, inactive_tab_text_color (color): colors of the titles
///     active_tab_stroke, inactive_tab_stroke (tuple[float, color]): outlines
///
/// Example::
///
///     tabs = TabView(["A", "B"])
///
///     def update_func(ctx):
///         tab_view(tabs, lambda i: label("content " + tabs.titles[i]), tab_style="pill", closable=True)
#[pyfunction]
#[pyo3(signature = (tabs, update_fun, **kwargs))]
unsafe fn tab_view(py: Python<'_>, tabs: Py<TabView>, update_fun: Bound<'_, PyAny>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let pill = match kwarg::<String>(kwargs, "tab_style")?.as_deref() {
        None | Some("classic") => false,
        Some("pill") => true,
        Some(other) => return Err(PyValueError::new_err(format!("Unknown tab style '{other}', expected classic or pill"))),
    };
    let options = TabViewOptions {
        pill,
        closable: kwarg(kwargs, "closable")?.unwrap_or(false),
        add_button: kwarg(kwargs, "add_button")?.unwrap_or(false),
        given: tab_set_from_kwargs(kwargs)?,
        margin: kwarg(kwargs, "margin")?.unwrap_or(8.0),
    };
    // the state is not borrowed while the content runs: it may use the tabs
    let (titles, active) = {
        let t = tabs.borrow(py);
        (t.titles.clone(), t.active)
    };
    let ui = current_ui(&UI)?;
    let mut error = None;
    let events = draw_tab_view(ui, &titles, active, &options, |ui: &mut egui::Ui| {
        if titles.is_empty() {
            return;
        }
        if let Err(e) = run_nested_call(ui, &update_fun, (active,)) {
            error = Some(e);
        }
    });
    if let Some(e) = error {
        return Err(e);
    }
    let mut t = tabs.borrow_mut(py);
    if let Some(i) = events.selected {
        t.active = i.min(t.titles.len().saturating_sub(1));
    }
    if let Some(i) = events.closed {
        if i < t.titles.len() {
            let title = t.titles[i].clone();
            t.remove_at(i);
            t.closed = Some(title);
        }
    }
    if events.added {
        t.added = true;
    }
    Ok(())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TabView>()?;
    m.add_function(wrap_pyfunction!(tab_view, m)?)?;
    Ok(())
}
