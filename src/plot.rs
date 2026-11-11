use eframe::egui;
use egui_plot::{Bar, BarChart, HLine, Legend, Line, PlotPoints, Points, VLine};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};

use crate::{current_ui, kwarg, UI};

type Color = Option<(u8, u8, u8)>;

fn color32((r, g, b): (u8, u8, u8)) -> egui::Color32 {
    egui::Color32::from_rgb(r, g, b)
}

/// Accepts a list of (x, y) tuples, or a list of y values whose x is their index
fn parse_points(points: &Bound<'_, PyAny>) -> PyResult<Vec<[f64; 2]>> {
    if let Ok(points) = points.extract::<Vec<(f64, f64)>>() {
        return Ok(points.into_iter().map(|(x, y)| [x, y]).collect());
    }

    match points.extract::<Vec<f64>>() {
        Ok(ys) => Ok(ys.into_iter().enumerate().map(|(x, y)| [x as f64, y]).collect()),
        Err(_) => Err(PyTypeError::new_err(
            "points must be a list of (x, y) tuples or a list of numbers",
        )),
    }
}

/// A line of a plot.
///
/// Args:
///     points (list[tuple[float, float]] | list[float]): (x, y) points, or y values whose x is their index
///
///     name (str): name shown in the legend
///
///     color (tuple[int, int, int]): rgb color
///
///     width (float): width of the line
///
///     fill (float): fill the area between the line and this y value
///
/// Usage::
///
///     PlotLine([(x / 10, math.sin(x / 10)) for x in range(100)], name="sin")
#[pyclass]
pub(crate) struct PlotLine {
    points: Vec<[f64; 2]>,
    #[pyo3(get, set)]
    name: String,
    #[pyo3(get, set)]
    color: Color,
    #[pyo3(get, set)]
    width: Option<f32>,
    #[pyo3(get, set)]
    fill: Option<f32>,
}

#[pymethods]
impl PlotLine {
    #[new]
    #[pyo3(signature = (points, name=String::new(), color=None, width=None, fill=None))]
    fn new(
        points: &Bound<'_, PyAny>,
        name: String,
        color: Color,
        width: Option<f32>,
        fill: Option<f32>,
    ) -> PyResult<Self> {
        Ok(PlotLine {
            points: parse_points(points)?,
            name,
            color,
            width,
            fill,
        })
    }

    /// The (x, y) points of the line
    #[getter]
    fn get_points(&self) -> Vec<(f64, f64)> {
        self.points.iter().map(|[x, y]| (*x, *y)).collect()
    }

    #[setter]
    fn set_points(&mut self, points: &Bound<'_, PyAny>) -> PyResult<()> {
        self.points = parse_points(points)?;
        Ok(())
    }
}

/// A scatter plot.
///
/// Args:
///     points (list[tuple[float, float]] | list[float]): (x, y) points, or y values whose x is their index
///
///     name (str): name shown in the legend
///
///     color (tuple[int, int, int]): rgb color
///
///     radius (float): radius of the markers
///
///     filled (bool): fill the markers. Default is True
///
/// Usage::
///
///     PlotPoints([(1, 2), (2, 3.5), (3, 1)], name="measures", radius=4)
#[pyclass(name = "PlotPoints")]
pub(crate) struct PyPlotPoints {
    points: Vec<[f64; 2]>,
    #[pyo3(get, set)]
    name: String,
    #[pyo3(get, set)]
    color: Color,
    #[pyo3(get, set)]
    radius: Option<f32>,
    #[pyo3(get, set)]
    filled: bool,
}

#[pymethods]
impl PyPlotPoints {
    #[new]
    #[pyo3(signature = (points, name=String::new(), color=None, radius=None, filled=true))]
    fn new(
        points: &Bound<'_, PyAny>,
        name: String,
        color: Color,
        radius: Option<f32>,
        filled: bool,
    ) -> PyResult<Self> {
        Ok(PyPlotPoints {
            points: parse_points(points)?,
            name,
            color,
            radius,
            filled,
        })
    }

    /// The (x, y) points
    #[getter]
    fn get_points(&self) -> Vec<(f64, f64)> {
        self.points.iter().map(|[x, y]| (*x, *y)).collect()
    }

    #[setter]
    fn set_points(&mut self, points: &Bound<'_, PyAny>) -> PyResult<()> {
        self.points = parse_points(points)?;
        Ok(())
    }
}

/// A bar chart.
///
/// Args:
///     bars (list[tuple[float, float]] | list[float]): (x, height) bars, or heights whose x is their index
///
///     name (str): name shown in the legend
///
///     color (tuple[int, int, int]): rgb color
///
///     width (float): width of the bars
///
///     horizontal (bool): draw horizontal bars. Default is False
///
/// Usage::
///
///     PlotBars([3, 5, 2, 8], name="sales")
#[pyclass]
pub(crate) struct PlotBars {
    bars: Vec<[f64; 2]>,
    #[pyo3(get, set)]
    name: String,
    #[pyo3(get, set)]
    color: Color,
    #[pyo3(get, set)]
    width: Option<f64>,
    #[pyo3(get, set)]
    horizontal: bool,
}

#[pymethods]
impl PlotBars {
    #[new]
    #[pyo3(signature = (bars, name=String::new(), color=None, width=None, horizontal=false))]
    fn new(
        bars: &Bound<'_, PyAny>,
        name: String,
        color: Color,
        width: Option<f64>,
        horizontal: bool,
    ) -> PyResult<Self> {
        Ok(PlotBars {
            bars: parse_points(bars)?,
            name,
            color,
            width,
            horizontal,
        })
    }

    /// The (x, height) bars
    #[getter]
    fn get_bars(&self) -> Vec<(f64, f64)> {
        self.bars.iter().map(|[x, h]| (*x, *h)).collect()
    }

    #[setter]
    fn set_bars(&mut self, bars: &Bound<'_, PyAny>) -> PyResult<()> {
        self.bars = parse_points(bars)?;
        Ok(())
    }
}

/// A horizontal line at the given y, or a vertical line at the given x if vertical is True.
///
/// Usage::
///
///     PlotRule(0.5, name="threshold", color=(255, 0, 0))
#[pyclass]
pub(crate) struct PlotRule {
    #[pyo3(get, set)]
    value: f64,
    #[pyo3(get, set)]
    name: String,
    #[pyo3(get, set)]
    color: Color,
    #[pyo3(get, set)]
    width: Option<f32>,
    #[pyo3(get, set)]
    vertical: bool,
}

#[pymethods]
impl PlotRule {
    #[new]
    #[pyo3(signature = (value, name=String::new(), color=None, width=None, vertical=false))]
    fn new(value: f64, name: String, color: Color, width: Option<f32>, vertical: bool) -> Self {
        PlotRule {
            value,
            name,
            color,
            width,
            vertical,
        }
    }
}

enum Item {
    Line(Line<'static>),
    Points(Points<'static>),
    Bars(BarChart),
    HLine(HLine),
    VLine(VLine),
}

/// Converts a plot item. default is the color of the items without one, from the theme palette
fn to_item(item: &Bound<'_, PyAny>, default: Option<egui::Color32>) -> PyResult<Item> {
    if let Ok(line) = item.cast::<PlotLine>() {
        let line = line.borrow();
        let mut l = Line::new(line.name.as_str(), PlotPoints::new(line.points.clone()));
        if let Some(color) = line.color.map(color32).or(default) {
            l = l.color(color);
        }
        if let Some(width) = line.width {
            l = l.width(width);
        }
        if let Some(fill) = line.fill {
            l = l.fill(fill);
        }
        return Ok(Item::Line(l));
    }

    if let Ok(points) = item.cast::<PyPlotPoints>() {
        let points = points.borrow();
        let mut p = Points::new(points.name.as_str(), PlotPoints::new(points.points.clone()))
            .filled(points.filled);
        if let Some(color) = points.color.map(color32).or(default) {
            p = p.color(color);
        }
        if let Some(radius) = points.radius {
            p = p.radius(radius);
        }
        return Ok(Item::Points(p));
    }

    if let Ok(bars) = item.cast::<PlotBars>() {
        let bars = bars.borrow();
        let mut b = BarChart::new(
            bars.name.as_str(),
            bars.bars.iter().map(|[x, h]| Bar::new(*x, *h)).collect(),
        );
        if let Some(color) = bars.color.map(color32).or(default) {
            b = b.color(color);
        }
        if let Some(width) = bars.width {
            b = b.width(width);
        }
        if bars.horizontal {
            b = b.horizontal();
        }
        return Ok(Item::Bars(b));
    }

    if let Ok(rule) = item.cast::<PlotRule>() {
        let rule = rule.borrow();
        return Ok(if rule.vertical {
            let mut l = VLine::new(rule.name.as_str(), rule.value);
            if let Some(color) = rule.color {
                l = l.color(color32(color));
            }
            if let Some(width) = rule.width {
                l = l.width(width);
            }
            Item::VLine(l)
        } else {
            let mut l = HLine::new(rule.name.as_str(), rule.value);
            if let Some(color) = rule.color {
                l = l.color(color32(color));
            }
            if let Some(width) = rule.width {
                l = l.width(width);
            }
            Item::HLine(l)
        });
    }

    Err(PyTypeError::new_err(
        "plot items must be PlotLine, PlotPoints, PlotBars or PlotRule",
    ))
}

/// Shows a plot that can be zoomed and dragged.
///
/// Returns the plot coordinates (x, y) of the pointer when it hovers the plot, None otherwise.
///
/// Args:
///     id (str): unique id of the plot
///
///     items (list[PlotLine | PlotPoints | PlotBars | PlotRule]): what to draw
///
/// Keyword args:
///
///     width (float), height (float): size of the plot. Default fills the available space
///
///     view_aspect (float): width / height ratio of the plot
///
///     data_aspect (float): ratio of a unit of x and a unit of y. Use 1 for circles to look round
///
///     legend (bool): show a legend with the item names. Default is True
///
///     x_label (str), y_label (str): axis labels
///
///     allow_zoom (bool), allow_drag (bool), allow_scroll (bool): user interactions. Default is True
///
/// Example::
///
///     sin = PlotLine([(x / 10, math.sin(x / 10)) for x in range(100)], name="sin")
///
///     def update_func(ctx):
///         pos = plot("my plot", [sin], height=200, x_label="x")
///         if pos:
///             label(f"x: {pos[0]:.2f} y: {pos[1]:.2f}")
#[pyfunction]
#[pyo3(signature = (id, items, **kwargs))]
unsafe fn plot(
    id: &str,
    items: Vec<Bound<'_, PyAny>>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<Option<(f64, f64)>> {
    let palette = crate::style::plot_palette();
    let items = items
        .iter()
        .enumerate()
        .map(|(i, item)| to_item(item, palette.as_ref().map(|p| p[i % p.len()])))
        .collect::<PyResult<Vec<_>>>()?;
    let mut p = egui_plot::Plot::new(id);

    let mut legend = Legend::default();
    if palette.is_some() {
        // modern theme: a lighter grid, and the legend where the values are the lowest
        p = p
            .grid_spacing(egui::Rangef::new(28.0, 120.0))
            .show_grid([false, true]);
        legend = legend.position(egui_plot::Corner::LeftTop);
    }
    if kwarg::<bool>(kwargs, "legend")?.unwrap_or(true) {
        p = p.legend(legend);
    }
    if let Some(width) = kwarg::<f32>(kwargs, "width")? {
        p = p.width(width);
    }
    if let Some(height) = kwarg::<f32>(kwargs, "height")? {
        p = p.height(height);
    }
    if let Some(aspect) = kwarg::<f32>(kwargs, "view_aspect")? {
        p = p.view_aspect(aspect);
    }
    if let Some(aspect) = kwarg::<f32>(kwargs, "data_aspect")? {
        p = p.data_aspect(aspect);
    }
    if let Some(label) = kwarg::<String>(kwargs, "x_label")? {
        p = p.x_axis_label(label);
    }
    if let Some(label) = kwarg::<String>(kwargs, "y_label")? {
        p = p.y_axis_label(label);
    }
    if let Some(allow) = kwarg::<bool>(kwargs, "allow_zoom")? {
        p = p.allow_zoom(allow);
    }
    if let Some(allow) = kwarg::<bool>(kwargs, "allow_drag")? {
        p = p.allow_drag(allow);
    }
    if let Some(allow) = kwarg::<bool>(kwargs, "allow_scroll")? {
        p = p.allow_scroll(allow);
    }

    let response = p.show(current_ui(&UI)?, |plot_ui| {
        for item in items {
            match item {
                Item::Line(line) => plot_ui.line(line),
                Item::Points(points) => plot_ui.points(points),
                Item::Bars(bars) => plot_ui.bar_chart(bars),
                Item::HLine(line) => plot_ui.hline(line),
                Item::VLine(line) => plot_ui.vline(line),
            }
        }
        plot_ui.pointer_coordinate()
    });

    Ok(response.inner.map(|point| (point.x, point.y)))
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PlotLine>()?;
    m.add_class::<PyPlotPoints>()?;
    m.add_class::<PlotBars>()?;
    m.add_class::<PlotRule>()?;
    m.add_function(wrap_pyfunction!(plot, m)?)?;
    Ok(())
}
