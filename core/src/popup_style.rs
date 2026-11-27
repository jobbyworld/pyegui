//! The fill of the popups, each with its own RGBA color: `popups.dropdown_fill` (the list of a combo box),
//! `popups.menu_fill` (menus and context menus) and `popups.tooltip_fill` (the tooltips). The alpha of the
//! color is the transparency of the popup: `#1e1e2e80` is half transparent.
//!
//! The order of lookup is: the token of the theme, then the window fill of the style.

use std::sync::RwLock;

use egui::style::StyleModifier;
use egui::{Color32, Style};

/// The kinds of popups that have their own fill
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Dropdown,
    Menu,
    Tooltip,
}

/// What the user gave for one theme; nothing set means the usual fill
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct PopupSet {
    pub dropdown_fill: Option<Color32>,
    pub menu_fill: Option<Color32>,
    pub tooltip_fill: Option<Color32>,
}

/// `[theme]`: 0 is light, 1 dark
static TOKENS: RwLock<[PopupSet; 2]> = RwLock::new(
    [PopupSet { dropdown_fill: None, menu_fill: None, tooltip_fill: None }; 2],
);

/// The tokens given for a theme
pub fn tokens(dark: bool) -> PopupSet {
    TOKENS.read().unwrap_or_else(|e| e.into_inner())[usize::from(dark)]
}

/// Changes the tokens of a theme
pub fn update_tokens(dark: bool, change: impl FnOnce(&mut PopupSet)) {
    change(&mut TOKENS.write().unwrap_or_else(|e| e.into_inner())[usize::from(dark)]);
}

/// Forgets every token
pub fn reset_tokens() {
    *TOKENS.write().unwrap_or_else(|e| e.into_inner()) = [PopupSet::default(); 2];
}

/// The fill given for a kind of popup in the theme of `style`, if any
pub fn fill(style: &Style, kind: Kind) -> Option<Color32> {
    let set = tokens(style.visuals.dark_mode);
    match kind {
        Kind::Dropdown => set.dropdown_fill,
        Kind::Menu => set.menu_fill,
        Kind::Tooltip => set.tooltip_fill,
    }
}

/// Puts the fill of the kind in a style (the popups take their fill from the window fill)
pub fn apply(style: &mut Style, kind: Kind) {
    if let Some(color) = fill(style, kind) {
        style.visuals.window_fill = color;
    }
}

/// The style modifier of a kind of popup
pub fn modifier(kind: Kind) -> StyleModifier {
    (move |style: &mut Style| apply(style, kind)).into()
}

/// Shows a tooltip with the fill of the tooltips: the style of the context is changed during the call
pub fn with_tooltip_fill<R>(ctx: &egui::Context, show: impl FnOnce() -> R) -> R {
    let style = ctx.global_style();
    match fill(&style, Kind::Tooltip) {
        Some(color) if color != style.visuals.window_fill => {
            let old = style.visuals.window_fill;
            ctx.global_style_mut(|s| s.visuals.window_fill = color);
            let result = show();
            ctx.global_style_mut(|s| s.visuals.window_fill = old);
            result
        }
        _ => show(),
    }
}
