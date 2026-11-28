//! Style sheets: the style tokens of both themes in a JSON file, and the one implementation that applies a token.
//!
//! A sheet is a JSON object `{"name", "description", "opacity", "light": {path: value}, "dark": {path: value},
//! "app": {"light": {...}, "dark": {...}}}` (see `examples/style_sheets`). The paths are the ones of
//! `Context.set_style_tokens` in Python, which calls [`apply_tokens`] too: `colors.*`, `widgets.<state>.<field>`,
//! `window.*`, `fonts.*`, `tabs.*`, `pill_tabs.*`, `grid.*`, `popups.*`, `spacing.*`,
//! `<context>.<state>.<field>`, `<context>.button_padding[_radius]`, `layout.sheet`, `layout.<region>.sheet`,
//! `layout.outline`, `layout.<region>.outline`, `layout.rounding`, `layout.<region>.rounding`,
//! `layout.[<region>.]resize_stroke`, `layout.[<region>.]resize_hover_stroke`, `layout.[<region>.]resize_active_stroke`
//! and `section.*`.
//!
//! A color is `"#rrggbb[aa]"` or `[r, g, b(, a)]`, a stroke `[width, color]`, a shadow
//! `[offset_x, offset_y, blur, spread, color]`, a rounding or a size a number, a pair `[x, y]`, a font a size
//! or `[size, family]`. The "app" section is what is not a token of the framework (the look of an app); the
//! framework does not read it, [`app_value`] gives its values.
//!
//! From Rust:
//!
//! ```ignore
//! let sheet = pyegui_core::sheet::StyleSheet::load("examples/style_sheets/finder.json")?;
//! pyegui_core::sheet::apply_sheet(ctx, &sheet);
//! let edge = pyegui_core::sheet::app_value(&sheet, ctx.theme() == egui::Theme::Dark, "edge");
//! ```

use std::path::Path;
use std::sync::Mutex;

use egui::{Color32, CornerRadius, Shadow, Stroke, Theme};
use serde_json::{Map, Value};

use crate::{context_style, grid_style, popup_style, section, tabs};

/// A style sheet read from a JSON file
#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    pub name: String,
    pub description: String,
    /// a global opacity the app may use (the framework does not read it)
    pub opacity: Option<f32>,
    /// the tokens of the light theme, path to value
    pub light: Map<String, Value>,
    /// the tokens of the dark theme
    pub dark: Map<String, Value>,
    /// the look of the app that is not a token of the framework, for the light theme
    pub app_light: Map<String, Value>,
    /// the same for the dark theme
    pub app_dark: Map<String, Value>,
}

impl StyleSheet {
    /// Reads a sheet from a JSON file. The name is the "name" of the file, else its stem.
    pub fn load(path: impl AsRef<Path>) -> Result<StyleSheet, String> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut sheet = StyleSheet::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if sheet.name.is_empty() {
            sheet.name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        }
        Ok(sheet)
    }

    /// Reads a sheet from the text of a JSON object
    pub fn parse(text: &str) -> Result<StyleSheet, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let Value::Object(root) = value else {
            return Err("a style sheet is a JSON object".into());
        };
        let section = |v: Option<&Value>| match v {
            Some(Value::Object(m)) => m.clone(),
            _ => Map::new(),
        };
        let app = root.get("app");
        Ok(StyleSheet {
            name: root.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
            description: root.get("description").and_then(Value::as_str).unwrap_or_default().to_string(),
            opacity: root.get("opacity").and_then(Value::as_f64).map(|o| o as f32),
            light: section(root.get("light")),
            dark: section(root.get("dark")),
            app_light: section(app.and_then(|a| a.get("light"))),
            app_dark: section(app.and_then(|a| a.get("dark"))),
        })
    }

    /// The tokens of a theme
    pub fn tokens(&self, theme: Theme) -> &Map<String, Value> {
        match theme {
            Theme::Light => &self.light,
            Theme::Dark => &self.dark,
        }
    }
}

/// The names of the sheets of a folder (its `*.json` files without the extension), sorted. Empty when the
/// folder cannot be read.
pub fn list(dir: impl AsRef<Path>) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json")).map(str::to_string))
        .collect();
    names.sort();
    names
}

/// A value of the "app" section of a sheet for a theme, or None when the sheet does not give it
pub fn app_value(sheet: &StyleSheet, dark: bool, name: &str) -> Option<Value> {
    let section = if dark { &sheet.app_dark } else { &sheet.app_light };
    section.get(name).filter(|v| !v.is_null()).cloned()
}

/// Forgets the tokens kept apart from the egui style: tabs, grids, popups, the layout sheets, the contexts and
/// the paddings. The egui style of the themes is not changed (`Context.reset_style` in Python resets it too).
pub fn reset_tokens() {
    tabs::reset_tokens();
    grid_style::reset_tokens();
    popup_style::reset_tokens();
    reset_layout_sheets();
    reset_layout_outlines();
    reset_layout_roundings();
    reset_layout_resize_strokes();
    context_style::reset_tokens();
    section::reset_tokens();
}

/// Applies a sheet to both themes: the token stores are forgotten first ([`reset_tokens`]), then the tokens of
/// each theme are applied over the egui style of that theme as it is now (give the accent and the glass before).
/// Returns the paths that are not known or whose value is not valid, prefixed by the theme ("dark:colors.foo").
pub fn apply_sheet(ctx: &egui::Context, sheet: &StyleSheet) -> Vec<String> {
    reset_tokens();
    let mut bad = Vec::new();
    for (theme, name) in [(Theme::Light, "light"), (Theme::Dark, "dark")] {
        bad.extend(apply_tokens(ctx, theme, sheet.tokens(theme)).into_iter().map(|p| format!("{name}:{p}")));
    }
    bad
}

/// Applies style tokens to a theme. Only the given paths are changed. Returns the paths that are not known
/// and the ones whose value is not valid (those are skipped, the others are applied).
pub fn apply_tokens(ctx: &egui::Context, theme: Theme, tokens: &Map<String, Value>) -> Vec<String> {
    let mut style: egui::Style = (*ctx.style_of(theme)).clone();
    let mut unknown = Vec::new();
    for (path, value) in tokens {
        match apply_one(&mut style, theme, path, value) {
            Ok(true) => {}
            Ok(false) | Err(_) => unknown.push(path.clone()),
        }
    }
    context_style::fit_padding(&mut style, None);
    ctx.set_style_of(theme, style);
    unknown
}

/// Like [`apply_tokens`], but stops at the first value that is not valid and returns its error (the egui style
/// is then left unchanged, the stores keep what was set before). This is what Python's `set_style_tokens` does.
pub fn try_apply_tokens(ctx: &egui::Context, theme: Theme, tokens: &Map<String, Value>) -> Result<Vec<String>, String> {
    let mut style: egui::Style = (*ctx.style_of(theme)).clone();
    let mut unknown = Vec::new();
    for (path, value) in tokens {
        if !apply_one(&mut style, theme, path, value).map_err(|e| format!("{path}: {e}"))? {
            unknown.push(path.clone());
        }
    }
    context_style::fit_padding(&mut style, None);
    ctx.set_style_of(theme, style);
    Ok(unknown)
}

/// A color: "#rrggbb[aa]", [r, g, b] or [r, g, b, a]
pub fn color(value: &Value) -> Result<Color32, String> {
    match value {
        Value::String(hex) => Color32::from_hex(hex).map_err(|_| format!("Invalid hex color '{hex}'")),
        Value::Array(items) if items.len() == 3 || items.len() == 4 => {
            let mut c = [255u8; 4];
            for (i, item) in items.iter().enumerate() {
                c[i] = byte(item).ok_or("color must be (r, g, b), (r, g, b, a) or '#rrggbb[aa]'")?;
            }
            Ok(if items.len() == 3 { Color32::from_rgb(c[0], c[1], c[2]) } else { Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]) })
        }
        _ => Err("color must be (r, g, b), (r, g, b, a) or '#rrggbb[aa]'".into()),
    }
}

/// A stroke: [width, color]
pub fn stroke(value: &Value) -> Result<Stroke, String> {
    match value.as_array().map(Vec::as_slice) {
        Some([w, c]) => Ok(Stroke::new(number(w)?, color(c)?)),
        _ => Err("a stroke is (width, color)".into()),
    }
}

/// A shadow: [offset_x, offset_y, blur, spread, color]
pub fn shadow(value: &Value) -> Result<Shadow, String> {
    let err = || "a shadow is (offset_x, offset_y, blur, spread, color)".to_string();
    match value.as_array().map(Vec::as_slice) {
        Some([x, y, blur, spread, c]) => {
            let int = |v: &Value| v.as_i64().ok_or_else(err);
            Ok(Shadow {
                offset: [i8::try_from(int(x)?).map_err(|_| err())?, i8::try_from(int(y)?).map_err(|_| err())?],
                blur: u8::try_from(int(blur)?).map_err(|_| err())?,
                spread: u8::try_from(int(spread)?).map_err(|_| err())?,
                color: color(c)?,
            })
        }
        _ => Err(err()),
    }
}

fn byte(value: &Value) -> Option<u8> {
    value.as_u64().and_then(|v| u8::try_from(v).ok())
}

fn number(value: &Value) -> Result<f32, String> {
    value.as_f64().map(|v| v as f32).ok_or_else(|| format!("expected a number, got {value}"))
}

fn pair(value: &Value) -> Result<egui::Vec2, String> {
    match value.as_array().map(Vec::as_slice) {
        Some([x, y]) => Ok(egui::vec2(number(x)?, number(y)?)),
        _ => Err("expected (x, y)".into()),
    }
}

fn radius(value: &Value) -> Result<CornerRadius, String> {
    Ok(CornerRadius::same(number(value)?.round().clamp(0.0, 255.0) as u8))
}

/// Applies one token. Ok(false) when the path is not known; Err when the value is not valid.
fn apply_one(style: &mut egui::Style, theme: Theme, path: &str, value: &Value) -> Result<bool, String> {
    let dark = theme == Theme::Dark;
    let parts: Vec<&str> = path.split('.').collect();
    let v = &mut style.visuals;
    match parts.as_slice() {
        ["colors", name] => match *name {
            "panel" => v.panel_fill = color(value)?,
            "window" => v.window_fill = color(value)?,
            "text_edit_bg" => v.extreme_bg_color = color(value)?,
            "striped_bg" => v.faint_bg_color = color(value)?,
            "code_bg" => v.code_bg_color = color(value)?,
            "text" => v.widgets.noninteractive.fg_stroke.color = color(value)?,
            "hyperlink" => v.hyperlink_color = color(value)?,
            "selection_bg" => v.selection.bg_fill = color(value)?,
            "selection_stroke" => v.selection.stroke.color = color(value)?,
            "warn" => v.warn_fg_color = color(value)?,
            "error" => v.error_fg_color = color(value)?,
            "cursor" => v.text_cursor.stroke.color = color(value)?,
            "separator" => v.widgets.noninteractive.bg_stroke.color = color(value)?,
            _ => return Ok(false),
        },
        ["widgets", state, field] => {
            let w = match *state {
                "noninteractive" => &mut v.widgets.noninteractive,
                "inactive" => &mut v.widgets.inactive,
                "hovered" => &mut v.widgets.hovered,
                "active" => &mut v.widgets.active,
                "open" => &mut v.widgets.open,
                _ => return Ok(false),
            };
            match *field {
                "fill" => w.bg_fill = color(value)?,
                "weak_fill" => w.weak_bg_fill = color(value)?,
                "stroke" => w.bg_stroke = stroke(value)?,
                "fg_stroke" => w.fg_stroke = stroke(value)?,
                "rounding" => w.corner_radius = radius(value)?,
                "expansion" => w.expansion = number(value)?,
                _ => return Ok(false),
            }
        }
        ["window", field] => match *field {
            "stroke" => v.window_stroke = stroke(value)?,
            "rounding" => v.window_corner_radius = radius(value)?,
            "menu_rounding" => v.menu_corner_radius = radius(value)?,
            "shadow" => v.window_shadow = shadow(value)?,
            "popup_shadow" => v.popup_shadow = shadow(value)?,
            _ => return Ok(false),
        },
        ["fonts", name] => {
            let Some((_, font)) = style.text_styles.iter_mut().find(|(k, _)| k.to_string() == *name) else {
                return Ok(false);
            };
            font.size = match value {
                Value::Array(items) if items.len() == 2 && items[1].is_string() => number(&items[0])?,
                _ => number(value).map_err(|_| "a font is (size, family) or a size".to_string())?,
            };
        }
        [group @ ("tabs" | "pill_tabs"), field] => {
            // a value that is not valid unsets the token
            let mut known = true;
            tabs::update_tokens(dark, *group == "pill_tabs", |t| match *field {
                "bar_fill" => t.bar_fill = color(value).ok(),
                "active_fill" => t.active_fill = color(value).ok(),
                "inactive_fill" => t.inactive_fill = color(value).ok(),
                "hovered_fill" => t.hovered_fill = color(value).ok(),
                "active_text" => t.active_text = color(value).ok(),
                "inactive_text" => t.inactive_text = color(value).ok(),
                "active_stroke" => t.active_stroke = stroke(value).ok(),
                "inactive_stroke" => t.inactive_stroke = stroke(value).ok(),
                "spacing" => t.spacing = number(value).ok(),
                "title_spacing" => t.title_spacing = number(value).ok(),
                "bar_height" => t.bar_height = number(value).ok(),
                "outline" => t.outline = value.as_str().and_then(tabs::TabOutline::parse),
                "radius" => t.radius = number(value).ok(),
                "foot" => t.foot = number(value).ok(),
                "top_gap" => t.top_gap = number(value).ok(),
                "icon_size" => t.icon_size = number(value).ok(),
                "close_stroke" => t.close_stroke = stroke(value).ok(),
                "arrow_hovered_fill" => t.arrow_hovered_fill = color(value).ok(),
                "arrow_active_fill" => t.arrow_active_fill = color(value).ok(),
                _ => known = false,
            });
            return Ok(known);
        }
        ["grid", field] => {
            let mut known = true;
            grid_style::update_tokens(dark, |t| match *field {
                "row_line" => t.row_line = stroke(value).ok(),
                "column_line" => t.column_line = stroke(value).ok(),
                "guide_line" => t.guide_line = stroke(value).ok(),
                "guide_radius" => t.guide_radius = number(value).ok(),
                "chevron" => t.chevron = value.as_str().and_then(grid_style::Chevron::parse),
                "chevron_color" => t.chevron_color = color(value).ok(),
                "chevron_hover_color" => t.chevron_hover_color = color(value).ok(),
                "chevron_size" => t.chevron_size = number(value).ok(),
                _ => known = false,
            });
            return Ok(known);
        }
        [context @ ("menu" | "dropdown" | "sidebar" | "topbar" | "bottombar" | "window"), state, field] => {
            let (Some(kind), Some(index)) = (context_style::context_of(context), context_style::state_index(state)) else {
                return Ok(false);
            };
            let mut set = context_style::tokens(dark, kind, index);
            match *field {
                "fill" => set.fill = Some(color(value)?),
                "weak_fill" => set.weak_fill = Some(color(value)?),
                "stroke" => set.stroke = Some(stroke(value)?),
                "fg_stroke" => set.fg_stroke = Some(stroke(value)?),
                "rounding" => set.rounding = Some(radius(value)?),
                "expansion" => set.expansion = Some(number(value)?),
                _ => return Ok(false),
            }
            context_style::update_tokens(dark, kind, index, |t| *t = set);
        }
        [context @ ("menu" | "dropdown" | "sidebar" | "topbar" | "bottombar"), field @ ("button_padding" | "button_padding_radius")] => {
            let Some(kind) = context_style::context_of(context) else { return Ok(true) };
            if *field == "button_padding" {
                let p = pair(value)?;
                context_style::update_padding(dark, Some(kind), |pad| pad.padding = Some((p.x, p.y)));
            } else {
                let factor = number(value)?;
                context_style::update_padding(dark, Some(kind), |pad| pad.radius_factor = Some(factor));
            }
        }
        ["layout", "sheet"] => {
            set_layout_sheet(dark, None, color(value).ok());
        }
        ["layout", region, "sheet"] => return Ok(set_layout_sheet(dark, Some(region), color(value).ok())),
        ["layout", "outline"] => {
            set_layout_outline(dark, None, outline(value)?);
        }
        ["layout", region, "outline"] => return Ok(set_layout_outline(dark, Some(region), outline(value)?)),
        ["layout", kind @ ("resize_stroke" | "resize_hover_stroke" | "resize_active_stroke")] => {
            let state = ResizeState::parse(kind).unwrap_or(ResizeState::Idle);
            set_layout_resize_stroke(dark, None, state, outline(value).map_err(|_| resize_error(value))?);
        }
        ["layout", region, kind @ ("resize_stroke" | "resize_hover_stroke" | "resize_active_stroke")] => {
            let state = ResizeState::parse(kind).unwrap_or(ResizeState::Idle);
            return Ok(set_layout_resize_stroke(dark, Some(region), state, outline(value).map_err(|_| resize_error(value))?));
        }
        ["layout", "rounding"] => {
            set_layout_rounding(dark, None, if value.is_null() { None } else { Some(number(value)?) });
        }
        ["layout", region, "rounding"] => {
            return Ok(set_layout_rounding(dark, Some(region), if value.is_null() { None } else { Some(number(value)?) }))
        }
        ["section", field] => {
            let mut known = true;
            let mut error = None;
            section::update_tokens(dark, |t| match *field {
                "text" => t.text = color(value).ok(),
                "font_size" => t.font_size = number(value).ok(),
                "height" => t.height = number(value).ok(),
                "gap" => t.gap = number(value).ok(),
                "uppercase" => t.uppercase = value.as_bool(),
                "chevron" => {
                    t.chevron = value.as_str().and_then(section::SectionChevron::parse);
                    if t.chevron.is_none() && !value.is_null() {
                        error = Some(format!("section.chevron is \"hover\", \"always\" or \"none\", got {value}"));
                    }
                }
                "chevron_size" => t.chevron_size = number(value).ok(),
                _ => known = false,
            });
            if let Some(error) = error {
                return Err(error);
            }
            return Ok(known);
        }
        ["popups", field] => {
            let c = color(value).ok();
            let mut known = true;
            popup_style::update_tokens(dark, |t| match *field {
                "dropdown_fill" => t.dropdown_fill = c,
                "menu_fill" => t.menu_fill = c,
                "tooltip_fill" => t.tooltip_fill = c,
                "window_fill" => t.window_fill = c,
                _ => known = false,
            });
            return Ok(known);
        }
        ["spacing", name] => match *name {
            "item_spacing" => style.spacing.item_spacing = pair(value)?,
            "button_padding" => style.spacing.button_padding = pair(value)?,
            "button_padding_radius" => {
                let factor = number(value)?;
                context_style::update_padding(dark, None, |p| p.radius_factor = Some(factor));
            }
            "interact_size" => style.spacing.interact_size = pair(value)?,
            "slider_width" => style.spacing.slider_width = number(value)?,
            "icon_width" => style.spacing.icon_width = number(value)?,
            _ => return Ok(false),
        },
        _ => return Ok(false),
    }
    Ok(true)
}

// ---- the sheets of the regions of the layout (`layout.sheet`, `layout.<region>.sheet`)

/// The regions of the layout of a window, in the order of the layout rules
pub const REGIONS: [&str; 5] = ["sidebar", "central", "dock", "topbar", "bottombar"];

/// The sheet colors given by the tokens, per theme (0 light, 1 dark) and per region (the last one is for the
/// whole window)
static LAYOUT_SHEETS: Mutex<[[Option<Color32>; REGIONS.len() + 1]; 2]> = Mutex::new([[None; REGIONS.len() + 1]; 2]);

/// Sets the token `layout.sheet` (region None) or `layout.<region>.sheet`; None forgets it. Returns false if
/// the region is unknown.
pub fn set_layout_sheet(dark: bool, region: Option<&str>, color: Option<Color32>) -> bool {
    let index = match region {
        None => REGIONS.len(),
        Some(name) => match REGIONS.iter().position(|r| *r == name) {
            Some(i) => i,
            None => return false,
        },
    };
    LAYOUT_SHEETS.lock().unwrap_or_else(|e| e.into_inner())[usize::from(dark)][index] = color;
    true
}

/// The sheet the tokens give to a region: `layout.<region>.sheet`, then `layout.sheet`
pub fn layout_sheet(region: &str, dark: bool) -> Option<Color32> {
    let tokens = LAYOUT_SHEETS.lock().unwrap_or_else(|e| e.into_inner())[usize::from(dark)];
    let own = REGIONS.iter().position(|r| *r == region).and_then(|i| tokens[i]);
    own.or(tokens[REGIONS.len()])
}

/// Forgets the tokens `layout.*.sheet`
pub fn reset_layout_sheets() {
    *LAYOUT_SHEETS.lock().unwrap_or_else(|e| e.into_inner()) = [[None; REGIONS.len() + 1]; 2];
}

// ---- the outline of the boxes of the regions (`layout.outline`, `layout.<region>.outline`)

/// The outlines given by the tokens, per theme and per region (the last one is for the whole window)
static LAYOUT_OUTLINES: Mutex<[[Option<Stroke>; REGIONS.len() + 1]; 2]> = Mutex::new([[None; REGIONS.len() + 1]; 2]);

/// An outline token: a stroke (width, color), or `"none"`, `false` or `0` for no line (a stroke of width 0); null
/// forgets the token
fn outline(value: &Value) -> Result<Option<Stroke>, String> {
    match value {
        Value::Null => Ok(None),
        Value::Bool(false) => Ok(Some(Stroke::NONE)),
        Value::String(s) if s == "none" => Ok(Some(Stroke::NONE)),
        Value::Number(n) if n.as_f64() == Some(0.0) => Ok(Some(Stroke::NONE)),
        _ => stroke(value).map(Some).map_err(|_| "an outline is (width, color), \"none\" or false".to_string()),
    }
}

/// Sets the token `layout.outline` (region None) or `layout.<region>.outline`; None forgets it. Returns false
/// if the region is unknown.
pub fn set_layout_outline(dark: bool, region: Option<&str>, stroke: Option<Stroke>) -> bool {
    let index = match region {
        None => REGIONS.len(),
        Some(name) => match REGIONS.iter().position(|r| *r == name) {
            Some(i) => i,
            None => return false,
        },
    };
    LAYOUT_OUTLINES.lock().unwrap_or_else(|e| e.into_inner())[usize::from(dark)][index] = stroke;
    true
}

/// The outline the tokens give to the box of a region: `layout.<region>.outline`, then `layout.outline`. A stroke
/// of width 0 means no line; None means not given (the rules of the layout, then the default).
///
/// Who reads it: the boxes of `sidebar_panel`, `topbar_panel` and `bottombar_panel` (after the call and the rules of
/// the layout), the line between a classic panel and the rest of the window when the call gives no
/// `separator_stroke` (`top_panel` is the region `topbar`, `bottom_panel` the region `bottombar`, `left_panel` and
/// `right_panel` the region `sidebar`), and the bands of finder-rs ([`bar_frame`]). The outline of the bars is
/// thus given by the style sheet, the same in both Finders.
pub fn layout_outline(region: &str, dark: bool) -> Option<Stroke> {
    let tokens = LAYOUT_OUTLINES.lock().unwrap_or_else(|e| e.into_inner())[usize::from(dark)];
    let own = REGIONS.iter().position(|r| *r == region).and_then(|i| tokens[i]);
    own.or(tokens[REGIONS.len()])
}

/// Forgets the tokens `layout.*.outline`
pub fn reset_layout_outlines() {
    *LAYOUT_OUTLINES.lock().unwrap_or_else(|e| e.into_inner()) = [[None; REGIONS.len() + 1]; 2];
}

/// The roundings given by the tokens, per theme and per region (the last one is for the whole window)
static LAYOUT_ROUNDINGS: Mutex<[[Option<f32>; REGIONS.len() + 1]; 2]> = Mutex::new([[None; REGIONS.len() + 1]; 2]);

/// Sets the token `layout.rounding` (region None) or `layout.<region>.rounding`; None forgets it. Returns false
/// if the region is unknown.
pub fn set_layout_rounding(dark: bool, region: Option<&str>, rounding: Option<f32>) -> bool {
    let index = match region {
        None => REGIONS.len(),
        Some(name) => match REGIONS.iter().position(|r| *r == name) {
            Some(i) => i,
            None => return false,
        },
    };
    LAYOUT_ROUNDINGS.lock().unwrap_or_else(|e| e.into_inner())[usize::from(dark)][index] = rounding;
    true
}

/// The rounding the tokens give to the box of a region: `layout.<region>.rounding`, then `layout.rounding`
pub fn layout_rounding(region: &str, dark: bool) -> Option<f32> {
    let tokens = LAYOUT_ROUNDINGS.lock().unwrap_or_else(|e| e.into_inner())[usize::from(dark)];
    let own = REGIONS.iter().position(|r| *r == region).and_then(|i| tokens[i]);
    own.or(tokens[REGIONS.len()])
}

/// Forgets the tokens `layout.*.rounding`
pub fn reset_layout_roundings() {
    *LAYOUT_ROUNDINGS.lock().unwrap_or_else(|e| e.into_inner()) = [[None; REGIONS.len() + 1]; 2];
}

// ---- the resize handle of the panels (`layout.[<region>.]resize_stroke`, `resize_hover_stroke`, `resize_active_stroke`)

/// A state of the resize handle of a panel: what the tokens `resize_stroke`, `resize_hover_stroke` and
/// `resize_active_stroke` give the line of
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeState {
    /// the line when the mouse is not on the edge (`resize_stroke`): the separator line of the panel
    Idle,
    /// the mouse is on the edge (`resize_hover_stroke`)
    Hovered,
    /// the edge is being dragged (`resize_active_stroke`)
    Active,
}

impl ResizeState {
    /// The state of a token name: `resize_stroke`, `resize_hover_stroke` or `resize_active_stroke`
    pub fn parse(name: &str) -> Option<ResizeState> {
        match name {
            "resize_stroke" => Some(ResizeState::Idle),
            "resize_hover_stroke" => Some(ResizeState::Hovered),
            "resize_active_stroke" => Some(ResizeState::Active),
            _ => None,
        }
    }

    /// The name of the token of the state
    pub fn token(self) -> &'static str {
        match self {
            ResizeState::Idle => "resize_stroke",
            ResizeState::Hovered => "resize_hover_stroke",
            ResizeState::Active => "resize_active_stroke",
        }
    }

    fn index(self) -> usize {
        match self {
            ResizeState::Idle => 0,
            ResizeState::Hovered => 1,
            ResizeState::Active => 2,
        }
    }
}

fn resize_error(value: &Value) -> String {
    format!("a resize stroke is (width, color), \"none\" or false, got {value}")
}

/// The strokes of the resize handles given by the tokens, per theme, per region (the last one is for the whole
/// window) and per state
static LAYOUT_RESIZE: Mutex<[[[Option<Stroke>; 3]; REGIONS.len() + 1]; 2]> = Mutex::new([[[None; 3]; REGIONS.len() + 1]; 2]);

/// Sets the token `layout.<state token>` (region None) or `layout.<region>.<state token>`; None forgets it. Returns
/// false if the region is unknown.
pub fn set_layout_resize_stroke(dark: bool, region: Option<&str>, state: ResizeState, stroke: Option<Stroke>) -> bool {
    let index = match region {
        None => REGIONS.len(),
        Some(name) => match REGIONS.iter().position(|r| *r == name) {
            Some(i) => i,
            None => return false,
        },
    };
    LAYOUT_RESIZE.lock().unwrap_or_else(|e| e.into_inner())[usize::from(dark)][index][state.index()] = stroke;
    true
}

/// Forgets the tokens `layout.*.resize_*stroke`
pub fn reset_layout_resize_strokes() {
    *LAYOUT_RESIZE.lock().unwrap_or_else(|e| e.into_inner()) = [[[None; 3]; REGIONS.len() + 1]; 2];
}

/// The strokes the style sheet gives to the resize handle of a panel, by state. A stroke of width 0 means no line
/// (`"none"`); None means not given: egui's own stroke is kept (the separator line `widgets.noninteractive.bg_stroke`,
/// or `layout.<region>.outline` for the panels that read it, then `widgets.hovered.fg_stroke` and
/// `widgets.active.fg_stroke`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResizeStrokes {
    /// the line when the mouse is not on the edge (`resize_stroke`)
    pub idle: Option<Stroke>,
    /// the line when the mouse is on the edge (`resize_hover_stroke`)
    pub hover: Option<Stroke>,
    /// the line while the edge is dragged (`resize_active_stroke`)
    pub active: Option<Stroke>,
}

/// The strokes egui draws the edge of a `Panel` with, read from the visuals of the parent ui: the idle separator line
/// (`widgets.noninteractive.bg_stroke`), the hovered line (`widgets.hovered.fg_stroke`) and the dragged line
/// (`widgets.active.fg_stroke`). Kept by [`ResizeStrokes::apply`] to give the content of the panel its own strokes
/// back ([`PanelStrokes::restore`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelStrokes {
    pub idle: Stroke,
    pub hover: Stroke,
    pub active: Stroke,
}

impl PanelStrokes {
    /// The strokes of the visuals
    pub fn of(visuals: &egui::Visuals) -> PanelStrokes {
        PanelStrokes {
            idle: visuals.widgets.noninteractive.bg_stroke,
            hover: visuals.widgets.hovered.fg_stroke,
            active: visuals.widgets.active.fg_stroke,
        }
    }

    /// Puts the strokes back into the visuals
    pub fn restore(&self, visuals: &mut egui::Visuals) {
        visuals.widgets.noninteractive.bg_stroke = self.idle;
        visuals.widgets.hovered.fg_stroke = self.hover;
        visuals.widgets.active.fg_stroke = self.active;
    }
}

impl ResizeStrokes {
    /// True when no state is given
    pub fn is_empty(&self) -> bool {
        self.idle.is_none() && self.hover.is_none() && self.active.is_none()
    }

    /// Puts the given strokes into the visuals egui draws the edge of a `Panel` with (the parent ui of the panel) and
    /// returns the strokes that were there. The content of the panel is drawn in a child ui that inherits these
    /// visuals: give it the returned strokes back ([`PanelStrokes::restore`]) first thing in the closure of the panel,
    /// so that its widgets do not change, and give them back to the parent ui after the panel too.
    pub fn apply(&self, visuals: &mut egui::Visuals) -> PanelStrokes {
        let saved = PanelStrokes::of(visuals);
        if let Some(s) = self.idle {
            visuals.widgets.noninteractive.bg_stroke = s;
        }
        if let Some(s) = self.hover {
            visuals.widgets.hovered.fg_stroke = s;
        }
        if let Some(s) = self.active {
            visuals.widgets.active.fg_stroke = s;
        }
        saved
    }

    /// The panel with its separator line shown when `resize_stroke` is a visible line, hidden when it is `"none"`;
    /// unchanged when `resize_stroke` is not given.
    pub fn panel(&self, panel: egui::Panel) -> egui::Panel {
        match self.idle {
            Some(s) => panel.show_separator_line(s.width > 0.0),
            None => panel,
        }
    }
}

/// The strokes the style sheet gives to the resize handle of the panels of a region (`sidebar`, `topbar`,
/// `bottombar`...), each state looked up in `layout.<region>.<token>`, then in `layout.<token>`. None when no state is
/// given (the panel is drawn as egui does).
///
/// Who reads it: `left_panel`, `right_panel` and `sidebar_panel` (region `sidebar`), `top_panel` and `topbar_panel`
/// (`topbar`), `bottom_panel` and `bottombar_panel` (`bottombar`) in Python, and [`show_panel`] /
/// [`show_panel_collapsible`] in Rust.
pub fn resize_strokes(region: &str, dark: bool) -> Option<ResizeStrokes> {
    let tokens = LAYOUT_RESIZE.lock().unwrap_or_else(|e| e.into_inner())[usize::from(dark)];
    let own = REGIONS.iter().position(|r| *r == region).map(|i| tokens[i]).unwrap_or([None; 3]);
    let global = tokens[REGIONS.len()];
    let get = |state: ResizeState| own[state.index()].or(global[state.index()]);
    let strokes = ResizeStrokes { idle: get(ResizeState::Idle), hover: get(ResizeState::Hovered), active: get(ResizeState::Active) };
    (!strokes.is_empty()).then_some(strokes)
}

/// Shows a `Panel` with the resize strokes of a region of the style sheet ([`resize_strokes`]): the line of its edge
/// at rest, under the mouse and while dragged, and the separator line shown or hidden by `resize_stroke`. The content
/// keeps the strokes of the theme. Without a token it is `panel.show(ui, add_contents)`.
///
/// ```ignore
/// pyegui_core::sheet::show_panel(ui, egui::Panel::left("sidebar"), "sidebar", |ui| { ui.label("...") });
/// ```
pub fn show_panel<R>(
    ui: &mut egui::Ui,
    panel: egui::Panel,
    region: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let Some(strokes) = resize_strokes(region, ui.visuals().dark_mode) else {
        return panel.show(ui, add_contents);
    };
    let saved = strokes.apply(ui.visuals_mut());
    let response = strokes.panel(panel).show(ui, |ui| {
        saved.restore(ui.visuals_mut());
        add_contents(ui)
    });
    saved.restore(ui.visuals_mut());
    response
}

/// The same as [`show_panel`] for `Panel::show_collapsible`
pub fn show_panel_collapsible<R>(
    ui: &mut egui::Ui,
    panel: egui::Panel,
    region: &str,
    is_expanded: &mut bool,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<R>> {
    let Some(strokes) = resize_strokes(region, ui.visuals().dark_mode) else {
        return panel.show_collapsible(ui, is_expanded, add_contents);
    };
    let saved = strokes.apply(ui.visuals_mut());
    let response = strokes.panel(panel).show_collapsible(ui, is_expanded, |ui| {
        saved.restore(ui.visuals_mut());
        add_contents(ui)
    });
    saved.restore(ui.visuals_mut());
    response
}

/// The box a style sheet gives to a bar (`topbar`, `bottombar`), or None: a bar is a box only when the sheet gives
/// it a sheet (`layout.<region>.sheet`, or `layout.sheet`); its outline is then `layout.<region>.outline` (none by
/// default) and its rounding `layout.<region>.rounding` (0 by default). Without a sheet the bar is the glass of the
/// window, with no box and no outline: the look of finder.py.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarBox {
    pub fill: Color32,
    pub outline: Stroke,
    pub rounding: f32,
}

/// The box of a bar given by the tokens, see [`BarBox`]
pub fn bar_box(region: &str, dark: bool) -> Option<BarBox> {
    let fill = layout_sheet(region, dark)?;
    Some(BarBox { fill, outline: layout_outline(region, dark).unwrap_or(Stroke::NONE), rounding: layout_rounding(region, dark).unwrap_or(0.0) })
}

/// The frame of a band of a Rust app (the top or bottom bar): the box of the sheet ([`bar_box`]), else `glass` with
/// no outline and no rounding; a margin of `margin` at the left and at the right only, so what the band holds is
/// centered in its height. Use it with `Panel::show_separator_line(false)`.
pub fn bar_frame(region: &str, dark: bool, glass: Color32, margin: i8) -> egui::Frame {
    let frame = egui::Frame::NONE.inner_margin(egui::Margin { left: margin, right: margin, top: 0, bottom: 0 });
    match bar_box(region, dark) {
        Some(b) => frame.fill(b.fill).stroke(b.outline).corner_radius(b.rounding.round().clamp(0.0, 255.0) as u8),
        None => frame.fill(glass),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tokens(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn a_bar_is_a_box_only_when_the_sheet_gives_it_a_sheet() {
        let _guard = crate::TOKENS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        reset_tokens();
        // no sheet: the glass, no box even with an outline
        apply_tokens(&ctx, Theme::Dark, &tokens(json!({"layout.topbar.outline": [1.0, "#ff0000ff"], "layout.rounding": 9})));
        assert_eq!(bar_box("topbar", true), None);
        assert_eq!(layout_outline("topbar", true), Some(Stroke::new(1.0, Color32::RED)));
        assert_eq!(layout_rounding("bottombar", true), Some(9.0));
        let glass = Color32::from_white_alpha(20);
        let frame = bar_frame("topbar", true, glass, 8);
        assert_eq!((frame.fill, frame.stroke), (glass, Stroke::NONE));
        // a sheet: a box with the outline and the rounding of the tokens
        let unknown = apply_tokens(&ctx, Theme::Dark, &tokens(json!({"layout.topbar.sheet": "#102030ff", "layout.bottombar.outline": "none", "layout.bottombar.sheet": "#000000ff", "section.chevron": "always", "section.gap": 6})));
        assert!(unknown.is_empty(), "{unknown:?}");
        let b = bar_box("topbar", true).unwrap();
        assert_eq!((b.fill, b.outline, b.rounding), (Color32::from_rgb(0x10, 0x20, 0x30), Stroke::new(1.0, Color32::RED), 9.0));
        assert_eq!(bar_box("bottombar", true).unwrap().outline, Stroke::NONE);
        assert_eq!(bar_box("topbar", false), None);
        assert_eq!(section::tokens(true).gap, Some(6.0));
        reset_tokens();
        assert_eq!(layout_outline("topbar", true), None);
        assert_eq!(section::tokens(true).gap, None);
    }

    #[test]
    fn resize_strokes_by_region_then_global() {
        let _guard = crate::TOKENS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        reset_tokens();
        assert_eq!(resize_strokes("sidebar", true), None);
        let unknown = apply_tokens(
            &ctx,
            Theme::Dark,
            &tokens(json!({
                "layout.resize_hover_stroke": [2.0, "#00ff00ff"],
                "layout.sidebar.resize_stroke": "none",
                "layout.sidebar.resize_active_stroke": [3.0, "#ff0000ff"],
                "layout.nowhere.resize_stroke": [1.0, "#ffffffff"],
            })),
        );
        assert_eq!(unknown, vec!["layout.nowhere.resize_stroke".to_string()]);
        let s = resize_strokes("sidebar", true).unwrap();
        assert_eq!(s.idle, Some(Stroke::NONE));
        assert_eq!(s.hover, Some(Stroke::new(2.0, Color32::GREEN)));
        assert_eq!(s.active, Some(Stroke::new(3.0, Color32::RED)));
        // the global token only for the other regions
        assert_eq!(resize_strokes("topbar", true), Some(ResizeStrokes { hover: Some(Stroke::new(2.0, Color32::GREEN)), ..Default::default() }));
        assert_eq!(resize_strokes("sidebar", false), None);
        // a bad value is reported
        let bad = try_apply_tokens(&ctx, Theme::Dark, &tokens(json!({"layout.sidebar.resize_stroke": "red"})));
        assert!(bad.is_err());

        // apply / restore: the visuals of the parent take the strokes, the content gets the theme back
        let mut visuals = egui::Visuals::dark();
        let theme = PanelStrokes::of(&visuals);
        let saved = s.apply(&mut visuals);
        assert_eq!(saved, theme);
        assert_eq!(visuals.widgets.noninteractive.bg_stroke, Stroke::NONE);
        assert_eq!(visuals.widgets.hovered.fg_stroke, Stroke::new(2.0, Color32::GREEN));
        assert_eq!(visuals.widgets.active.fg_stroke, Stroke::new(3.0, Color32::RED));
        saved.restore(&mut visuals);
        assert_eq!(PanelStrokes::of(&visuals), theme);

        // a panel drawn with the tokens: the content keeps the strokes of the theme
        ctx.set_theme(Theme::Dark);
        let theme = PanelStrokes::of(&ctx.style_of(Theme::Dark).visuals);
        let mut seen = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show_panel(ui, egui::Panel::left("p"), "sidebar", |ui| seen = Some(PanelStrokes::of(ui.visuals())));
            assert_eq!(PanelStrokes::of(ui.visuals()), theme);
        });
        output.textures_delta.clear();
        assert_eq!(seen, Some(theme));

        reset_tokens();
        assert_eq!(resize_strokes("sidebar", true), None);
    }

    // the stores are globals: one test does it all so that the tests do not race on them
    #[test]
    fn apply_tokens_of_each_family() {
        let _guard = crate::TOKENS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        reset_tokens();
        let unknown = apply_tokens(
            &ctx,
            Theme::Dark,
            &tokens(json!({
                "colors.panel": "#102030ff",
                "colors.hyperlink": [1, 2, 3],
                "widgets.hovered.weak_fill": "#3a3a4aff",
                "widgets.inactive.stroke": [2.0, "#ff000080"],
                "widgets.inactive.rounding": 8,
                "window.shadow": [1, 2, 8, 0, [0, 0, 0, 100]],
                "fonts.Body": [15.0, "Proportional"],
                "fonts.Heading": 22,
                "spacing.item_spacing": [4, 5],
                "spacing.button_padding_radius": 0.5,
                "tabs.active_fill": "#112233ff",
                "pill_tabs.spacing": 3,
                "tabs.outline": "joined",
                "tabs.radius": 7,
                "tabs.foot": 5,
                "tabs.top_gap": 6,
                "tabs.icon_size": 16,
                "tabs.close_stroke": [1.5, "#ffffffff"],
                "tabs.arrow_hovered_fill": "#ffffff14",
                "tabs.arrow_active_fill": "#ffffff20",
                "grid.row_line": [1.0, "#3c3c3c3c"],
                "grid.chevron": "triangle",
                "popups.menu_fill": "#1e1e2e80",
                "menu.hovered.weak_fill": "#445566ff",
                "sidebar.button_padding": [6, 3],
                "layout.sheet": "#000000ff",
                "layout.sidebar.sheet": "#ffffff80",
                "colors.nope": "#000000",
                "layout.nowhere.sheet": "#000000",
                "widgets.inactive.bogus": 1,
                "nothing": 1,
                "widgets.active.fill": 12,
            })),
        );
        let mut unknown = unknown;
        unknown.sort();
        assert_eq!(unknown, ["colors.nope", "layout.nowhere.sheet", "nothing", "widgets.active.fill", "widgets.inactive.bogus"]);

        let style = ctx.style_of(Theme::Dark);
        let v = &style.visuals;
        assert_eq!(v.panel_fill, Color32::from_rgb(0x10, 0x20, 0x30));
        assert_eq!(v.hyperlink_color, Color32::from_rgb(1, 2, 3));
        assert_eq!(v.widgets.hovered.weak_bg_fill, Color32::from_hex("#3a3a4aff").unwrap());
        assert_eq!(v.widgets.inactive.bg_stroke, Stroke::new(2.0, Color32::from_hex("#ff000080").unwrap()));
        assert_eq!(v.widgets.inactive.corner_radius, CornerRadius::same(8));
        assert_eq!(v.window_shadow.offset, [1, 2]);
        assert_eq!(v.window_shadow.blur, 8);
        assert_eq!(style.text_styles[&egui::TextStyle::Body].size, 15.0);
        assert_eq!(style.text_styles[&egui::TextStyle::Heading].size, 22.0);
        assert_eq!(style.spacing.item_spacing, egui::vec2(4.0, 5.0));
        // the light theme is not touched
        assert_ne!(ctx.style_of(Theme::Light).visuals.panel_fill, Color32::from_rgb(0x10, 0x20, 0x30));

        assert_eq!(context_style::padding(true, None).radius_factor, Some(0.5));
        assert_eq!(tabs::tokens(true, false).active_fill, Some(Color32::from_hex("#112233ff").unwrap()));
        assert_eq!(tabs::tokens(true, true).spacing, Some(3.0));
        let t = tabs::tokens(true, false);
        assert_eq!(t.outline, Some(tabs::TabOutline::Joined));
        assert_eq!((t.radius, t.foot, t.top_gap, t.icon_size), (Some(7.0), Some(5.0), Some(6.0), Some(16.0)));
        assert_eq!(t.close_stroke, Some(Stroke::new(1.5, Color32::WHITE)));
        assert_eq!(t.arrow_hovered_fill, Some(Color32::from_white_alpha(20)));
        assert_eq!(t.arrow_active_fill, Some(Color32::from_white_alpha(32)));
        assert!(grid_style::tokens(true).row_line.is_some());
        assert!(grid_style::tokens(true).chevron.is_some());
        assert_eq!(popup_style::tokens(true).menu_fill, Some(Color32::from_hex("#1e1e2e80").unwrap()));
        let menu = context_style::context_of("menu").unwrap();
        let hovered = context_style::state_index("hovered").unwrap();
        assert_eq!(context_style::tokens(true, menu, hovered).weak_fill, Some(Color32::from_hex("#445566ff").unwrap()));
        let sidebar = context_style::context_of("sidebar").unwrap();
        assert_eq!(context_style::padding(true, Some(sidebar)).padding, Some((6.0, 3.0)));
        assert_eq!(layout_sheet("sidebar", true), Color32::from_hex("#ffffff80").ok());
        assert_eq!(layout_sheet("dock", true), Some(Color32::BLACK));
        assert_eq!(layout_sheet("dock", false), None);

        // try_apply_tokens stops at a value that is not valid
        assert!(try_apply_tokens(&ctx, Theme::Light, &tokens(json!({"colors.panel": "red"}))).is_err());
        assert_eq!(try_apply_tokens(&ctx, Theme::Light, &tokens(json!({"colors.what": "#000000"}))).unwrap(), ["colors.what"]);

        reset_tokens();
        assert_eq!(layout_sheet("sidebar", true), None);
        assert_eq!(popup_style::tokens(true).menu_fill, None);

        // the sheet finder of the examples: loaded, listed, applied with no unknown path
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/style_sheets");
        assert!(list(&dir).contains(&"finder".to_string()));
        let sheet = StyleSheet::load(dir.join("finder.json")).unwrap();
        assert_eq!(sheet.name, "finder");
        assert!(!sheet.light.is_empty() && !sheet.dark.is_empty());
        assert!(app_value(&sheet, true, "edge").is_some() || app_value(&sheet, false, "edge").is_some() || !sheet.app_dark.is_empty());
        let bad = apply_sheet(&ctx, &sheet);
        assert!(bad.is_empty(), "{bad:?}");
        assert!(popup_style::tokens(true).window_fill.is_some());
        for name in list(&dir) {
            let sheet = StyleSheet::load(dir.join(format!("{name}.json"))).unwrap();
            // only the colors of the snapshot that cannot be set (strong_text, weak_text) may be left over
            let bad = apply_sheet(&ctx, &sheet);
            assert!(bad.iter().all(|p| p.ends_with("_text")), "{name}: {bad:?}");
        }
        reset_tokens();
    }

    #[test]
    fn parse_values() {
        assert_eq!(color(&json!("#ff0000")).unwrap(), Color32::RED);
        assert_eq!(color(&json!([255, 0, 0, 255])).unwrap(), Color32::RED);
        assert!(color(&json!([300, 0, 0])).is_err());
        assert_eq!(stroke(&json!([1.5, "#000000"])).unwrap(), Stroke::new(1.5, Color32::BLACK));
        assert!(shadow(&json!([0, 0, 4])).is_err());
        let sheet = StyleSheet::parse(r##"{"light": {"colors.panel": "#ffffff"}, "app": {"dark": {"edge": [1, "#ffffff10"], "x": null}}}"##).unwrap();
        assert_eq!(sheet.light.len(), 1);
        assert_eq!(app_value(&sheet, true, "edge"), Some(json!([1, "#ffffff10"])));
        assert_eq!(app_value(&sheet, true, "x"), None);
        assert_eq!(app_value(&sheet, false, "edge"), None);
        assert!(StyleSheet::parse("[1]").is_err());
    }
}
