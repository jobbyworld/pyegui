//! The look of the widgets in a context: a button in a menu is not drawn like a button of the page.
//! A token has the path `<context>.<state>.<field>`, for instance `menu.hovered.weak_fill`, with the
//! contexts `menu` (menus and context menus), `dropdown` (the list of a combo box), `sidebar`, `topbar`
//! and `bottombar`, the states `noninteractive`, `inactive`, `hovered`, `active` and `open`, and the fields
//! of `widgets.<state>.*`: `fill`, `weak_fill`, `stroke`, `fg_stroke`, `rounding`, `expansion`.
//!
//! The order of lookup is: the token of the context, then `widgets.<state>.<field>`, then the default.
//! The widgets inside a context inherit its style, so nothing else is to be given at each call.

use std::sync::RwLock;

use egui::style::WidgetVisuals;
use egui::{Color32, CornerRadius, Stroke, Style};

/// The places where the widgets have their own look
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Context {
    Menu,
    Dropdown,
    Sidebar,
    Topbar,
    Bottombar,
}

pub const CONTEXTS: [(&str, Context); 5] = [
    ("menu", Context::Menu),
    ("dropdown", Context::Dropdown),
    ("sidebar", Context::Sidebar),
    ("topbar", Context::Topbar),
    ("bottombar", Context::Bottombar),
];

pub const STATES: [&str; 5] = ["noninteractive", "inactive", "hovered", "active", "open"];

/// What the user gave for one state of one context
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct WidgetSet {
    pub fill: Option<Color32>,
    pub weak_fill: Option<Color32>,
    pub stroke: Option<Stroke>,
    pub fg_stroke: Option<Stroke>,
    pub rounding: Option<CornerRadius>,
    pub expansion: Option<f32>,
}

/// `[theme][context][state]`
type Table = [[[WidgetSet; 5]; 5]; 2];

static TOKENS: RwLock<Table> = RwLock::new([[[WidgetSet {
    fill: None,
    weak_fill: None,
    stroke: None,
    fg_stroke: None,
    rounding: None,
    expansion: None,
}; 5]; 5]; 2]);

fn context_index(context: Context) -> usize {
    CONTEXTS.iter().position(|(_, c)| *c == context).unwrap_or(0)
}

pub fn context_of(name: &str) -> Option<Context> {
    CONTEXTS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

pub fn state_index(name: &str) -> Option<usize> {
    STATES.iter().position(|s| *s == name)
}

/// The tokens given for a state of a context in a theme
pub fn tokens(dark: bool, context: Context, state: usize) -> WidgetSet {
    TOKENS.read().unwrap_or_else(|e| e.into_inner())[usize::from(dark)][context_index(context)][state]
}

/// Changes the tokens of a state of a context
pub fn update_tokens(dark: bool, context: Context, state: usize, change: impl FnOnce(&mut WidgetSet)) {
    change(&mut TOKENS.write().unwrap_or_else(|e| e.into_inner())[usize::from(dark)][context_index(context)][state]);
}

/// The padding of the buttons of a context: `<context>.button_padding` (x, y), and `<context>.button_padding_radius`,
/// a factor of the rounding of the buttons that the padding at the left and at the right is at least
/// (a round button needs room for its corners). The context `page` is the whole app: `spacing.button_padding_radius`
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Padding {
    pub padding: Option<(f32, f32)>,
    pub radius_factor: Option<f32>,
}

/// `[theme][context]`, the page is the last one
static PADDINGS: RwLock<[[Padding; 6]; 2]> = RwLock::new([[Padding { padding: None, radius_factor: None }; 6]; 2]);

/// The padding tokens of a context (None is the page) in a theme
pub fn padding(dark: bool, context: Option<Context>) -> Padding {
    let i = context.map_or(5, context_index);
    PADDINGS.read().unwrap_or_else(|e| e.into_inner())[usize::from(dark)][i]
}

pub fn update_padding(dark: bool, context: Option<Context>, change: impl FnOnce(&mut Padding)) {
    let i = context.map_or(5, context_index);
    change(&mut PADDINGS.write().unwrap_or_else(|e| e.into_inner())[usize::from(dark)][i]);
}

/// The padding of the buttons of a style, from its rounding and the tokens of a context (None is the page)
pub fn fit_padding(style: &mut Style, context: Option<Context>) {
    let dark = style.visuals.dark_mode;
    let here = padding(dark, context);
    let page = padding(dark, None);
    if let Some((x, y)) = here.padding {
        style.spacing.button_padding = egui::vec2(x, y);
    }
    if let Some(factor) = here.radius_factor.or(page.radius_factor) {
        let radius = f32::from(style.visuals.widgets.inactive.corner_radius.nw);
        style.spacing.button_padding.x = style.spacing.button_padding.x.max(factor * radius);
    }
}

/// Forgets every token
pub fn reset_tokens() {
    *TOKENS.write().unwrap_or_else(|e| e.into_inner()) = Default::default();
    *PADDINGS.write().unwrap_or_else(|e| e.into_inner()) = Default::default();
}

fn put(w: &mut WidgetVisuals, set: &WidgetSet) {
    if let Some(c) = set.fill {
        w.bg_fill = c;
    }
    if let Some(c) = set.weak_fill {
        w.weak_bg_fill = c;
    }
    if let Some(s) = set.stroke {
        w.bg_stroke = s;
    }
    if let Some(s) = set.fg_stroke {
        w.fg_stroke = s;
    }
    if let Some(r) = set.rounding {
        w.corner_radius = r;
    }
    if let Some(e) = set.expansion {
        w.expansion = e;
    }
}

/// Puts the tokens of a context in a style: what is drawn in that style takes them
pub fn apply(style: &mut Style, context: Context) {
    let dark = style.visuals.dark_mode;
    let w = &mut style.visuals.widgets;
    for (index, target) in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open]
        .into_iter()
        .enumerate()
    {
        put(target, &tokens(dark, context, index));
    }
    fit_padding(style, Some(context));
}

/// The same on the style of a ui
pub fn apply_ui(ui: &mut egui::Ui, context: Context) {
    let mut style = (**ui.style()).clone();
    apply(&mut style, context);
    ui.set_style(style);
}
