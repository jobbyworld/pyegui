use eframe::egui::{self, Color32, Frame, Shadow, Stroke};
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

/// A margin: a float for the four sides, or (left, top, right, bottom)
pub(crate) fn extract_margin(value: &Bound<'_, PyAny>, name: &str) -> PyResult<egui::Margin> {
    let side = |v: f32| v.round().clamp(0.0, 127.0) as i8;
    if let Ok(all) = value.extract::<f32>() {
        Ok(egui::Margin::same(side(all)))
    } else if let Ok((left, top, right, bottom)) = value.extract::<(f32, f32, f32, f32)>() {
        Ok(egui::Margin { left: side(left), right: side(right), top: side(top), bottom: side(bottom) })
    } else {
        Err(PyTypeError::new_err(format!("{} must be a float or a tuple (left, top, right, bottom)", name)))
    }
}

/// A margin keyword arg: a float for the four sides, or (left, top, right, bottom)
fn kwarg_margin(kwargs: Option<&Bound<'_, PyDict>>, name: &str) -> PyResult<Option<egui::Margin>> {
    let Some(value) = kwargs.and_then(|k| k.get_item(name).ok().flatten()) else {
        return Ok(None);
    };
    extract_margin(&value, name).map(Some)
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
    if let Some(margin) = kwarg_margin(kwargs, "inner_margin")? {
        frame = frame.inner_margin(margin);
    }
    if let Some(margin) = kwarg_margin(kwargs, "outer_margin")? {
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
///     inner_margin, outer_margin (float or (left, top, right, bottom)): space inside and outside the border
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

pub(crate) use pyegui_core::style::*;

// global style

pub(crate) fn style_options_from_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<StyleOptions> {
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
        menu_margin: kwarg_vec2(kwargs, "menu_margin")?,
        body_size: kwarg(kwargs, "body_size")?,
        heading_size: kwarg(kwargs, "heading_size")?,
        monospace_size: kwarg(kwargs, "monospace_size")?,
        animation_time: kwarg(kwargs, "animation_time")?,
        theme,
    })
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(frame, m)?)?;
    Ok(())
}
