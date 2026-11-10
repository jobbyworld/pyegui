use eframe::egui::{self, Color32, CornerRadius, Frame, Shadow, Stroke, Style, TextStyle};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyDict};

use crate::{current_ui, kwarg, run_nested_update_func, UI};

// conversions

/// Accepts (r, g, b), (r, g, b, a) or "#rrggbb[aa]"
pub(crate) fn extract_color(color: &Bound<'_, PyAny>) -> PyResult<Color32> {
    if let Ok(hex) = color.extract::<String>() {
        return Color32::from_hex(&hex)
            .map_err(|_| PyValueError::new_err(format!("Invalid hex color '{}'", hex)));
    }
    if let Ok((r, g, b, a)) = color.extract::<(u8, u8, u8, u8)>() {
        return Ok(Color32::from_rgba_unmultiplied(r, g, b, a));
    }
    if let Ok((r, g, b)) = color.extract::<(u8, u8, u8)>() {
        return Ok(Color32::from_rgb(r, g, b));
    }
    Err(PyTypeError::new_err(
        "color must be (r, g, b), (r, g, b, a) or '#rrggbb[aa]'",
    ))
}

pub(crate) fn kwarg_color(kwargs: Option<&Bound<'_, PyDict>>, key: &str) -> PyResult<Option<Color32>> {
    kwarg::<Bound<'_, PyAny>>(kwargs, key)?
        .map(|color| extract_color(&color))
        .transpose()
}

/// Accepts (width, color)
pub(crate) fn kwarg_stroke(kwargs: Option<&Bound<'_, PyDict>>, key: &str) -> PyResult<Option<Stroke>> {
    match kwarg::<(f32, Bound<'_, PyAny>)>(kwargs, key)? {
        Some((width, color)) => Ok(Some(Stroke::new(width, extract_color(&color)?))),
        None => Ok(None),
    }
}

/// Accepts False, (blur, color) or (offset_x, offset_y, blur, spread, color)
pub(crate) fn kwarg_shadow(kwargs: Option<&Bound<'_, PyDict>>, key: &str) -> PyResult<Option<Shadow>> {
    let Some(shadow) = kwarg::<Bound<'_, PyAny>>(kwargs, key)? else {
        return Ok(None);
    };

    if let Ok(enabled) = shadow.cast::<PyBool>() {
        if !enabled.is_true() {
            return Ok(Some(Shadow::NONE));
        }
    }
    if let Ok((blur, color)) = shadow.extract::<(u8, Bound<'_, PyAny>)>() {
        return Ok(Some(Shadow {
            offset: [0, 0],
            blur,
            spread: 0,
            color: extract_color(&color)?,
        }));
    }
    if let Ok((x, y, blur, spread, color)) = shadow.extract::<(i8, i8, u8, u8, Bound<'_, PyAny>)>() {
        return Ok(Some(Shadow {
            offset: [x, y],
            blur,
            spread,
            color: extract_color(&color)?,
        }));
    }
    Err(PyTypeError::new_err(
        "shadow must be False, (blur, color) or (offset_x, offset_y, blur, spread, color)",
    ))
}

fn kwarg_vec2(kwargs: Option<&Bound<'_, PyDict>>, key: &str) -> PyResult<Option<egui::Vec2>> {
    Ok(kwarg::<(f32, f32)>(kwargs, key)?.map(|(x, y)| egui::vec2(x, y)))
}

// frames

const FRAME_KWARGS: [&str; 6] = ["fill", "stroke", "rounding", "inner_margin", "outer_margin", "shadow"];

/// True if any keyword arg changes the frame
pub(crate) fn has_frame_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<bool> {
    let Some(kwargs) = kwargs else {
        return Ok(false);
    };
    for key in FRAME_KWARGS {
        if kwargs.contains(key)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Applies fill, stroke, rounding, inner_margin, outer_margin and shadow keyword args
pub(crate) fn apply_frame_kwargs(mut frame: Frame, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Frame> {
    if let Some(fill) = kwarg_color(kwargs, "fill")? {
        frame = frame.fill(fill);
    }
    if let Some(stroke) = kwarg_stroke(kwargs, "stroke")? {
        frame = frame.stroke(stroke);
    }
    if let Some(rounding) = kwarg::<f32>(kwargs, "rounding")? {
        frame = frame.corner_radius(rounding);
    }
    if let Some(margin) = kwarg::<f32>(kwargs, "inner_margin")? {
        frame = frame.inner_margin(margin);
    }
    if let Some(margin) = kwarg::<f32>(kwargs, "outer_margin")? {
        frame = frame.outer_margin(margin);
    }
    if let Some(shadow) = kwarg_shadow(kwargs, "shadow")? {
        frame = frame.shadow(shadow);
    }
    Ok(frame)
}

/// A frame around the contents, with a custom background, border, rounding, margins and shadow.
///
/// Keyword args:
///
///     fill (color): background color. Colors are (r, g, b), (r, g, b, a) or "#rrggbb[aa]"
///
///     stroke (tuple[float, color]): border width and color
///
///     rounding (float): corner radius
///
///     inner_margin (float), outer_margin (float): space inside and outside the border
///
///     shadow (False | tuple[int, color] | tuple[int, int, int, int, color]): (blur, color) or (offset_x, offset_y, blur, spread, color)
///
/// Example::
///
///     def card():
///         heading("Card")
///         label("A frame with a translucent background")
///
///     frame(card, fill=(255, 255, 255, 60), stroke=(1, (255, 255, 255, 120)), rounding=16, inner_margin=12, shadow=(24, (0, 0, 0, 60)))
#[pyfunction]
#[pyo3(signature = (update_fun, **kwargs))]
unsafe fn frame(update_fun: Bound<'_, PyAny>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    apply_frame_kwargs(Frame::new(), kwargs)?
        .show(current_ui(&UI)?, |ui| run_nested_update_func(ui, update_fun))
        .inner
}

// global style

/// Options of Context.set_style, parsed before the style is modified
pub(crate) struct StyleOptions {
    rounding: Option<f32>,
    widget_rounding: Option<f32>,
    window_rounding: Option<f32>,
    panel_fill: Option<Color32>,
    window_fill: Option<Color32>,
    window_stroke: Option<Stroke>,
    window_shadow: Option<Shadow>,
    popup_shadow: Option<Shadow>,
    text_color: Option<Color32>,
    hyperlink_color: Option<Color32>,
    selection_color: Option<Color32>,
    extreme_bg_color: Option<Color32>,
    faint_bg_color: Option<Color32>,
    code_bg_color: Option<Color32>,
    button_fill: Option<Color32>,
    button_hover_fill: Option<Color32>,
    button_active_fill: Option<Color32>,
    button_stroke: Option<Stroke>,
    button_hover_stroke: Option<Stroke>,
    item_spacing: Option<egui::Vec2>,
    button_padding: Option<egui::Vec2>,
    window_margin: Option<f32>,
    body_size: Option<f32>,
    heading_size: Option<f32>,
    monospace_size: Option<f32>,
    animation_time: Option<f32>,
    pub(crate) theme: Option<egui::Theme>,
}

impl StyleOptions {
    pub(crate) fn from_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let theme = match kwarg::<String>(kwargs, "theme")?.as_deref() {
            None => None,
            Some("light") => Some(egui::Theme::Light),
            Some("dark") => Some(egui::Theme::Dark),
            Some(theme) => {
                return Err(PyValueError::new_err(format!(
                    "Unknown theme '{}', expected light or dark",
                    theme
                )))
            }
        };

        Ok(StyleOptions {
            rounding: kwarg(kwargs, "rounding")?,
            widget_rounding: kwarg(kwargs, "widget_rounding")?,
            window_rounding: kwarg(kwargs, "window_rounding")?,
            panel_fill: kwarg_color(kwargs, "panel_fill")?,
            window_fill: kwarg_color(kwargs, "window_fill")?,
            window_stroke: kwarg_stroke(kwargs, "window_stroke")?,
            window_shadow: kwarg_shadow(kwargs, "window_shadow")?,
            popup_shadow: kwarg_shadow(kwargs, "popup_shadow")?,
            text_color: kwarg_color(kwargs, "text_color")?,
            hyperlink_color: kwarg_color(kwargs, "hyperlink_color")?,
            selection_color: kwarg_color(kwargs, "selection_color")?,
            extreme_bg_color: kwarg_color(kwargs, "extreme_bg_color")?,
            faint_bg_color: kwarg_color(kwargs, "faint_bg_color")?,
            code_bg_color: kwarg_color(kwargs, "code_bg_color")?,
            button_fill: kwarg_color(kwargs, "button_fill")?,
            button_hover_fill: kwarg_color(kwargs, "button_hover_fill")?,
            button_active_fill: kwarg_color(kwargs, "button_active_fill")?,
            button_stroke: kwarg_stroke(kwargs, "button_stroke")?,
            button_hover_stroke: kwarg_stroke(kwargs, "button_hover_stroke")?,
            item_spacing: kwarg_vec2(kwargs, "item_spacing")?,
            button_padding: kwarg_vec2(kwargs, "button_padding")?,
            window_margin: kwarg(kwargs, "window_margin")?,
            body_size: kwarg(kwargs, "body_size")?,
            heading_size: kwarg(kwargs, "heading_size")?,
            monospace_size: kwarg(kwargs, "monospace_size")?,
            animation_time: kwarg(kwargs, "animation_time")?,
            theme,
        })
    }

    pub(crate) fn apply(&self, style: &mut Style) {
        let visuals = &mut style.visuals;
        let widgets = &mut visuals.widgets;

        if let Some(rounding) = self.widget_rounding.or(self.rounding) {
            let rounding = CornerRadius::from(rounding);
            widgets.noninteractive.corner_radius = rounding;
            widgets.inactive.corner_radius = rounding;
            widgets.hovered.corner_radius = rounding;
            widgets.active.corner_radius = rounding;
            widgets.open.corner_radius = rounding;
        }
        if let Some(rounding) = self.window_rounding.or(self.rounding) {
            visuals.window_corner_radius = rounding.into();
            visuals.menu_corner_radius = rounding.into();
        }
        if let Some(fill) = self.button_fill {
            widgets.inactive.weak_bg_fill = fill;
            widgets.inactive.bg_fill = fill;
        }
        if let Some(fill) = self.button_hover_fill {
            widgets.hovered.weak_bg_fill = fill;
            widgets.hovered.bg_fill = fill;
        }
        if let Some(fill) = self.button_active_fill {
            widgets.active.weak_bg_fill = fill;
            widgets.active.bg_fill = fill;
            widgets.open.weak_bg_fill = fill;
            widgets.open.bg_fill = fill;
        }
        if let Some(stroke) = self.button_stroke {
            widgets.inactive.bg_stroke = stroke;
        }
        if let Some(stroke) = self.button_hover_stroke {
            widgets.hovered.bg_stroke = stroke;
            widgets.active.bg_stroke = stroke;
        }

        if let Some(fill) = self.panel_fill {
            visuals.panel_fill = fill;
        }
        if let Some(fill) = self.window_fill {
            visuals.window_fill = fill;
        }
        if let Some(stroke) = self.window_stroke {
            visuals.window_stroke = stroke;
        }
        if let Some(shadow) = self.window_shadow {
            visuals.window_shadow = shadow;
        }
        if let Some(shadow) = self.popup_shadow {
            visuals.popup_shadow = shadow;
        }
        if let Some(color) = self.text_color {
            visuals.override_text_color = Some(color);
        }
        if let Some(color) = self.hyperlink_color {
            visuals.hyperlink_color = color;
        }
        if let Some(color) = self.selection_color {
            visuals.selection.bg_fill = color;
        }
        if let Some(color) = self.extreme_bg_color {
            visuals.extreme_bg_color = color;
        }
        if let Some(color) = self.faint_bg_color {
            visuals.faint_bg_color = color;
        }
        if let Some(color) = self.code_bg_color {
            visuals.code_bg_color = color;
        }

        if let Some(spacing) = self.item_spacing {
            style.spacing.item_spacing = spacing;
        }
        if let Some(padding) = self.button_padding {
            style.spacing.button_padding = padding;
        }
        if let Some(margin) = self.window_margin {
            style.spacing.window_margin = margin.into();
        }

        for (text_style, size) in [
            (TextStyle::Body, self.body_size),
            (TextStyle::Button, self.body_size),
            (TextStyle::Heading, self.heading_size),
            (TextStyle::Monospace, self.monospace_size),
        ] {
            if let (Some(size), Some(font)) = (size, style.text_styles.get_mut(&text_style)) {
                font.size = size;
            }
        }

        if let Some(time) = self.animation_time {
            style.animation_time = time;
        }
    }
}

/// Default style of a theme
pub(crate) fn default_style(theme: egui::Theme) -> Style {
    Style {
        visuals: match theme {
            egui::Theme::Light => egui::Visuals::light(),
            egui::Theme::Dark => egui::Visuals::dark(),
        },
        ..Style::default()
    }
}

/// Translucent style looking like frosted glass. Best used with a backdrop effect in run_native.
pub(crate) fn glass_style(style: &mut Style, tint: Option<Color32>) {
    let dark = style.visuals.dark_mode;
    // white glass on light theme, smoked glass on dark theme
    let base = tint.unwrap_or(if dark {
        Color32::from_rgb(20, 22, 30)
    } else {
        Color32::from_rgb(250, 250, 255)
    });
    let with_alpha = |color: Color32, alpha: u8| {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
    };
    let highlight = |alpha: u8| Color32::from_rgba_unmultiplied(255, 255, 255, alpha);

    let visuals = &mut style.visuals;

    visuals.panel_fill = with_alpha(base, if dark { 70 } else { 110 });
    visuals.window_fill = with_alpha(base, if dark { 150 } else { 180 });
    visuals.window_stroke = Stroke::new(1.0, highlight(if dark { 45 } else { 180 }));
    visuals.window_corner_radius = CornerRadius::same(16);
    visuals.menu_corner_radius = CornerRadius::same(12);
    visuals.window_shadow = Shadow {
        offset: [0, 8],
        blur: 32,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 45 }),
    };
    visuals.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 20,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 90 } else { 35 }),
    };
    visuals.extreme_bg_color = if dark {
        Color32::from_black_alpha(70)
    } else {
        highlight(130)
    };
    visuals.text_edit_bg_color = Some(visuals.extreme_bg_color);
    visuals.code_bg_color = if dark { highlight(18) } else { highlight(110) };
    visuals.faint_bg_color = if dark { highlight(8) } else { highlight(50) };

    let widgets = &mut visuals.widgets;
    let (fill, hover, active) = if dark { (22, 40, 60) } else { (90, 150, 210) };

    widgets.noninteractive.bg_fill = with_alpha(base, 0);
    widgets.noninteractive.weak_bg_fill = with_alpha(base, 0);
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, highlight(if dark { 30 } else { 120 }));

    for (widget, alpha, stroke_alpha) in [
        (&mut widgets.inactive, fill, if dark { 35 } else { 170 }),
        (&mut widgets.hovered, hover, if dark { 70 } else { 230 }),
        (&mut widgets.active, active, if dark { 90 } else { 255 }),
        (&mut widgets.open, active, if dark { 90 } else { 255 }),
    ] {
        widget.bg_fill = highlight(alpha);
        widget.weak_bg_fill = highlight(alpha);
        widget.bg_stroke = Stroke::new(1.0, highlight(stroke_alpha));
        widget.corner_radius = CornerRadius::same(10);
        widget.expansion = 0.0;
    }

    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.window_margin = 12.0.into();
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(frame, m)?)?;
    Ok(())
}
