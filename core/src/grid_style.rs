//! The look of the lines and of the open symbol of the grids (`DataGrid`, and the trees and lists drawn with it).
//! The values are tokens kept for each theme, like the ones of the tabs: `grid.row_line`, `grid.column_line`,
//! `grid.guide_line` (the lines of a tree), `grid.guide_radius` (the rounding of the elbows where a line meets a row),
//! `grid.chevron` (the symbol that opens a branch: triangle, arrow or plus), `grid.chevron_color`,
//! `grid.chevron_hover_color` and `grid.chevron_size`.
//!
//! The order of lookup is: the token of the theme, then the default drawn from the theme.

use std::sync::RwLock;

use egui::{Color32, Stroke, Visuals};

/// The symbol of a branch that opens and closes
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum Chevron {
    /// a filled triangle, pointing right when closed and down when open
    #[default]
    Triangle,
    /// an open arrow (a chevron)
    Arrow,
    /// a small square with a plus when closed and a minus when open
    Plus,
}

impl Chevron {
    pub fn parse(name: &str) -> Option<Chevron> {
        Some(match name {
            "triangle" => Chevron::Triangle,
            "arrow" => Chevron::Arrow,
            "plus" => Chevron::Plus,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Chevron::Triangle => "triangle",
            Chevron::Arrow => "arrow",
            Chevron::Plus => "plus",
        }
    }
}

/// What the user gave for one theme; nothing set means the default
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct GridSet {
    pub row_line: Option<Stroke>,
    pub column_line: Option<Stroke>,
    pub guide_line: Option<Stroke>,
    pub guide_radius: Option<f32>,
    pub chevron: Option<Chevron>,
    pub chevron_color: Option<Color32>,
    pub chevron_hover_color: Option<Color32>,
    pub chevron_size: Option<f32>,
}

/// `[theme]`: 0 is light, 1 dark
static TOKENS: RwLock<[GridSet; 2]> = RwLock::new([GridSet {
    row_line: None,
    column_line: None,
    guide_line: None,
    guide_radius: None,
    chevron: None,
    chevron_color: None,
    chevron_hover_color: None,
    chevron_size: None,
}; 2]);

/// The tokens given for a theme
pub fn tokens(dark: bool) -> GridSet {
    TOKENS.read().unwrap_or_else(|e| e.into_inner())[usize::from(dark)]
}

/// Changes the tokens of a theme
pub fn update_tokens(dark: bool, change: impl FnOnce(&mut GridSet)) {
    change(&mut TOKENS.write().unwrap_or_else(|e| e.into_inner())[usize::from(dark)]);
}

/// Forgets every token
pub fn reset_tokens() {
    *TOKENS.write().unwrap_or_else(|e| e.into_inner()) = [GridSet::default(); 2];
}

/// Every value, with the defaults where no token is given
#[derive(Clone, Copy, Debug)]
pub struct GridLook {
    pub row_line: Stroke,
    pub column_line: Stroke,
    pub guide_line: Stroke,
    pub guide_radius: f32,
    pub chevron: Chevron,
    pub chevron_color: Color32,
    pub chevron_hover_color: Color32,
    /// half the width of the symbol
    pub chevron_size: f32,
}

/// The look for these visuals
pub fn look(visuals: &Visuals) -> GridLook {
    let t = tokens(visuals.dark_mode);
    let line = visuals.widgets.noninteractive.bg_stroke.color;
    GridLook {
        row_line: t.row_line.unwrap_or(Stroke::new(1.0, line.gamma_multiply(0.5))),
        column_line: t.column_line.unwrap_or(Stroke::new(1.0, line.gamma_multiply(0.6))),
        guide_line: t.guide_line.unwrap_or(Stroke::new(1.0, line.gamma_multiply(0.9))),
        guide_radius: t.guide_radius.unwrap_or(0.0),
        chevron: t.chevron.unwrap_or_default(),
        chevron_color: t.chevron_color.unwrap_or(visuals.weak_text_color()),
        chevron_hover_color: t.chevron_hover_color.unwrap_or(visuals.strong_text_color()),
        chevron_size: t.chevron_size.unwrap_or(4.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the tokens are global: the tests that change them (here and in other modules) run one after the other
    use crate::TOKENS_TEST_LOCK as LOCK;

    #[test]
    fn a_token_wins_over_the_default_and_reset_forgets_it() {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_tokens();
        let visuals = Visuals::light();
        let default = look(&visuals);
        update_tokens(false, |t| {
            t.guide_radius = Some(6.0);
            t.chevron = Some(Chevron::Plus);
        });
        let given = look(&visuals);
        assert_eq!(given.guide_radius, 6.0);
        assert_eq!(given.chevron, Chevron::Plus);
        assert_eq!(given.row_line, default.row_line);
        // the other theme is not changed
        assert_eq!(look(&Visuals::dark()).guide_radius, 0.0);
        reset_tokens();
        assert_eq!(look(&visuals).guide_radius, 0.0);
    }

    #[test]
    fn the_names_of_the_symbols_round_trip() {
        for c in [Chevron::Triangle, Chevron::Arrow, Chevron::Plus] {
            assert_eq!(Chevron::parse(c.name()), Some(c));
        }
        assert_eq!(Chevron::parse("x"), None);
    }
}
