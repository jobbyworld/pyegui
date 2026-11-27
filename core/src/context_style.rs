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

/// Forgets every token
pub fn reset_tokens() {
    *TOKENS.write().unwrap_or_else(|e| e.into_inner()) = Default::default();
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
}

/// The same on the style of a ui
pub fn apply_ui(ui: &mut egui::Ui, context: Context) {
    let mut style = (**ui.style()).clone();
    apply(&mut style, context);
    ui.set_style(style);
}
