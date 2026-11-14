//! Layout rules: a few values (gap, rounding, outline, opacity, inner margin) given once for the
//! whole window and overridden by region when needed. The regions are the sidebar, the top bar, the
//! bottom bar, the central area and the dock.
//!
//! A value is looked up from the most specific to the most general: the arguments of the call, then
//! the rules of the region, then the global rules, then the defaults of the framework.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{LazyLock, Mutex};

use eframe::egui::{self, Color32, Frame, Margin, Stroke};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};

use crate::style::{kwarg_color, kwarg_stroke};
use crate::{kwarg, panel_parent_ui, run_nested_update_func, with_bool_copy, Bool, UI};

pub(crate) const REGIONS: [&str; 5] = ["sidebar", "central", "dock", "topbar", "bottombar"];

/// The values a rule can set. A value that is None is not set at this level.
#[derive(Clone, Default)]
pub(crate) struct Tokens {
    gap: Option<f32>,
    rounding: Option<f32>,
    outline: Option<Stroke>,
    opacity: Option<f32>,
    inner_margin: Option<f32>,
    sheet: Option<Color32>,
}

const TOKEN_NAMES: [&str; 6] = ["gap", "rounding", "outline", "opacity", "inner_margin", "sheet"];
/// the key of the global rules that is not a token: what is seen between the boxes
const BACKGROUND: &str = "background";

impl Tokens {
    fn from_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Tokens> {
        Ok(Tokens {
            gap: kwarg::<f32>(kwargs, "gap")?,
            rounding: kwarg::<f32>(kwargs, "rounding")?,
            outline: kwarg_stroke(kwargs, "outline")?,
            opacity: kwarg::<f32>(kwargs, "opacity")?,
            inner_margin: kwarg::<f32>(kwargs, "inner_margin")?,
            sheet: kwarg_color(kwargs, "sheet")?,
        })
    }

    /// What is set here, completed by what is set in `other`
    fn or(&self, other: &Tokens) -> Tokens {
        Tokens {
            gap: self.gap.or(other.gap),
            rounding: self.rounding.or(other.rounding),
            outline: self.outline.or(other.outline),
            opacity: self.opacity.or(other.opacity),
            inner_margin: self.inner_margin.or(other.inner_margin),
            sheet: self.sheet.or(other.sheet),
        }
    }
}

#[derive(Default)]
struct Rules {
    /// the rules were given, or a region was drawn: the framework lays the window out
    active: bool,
    global: Tokens,
    background: Option<Color32>,
    regions: HashMap<String, Tokens>,
}

static RULES: LazyLock<Mutex<Rules>> = LazyLock::new(|| Mutex::new(Rules::default()));

/// The values of a region, all set
#[derive(Clone, Copy)]
pub(crate) struct Resolved {
    pub gap: f32,
    pub rounding: f32,
    pub outline: Stroke,
    pub opacity: f32,
    pub inner_margin: f32,
    sheet: Option<Color32>,
}

impl Resolved {
    /// The fill of the boxes of the region: the sheet color (the panel color by default) with the opacity
    pub(crate) fn sheet_fill(&self, visuals: &egui::Visuals) -> Color32 {
        self.sheet.unwrap_or(visuals.panel_fill).gamma_multiply(self.opacity)
    }
}

/// What is seen between the boxes: the background of the rules (the panel color by default)
pub(crate) fn background(visuals: &egui::Visuals) -> Color32 {
    let rules = RULES.lock().unwrap_or_else(|e| e.into_inner());
    rules.background.unwrap_or(visuals.panel_fill)
}

const DEFAULT_GAP: f32 = 8.0;
const DEFAULT_ROUNDING: f32 = 12.0;

fn default_outline() -> Stroke {
    Stroke::new(1.0, Color32::from_rgba_unmultiplied(127, 127, 127, 60))
}

fn default_inner_margin(region: &str) -> f32 {
    match region {
        "sidebar" => 12.0,
        _ => 8.0,
    }
}

/// The values of a region: the call, then the region, then the global rules, then the defaults
pub(crate) fn resolve(region: &str, call: &Tokens) -> Resolved {
    let rules = RULES.lock().unwrap_or_else(|e| e.into_inner());
    let mut tokens = call.clone();
    if let Some(of_region) = rules.regions.get(region) {
        tokens = tokens.or(of_region);
    }
    tokens = tokens.or(&rules.global);
    Resolved {
        gap: tokens.gap.unwrap_or(DEFAULT_GAP),
        rounding: tokens.rounding.unwrap_or(DEFAULT_ROUNDING),
        outline: tokens.outline.unwrap_or_else(default_outline),
        opacity: tokens.opacity.unwrap_or(1.0).clamp(0.0, 1.0),
        inner_margin: tokens.inner_margin.unwrap_or_else(|| default_inner_margin(region)),
        sheet: tokens.sheet,
    }
}

/// The rules of the dock when the framework lays the window out, to be used as its defaults
pub(crate) fn dock_rules() -> Option<Resolved> {
    let active = RULES.lock().unwrap_or_else(|e| e.into_inner()).active;
    active.then(|| resolve("dock", &Tokens::default()))
}

/// Sets the rules: the keys of the dict are the global values, and the name of a region holds the
/// values of this region. Replaces the previous rules.
pub(crate) fn set_rules(dict: &Bound<'_, PyDict>) -> PyResult<()> {
    for key in dict.keys() {
        let name: String = key.extract()?;
        if !TOKEN_NAMES.contains(&name.as_str()) && !REGIONS.contains(&name.as_str()) && name != BACKGROUND {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "unknown layout key '{}': the values are {:?}, '{}' and the regions are {:?}",
                name, TOKEN_NAMES, BACKGROUND, REGIONS
            )));
        }
    }
    let global = Tokens::from_kwargs(Some(dict))?;
    let mut regions = HashMap::new();
    for region in REGIONS {
        if let Some(values) = dict.get_item(region)? {
            let values = values.cast::<PyDict>()?;
            regions.insert(region.to_string(), Tokens::from_kwargs(Some(values))?);
        }
    }
    let mut rules = RULES.lock().unwrap_or_else(|e| e.into_inner());
    let background = kwarg_color(Some(dict), BACKGROUND)?;
    *rules = Rules { active: true, global, background, regions };
    Ok(())
}

// where the panels are, to know which sides of a box face another box

const LEFT: u8 = 1;
const RIGHT: u8 = 2;
const TOP: u8 = 4;
const BOTTOM: u8 = 8;

static PLACED: AtomicU8 = AtomicU8::new(0);
static FULLSIZE: AtomicBool = AtomicBool::new(false);

/// Called before the update function of every frame
pub(crate) fn begin_frame() {
    PLACED.store(0, Ordering::Relaxed);
}

/// macOS: the content goes under the title bar, with the window buttons over the top left corner
pub(crate) fn set_fullsize_content(fullsize: bool) {
    FULLSIZE.store(fullsize, Ordering::Relaxed);
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    fn flag(self) -> u8 {
        match self {
            Side::Left => LEFT,
            Side::Right => RIGHT,
            Side::Top => TOP,
            Side::Bottom => BOTTOM,
        }
    }
}

/// The space around a box. A side that faces the window gets the gap, a side that faces another box
/// gets half of it, the other box gives the other half. The side of a panel that faces the central
/// area is always shared, since the central area comes last.
fn margins(placed: u8, side: Option<Side>, gap: f32) -> Margin {
    let whole = gap.round().clamp(0.0, 127.0) as i8;
    let half = (gap / 2.0).round().clamp(0.0, 127.0) as i8;
    let pick = |flag: u8, facing_center: bool| {
        if facing_center || placed & flag != 0 {
            half
        } else {
            whole
        }
    };
    Margin {
        left: pick(LEFT, side == Some(Side::Right)),
        right: pick(RIGHT, side == Some(Side::Left)),
        top: pick(TOP, side == Some(Side::Bottom)),
        bottom: pick(BOTTOM, side == Some(Side::Top)),
    }
}

/// The frame of the central panel: the margins follow the panels placed around it
pub(crate) fn central_frame(style: &egui::Style) -> Option<Frame> {
    if !RULES.lock().unwrap_or_else(|e| e.into_inner()).active {
        return None;
    }
    let values = resolve("central", &Tokens::default());
    let fill = background(&style.visuals);
    Some(
        Frame::new()
            .inner_margin(margins(PLACED.load(Ordering::Relaxed), None, values.gap))
            .fill(fill),
    )
}

// the panels of the regions

unsafe fn show_region_panel(
    mut panel: egui::Panel,
    side: Side,
    region: &str,
    update_fun: Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    if let Some(resizable) = kwarg::<bool>(kwargs, "resizable")? {
        panel = panel.resizable(resizable);
    }
    panel = panel.show_separator_line(false);
    if let Some(size) = kwarg::<f32>(kwargs, "default_size")? {
        panel = panel.default_size(size);
    }
    if let Some(size) = kwarg::<f32>(kwargs, "min_size")? {
        panel = panel.min_size(size);
    }
    if let Some(size) = kwarg::<f32>(kwargs, "max_size")? {
        panel = panel.max_size(size);
    }
    if let Some(size) = kwarg::<f32>(kwargs, "exact_size")? {
        panel = panel.exact_size(size);
    }

    if let Ok(mut rules) = RULES.lock() {
        rules.active = true;
    }
    let values = resolve(region, &Tokens::from_kwargs(kwargs)?);
    let placed = PLACED.load(Ordering::Relaxed);
    PLACED.store(placed | side.flag(), Ordering::Relaxed);

    let ui = panel_parent_ui(&UI)?;
    let fill = values.sheet_fill(&ui.style().visuals);
    // the margin is the inner margin of the panel, so that it keeps the color of the window
    // the gap around the box shows the background of the rules
    let backdrop = background(&ui.style().visuals);
    let panel_frame = Frame::new().inner_margin(margins(placed, Some(side), values.gap)).fill(backdrop);
    panel = panel.frame(panel_frame);

    let py = update_fun.py();
    let title_bar = region == "sidebar" && cfg!(target_os = "macos") && FULLSIZE.load(Ordering::Relaxed);
    let content = move |ui: &mut egui::Ui| -> PyResult<()> {
        let boxed = Frame::new()
            .inner_margin(values.inner_margin.round().clamp(0.0, 127.0) as i8)
            .corner_radius(values.rounding)
            .fill(fill)
            .stroke(values.outline);
        // the box fills the panel; its size is taken before the frame is opened, since the frame
        // adds its margins around what is asked inside
        let outer = ui.available_size();
        let margin = boxed.total_margin().sum();
        let inner = (outer - margin).max(egui::Vec2::ZERO);
        boxed
            .show(ui, |ui| {
                match side {
                    Side::Left | Side::Right => ui.set_min_size(inner),
                    Side::Top | Side::Bottom => ui.set_min_width(inner.x),
                }
                if title_bar {
                    ui.add_space(26.0); // the window buttons are over the sidebar
                }
                run_nested_update_func(ui, update_fun)
            })
            .inner
    };

    match kwarg::<Py<Bool>>(kwargs, "expanded")? {
        Some(expanded) => with_bool_copy(py, &expanded, |is_expanded| {
            panel
                .show_collapsible(ui, is_expanded, |ui| content(ui))
                .map_or(Ok(()), |r| r.inner)
        }),
        None => panel.show(ui, |ui| content(ui)).inner,
    }
}

fn panel_id(kwargs: Option<&Bound<'_, PyDict>>, default: &str) -> PyResult<String> {
    Ok(kwarg::<String>(kwargs, "id")?.unwrap_or_else(|| default.to_string()))
}

/// The sidebar: a box at the left of the window, as tall as the window, that can be resized. On macOS
/// with fullsize_content the room of the window buttons is kept at its top.
///
/// Keyword args:
///
///     id (str): id of the panel. Default is "sidebar"
///
///     default_size (float), min_size (float), max_size (float), exact_size (float), resizable (bool):
///         the width, see left_panel
///
///     expanded (Bool): the panel is shown when expanded.value is True
///
///     gap, rounding, outline, sheet, opacity, inner_margin: the layout rules of this region, see
///         Context.set_layout. They are looked up in the call, then in the rules of the sidebar,
///         then in the global rules
///
/// Example::
///
///     def update_func(ctx):
///         sidebar_panel(lambda: label("Files"), default_size=200)
///         topbar_panel(lambda: label("Title"))
///         heading("Content")
#[pyfunction]
#[pyo3(signature = (update_fun, **kwargs))]
unsafe fn sidebar_panel(update_fun: Bound<'_, PyAny>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let id = panel_id(kwargs, "sidebar")?;
    show_region_panel(egui::Panel::left(id), Side::Left, "sidebar", update_fun, kwargs)
}

/// The top bar: a box at the top of the window, after the sidebar if there is one. Accepts the same
/// keyword args as sidebar_panel, the size being the height. Default id is "topbar".
///
/// Example::
///
///     def update_func(ctx):
///         topbar_panel(lambda: heading("Title"))
#[pyfunction]
#[pyo3(signature = (update_fun, **kwargs))]
unsafe fn topbar_panel(update_fun: Bound<'_, PyAny>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let id = panel_id(kwargs, "topbar")?;
    show_region_panel(egui::Panel::top(id), Side::Top, "topbar", update_fun, kwargs)
}

/// The bottom bar: a box at the bottom of the window. Accepts the same keyword args as
/// sidebar_panel, the size being the height. Default id is "bottombar".
///
/// Example::
///
///     def update_func(ctx):
///         bottombar_panel(lambda: small("Ready"))
#[pyfunction]
#[pyo3(signature = (update_fun, **kwargs))]
unsafe fn bottombar_panel(update_fun: Bound<'_, PyAny>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let id = panel_id(kwargs, "bottombar")?;
    show_region_panel(egui::Panel::bottom(id), Side::Bottom, "bottombar", update_fun, kwargs)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(sidebar_panel, m)?)?;
    m.add_function(wrap_pyfunction!(topbar_panel, m)?)?;
    m.add_function(wrap_pyfunction!(bottombar_panel, m)?)?;
    Ok(())
}
