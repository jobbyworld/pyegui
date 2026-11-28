//! The look of tabs, and a tab view. The values that give the look of a tab (the fills, the outlines, the title
//! colors, the spacing) are tokens kept for each theme and for each style of tab (classic, joined to its pane,
//! or pill, a capsule in the bar). The `Dock` of the Python extension, the `TabView` and the style sheet read the
//! same tokens: a token given once is the look of every tab of the app, and an option given to a dock or a
//! tab view still wins over it.
//!
//! The order of lookup is: the option of the call, then the token of the theme, then the default.

use std::sync::RwLock;

use egui::{Color32, CornerRadius, Rect, Sense, Stroke, TextStyle, Ui, Vec2};

use crate::dock_draw::{pane_outline, tab_foot, tab_outline};

/// What gives the look of the tabs of one style in one theme; nothing set means the default.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct TabSet {
    /// background of the bar the tabs are in
    pub bar_fill: Option<Color32>,
    pub active_fill: Option<Color32>,
    pub inactive_fill: Option<Color32>,
    pub hovered_fill: Option<Color32>,
    /// color of the title of the active tab and of the others
    pub active_text: Option<Color32>,
    pub inactive_text: Option<Color32>,
    pub active_stroke: Option<Stroke>,
    pub inactive_stroke: Option<Stroke>,
    /// space between two tabs
    pub spacing: Option<f32>,
    /// space between the side of a tab and its title
    pub title_spacing: Option<f32>,
    pub bar_height: Option<f32>,
    /// how the outline of a tab meets its pane: see [`TabOutline`]
    pub outline: Option<TabOutline>,
    /// radius of the top corners of a tab
    pub radius: Option<f32>,
    /// radius of the concave feet that join an active tab to its pane (`Joined`); none: from the radius
    pub foot: Option<f32>,
    /// space between the top of the bar and the top of the tabs
    pub top_gap: Option<f32>,
    /// size of the icon before the title
    pub icon_size: Option<f32>,
    /// the cross of a closable tab when the mouse is not on it
    pub close_stroke: Option<Stroke>,
    /// fill of the scroll arrows of a bar too narrow for its tabs, under the mouse and pressed
    pub arrow_hovered_fill: Option<Color32>,
    pub arrow_active_fill: Option<Color32>,
}

/// How the outline of a tab meets the outline of its pane
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TabOutline {
    /// two open lines that meet at the bottom of the bar, the sides of the tab going straight down (the `Dock`
    /// of the Python extension)
    #[default]
    Separate,
    /// one line around the active tab and its pane, joined by concave feet, the corners smoothed over a pixel
    /// (the dock of finder-rs)
    Joined,
}

impl TabOutline {
    /// "separate" or "joined"
    pub fn parse(name: &str) -> Option<TabOutline> {
        match name {
            "separate" => Some(TabOutline::Separate),
            "joined" => Some(TabOutline::Joined),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TabOutline::Separate => "separate",
            TabOutline::Joined => "joined",
        }
    }
}

/// `[theme][style]`: theme 0 is light, 1 dark; style 0 is classic, 1 pill
static TOKENS: RwLock<[[TabSet; 2]; 2]> = RwLock::new([[TabSet::none(); 2]; 2]);

impl TabSet {
    const fn none() -> TabSet {
        TabSet {
            bar_fill: None,
            active_fill: None,
            inactive_fill: None,
            hovered_fill: None,
            active_text: None,
            inactive_text: None,
            active_stroke: None,
            inactive_stroke: None,
            spacing: None,
            title_spacing: None,
            bar_height: None,
            outline: None,
            radius: None,
            foot: None,
            top_gap: None,
            icon_size: None,
            close_stroke: None,
            arrow_hovered_fill: None,
            arrow_active_fill: None,
        }
    }

    /// This set, the fields it does not give taken from `fallback`
    pub fn or(self, fallback: TabSet) -> TabSet {
        TabSet {
            bar_fill: self.bar_fill.or(fallback.bar_fill),
            active_fill: self.active_fill.or(fallback.active_fill),
            inactive_fill: self.inactive_fill.or(fallback.inactive_fill),
            hovered_fill: self.hovered_fill.or(fallback.hovered_fill),
            active_text: self.active_text.or(fallback.active_text),
            inactive_text: self.inactive_text.or(fallback.inactive_text),
            active_stroke: self.active_stroke.or(fallback.active_stroke),
            inactive_stroke: self.inactive_stroke.or(fallback.inactive_stroke),
            spacing: self.spacing.or(fallback.spacing),
            title_spacing: self.title_spacing.or(fallback.title_spacing),
            bar_height: self.bar_height.or(fallback.bar_height),
            outline: self.outline.or(fallback.outline),
            radius: self.radius.or(fallback.radius),
            foot: self.foot.or(fallback.foot),
            top_gap: self.top_gap.or(fallback.top_gap),
            icon_size: self.icon_size.or(fallback.icon_size),
            close_stroke: self.close_stroke.or(fallback.close_stroke),
            arrow_hovered_fill: self.arrow_hovered_fill.or(fallback.arrow_hovered_fill),
            arrow_active_fill: self.arrow_active_fill.or(fallback.arrow_active_fill),
        }
    }
}

fn index(dark: bool, pill: bool) -> (usize, usize) {
    (usize::from(dark), usize::from(pill))
}

/// The tokens given for a theme and a style of tab (nothing set where none was given)
pub fn tokens(dark: bool, pill: bool) -> TabSet {
    let (t, s) = index(dark, pill);
    TOKENS.read().unwrap_or_else(|e| e.into_inner())[t][s]
}

/// Changes the tokens of a theme and a style of tab
pub fn update_tokens(dark: bool, pill: bool, change: impl FnOnce(&mut TabSet)) {
    let (t, s) = index(dark, pill);
    let mut all = TOKENS.write().unwrap_or_else(|e| e.into_inner());
    change(&mut all[t][s]);
}

/// Forgets every token
pub fn reset_tokens() {
    *TOKENS.write().unwrap_or_else(|e| e.into_inner()) = [[TabSet::none(); 2]; 2];
}

/// A light outline that contrasts with the theme
fn border(dark: bool, alpha: u8) -> Color32 {
    if dark {
        Color32::from_white_alpha(alpha)
    } else {
        Color32::from_black_alpha(alpha)
    }
}

/// The tint of the inactive tabs, which sets them apart from the active one
fn inactive_tint(dark: bool, hovered: bool) -> Color32 {
    let alpha = match (dark, hovered) {
        (true, false) => 70,
        (true, true) => 35,
        (false, false) => 20,
        (false, true) => 10,
    };
    Color32::from_black_alpha(alpha)
}

/// Every value of the look of a tab, with the defaults in the places that nothing gives. The `given` set (options of
/// a call) wins over the tokens of the theme.
#[derive(Clone, Copy, Debug)]
pub struct TabLook {
    pub pill: bool,
    pub bar_fill: Color32,
    pub active_fill: Color32,
    pub inactive_fill: Color32,
    pub hovered_fill: Color32,
    pub active_text: Color32,
    pub inactive_text: Color32,
    pub active_stroke: Stroke,
    pub inactive_stroke: Stroke,
    pub spacing: f32,
    pub title_spacing: f32,
    pub bar_height: f32,
    pub outline: TabOutline,
    pub radius: f32,
    /// none: [`tab_foot`] of the radius of the tab
    pub foot: Option<f32>,
    pub top_gap: f32,
    pub icon_size: f32,
    /// none: the stroke of the widgets of the theme for the state of the cross
    pub close_stroke: Option<Stroke>,
    /// none: the fills of the buttons of the theme
    pub arrow_hovered_fill: Option<Color32>,
    pub arrow_active_fill: Option<Color32>,
}

impl TabLook {
    /// The radius of the feet of a tab whose corners have the radius `r`
    pub fn foot(&self, r: u8) -> f32 {
        self.foot.unwrap_or_else(|| tab_foot(r))
    }
}

/// The look of the tabs: the `given` set (options of a call) wins over the tokens of the theme, which win over
/// the defaults. The defaults depend on the outline (`tabs.outline`, `Separate` when nothing gives it):
/// `Separate` gives the look of the `Dock` of the Python extension, `Joined` the one of the dock of finder-rs
/// (no tint on the inactive tabs, a light hover, the corners rounded by at least 6, a gap above the tabs).
pub fn look(visuals: &egui::Visuals, pill: bool, given: TabSet) -> TabLook {
    let dark = visuals.dark_mode;
    let set = given.or(tokens(dark, pill));
    let outline = set.outline.unwrap_or_default();
    let joined = outline == TabOutline::Joined && !pill;
    let text = visuals.widgets.noninteractive.text_color();
    // pill: the active tab is the selection color and the others are bare; classic: the active tab has the
    // background of its pane (transparent), the others a dark tint. Joined: the active tab is the sheet of its
    // pane, the others show the bar through
    let active_fill = set.active_fill.unwrap_or(if pill {
        visuals.selection.bg_fill
    } else if joined {
        visuals.panel_fill
    } else {
        Color32::TRANSPARENT
    });
    let default_hovered = if pill {
        visuals.widgets.hovered.weak_bg_fill
    } else if joined {
        if dark { Color32::from_white_alpha(14) } else { active_fill.gamma_multiply(0.4) }
    } else {
        inactive_tint(dark, true)
    };
    let default_inactive_text = if joined {
        visuals.weak_text_color().gamma_multiply(if dark { 1.0 } else { 0.7 })
    } else {
        text.gamma_multiply(0.75)
    };
    let default_active_text = if pill {
        visuals.selection.stroke.color
    } else if joined {
        visuals.text_color()
    } else {
        visuals.widgets.active.text_color()
    };
    let theme_radius = f32::from(visuals.widgets.inactive.corner_radius.nw);
    TabLook {
        pill,
        bar_fill: set.bar_fill.unwrap_or(Color32::TRANSPARENT),
        active_fill,
        inactive_fill: set.inactive_fill.unwrap_or(if pill || joined { Color32::TRANSPARENT } else { inactive_tint(dark, false) }),
        hovered_fill: set.hovered_fill.unwrap_or(default_hovered),
        active_text: set.active_text.unwrap_or(default_active_text),
        inactive_text: set.inactive_text.unwrap_or(default_inactive_text),
        active_stroke: set.active_stroke.unwrap_or(if pill { Stroke::NONE } else { Stroke::new(1.0, border(dark, 45)) }),
        inactive_stroke: set.inactive_stroke.unwrap_or(if pill { Stroke::NONE } else { Stroke::new(1.0, border(dark, 25)) }),
        spacing: set.spacing.unwrap_or(if pill { 4.0 } else { 2.0 }),
        title_spacing: set.title_spacing.unwrap_or(8.0),
        bar_height: set.bar_height.unwrap_or(24.0),
        outline,
        radius: set.radius.unwrap_or(if joined { theme_radius.max(6.0) } else { theme_radius }),
        foot: set.foot,
        top_gap: set.top_gap.unwrap_or(if joined { crate::columns::TAB_TOP_GAP } else { 0.0 }),
        icon_size: set.icon_size.unwrap_or(14.0),
        close_stroke: set.close_stroke,
        arrow_hovered_fill: set.arrow_hovered_fill.or(joined.then(|| Color32::from_white_alpha(20))),
        arrow_active_fill: set.arrow_active_fill.or(joined.then(|| Color32::from_white_alpha(32))),
    }
}

/// What the user did in a tab view during the frame
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct TabEvents {
    /// a tab was clicked (the index of the tab)
    pub selected: Option<usize>,
    /// the cross of a tab was clicked, or the middle button on a closable tab
    pub closed: Option<usize>,
    /// the "+" button was clicked
    pub added: bool,
}

#[derive(Clone, Debug, Default)]
pub struct TabViewOptions {
    /// pill tabs, a capsule in the bar, instead of tabs joined to their pane
    pub pill: bool,
    pub closable: bool,
    /// show a "+" after the last tab
    pub add_button: bool,
    /// the options of the call, that win over the tokens
    pub given: TabSet,
    /// space inside the pane around the content
    pub margin: f32,
}

/// Draws a bar of tabs and, under it, the pane with the content of the active tab. `active` is the index of the
/// active tab. Returns what was done; the caller changes `active`, adds or removes titles.
pub fn tab_view(ui: &mut Ui, titles: &[String], active: usize, opts: &TabViewOptions, content: impl FnOnce(&mut Ui)) -> TabEvents {
    let look = look(ui.visuals(), opts.pill, opts.given);
    let mut events = TabEvents::default();
    let width = ui.available_width();
    let (bar, _) = ui.allocate_exact_size(Vec2::new(width, look.bar_height), Sense::hover());
    let painter = ui.painter().clone();
    if look.bar_fill.a() > 0 {
        painter.rect_filled(bar, 0.0, look.bar_fill);
    }
    let font = TextStyle::Button.resolve(ui.style());
    let radius = if opts.pill { 0 } else { ui.visuals().widgets.inactive.corner_radius.nw };
    let x_margin = if opts.pill { look.title_spacing } else { look.title_spacing.max(f32::from(radius) * 0.6) + look.active_stroke.width };
    let cross = 14.0;
    let mut x = bar.left();
    let mut active_rect = None;
    for (i, title) in titles.iter().enumerate() {
        let is_active = i == active;
        let color = if is_active { look.active_text } else { look.inactive_text };
        let galley = painter.layout_no_wrap(title.clone(), font.clone(), color);
        let w = galley.size().x + 2.0 * x_margin + if opts.closable { 4.0 + cross } else { 0.0 };
        let tab = Rect::from_min_size(egui::pos2(x, bar.top()), Vec2::new(w, bar.height()));
        x += w + look.spacing;
        let response = ui.interact(tab, ui.auto_id_with(("tab", i)), Sense::click());
        let hovered = response.hovered();
        let fill = if is_active { look.active_fill } else if hovered { look.hovered_fill } else { look.inactive_fill };
        let stroke = if is_active { look.active_stroke } else { look.inactive_stroke };
        if opts.pill {
            let pill = tab.shrink2(Vec2::new(0.0, 3.0));
            let r = (pill.height() / 2.0).min(255.0) as u8;
            painter.rect(pill, CornerRadius::same(r), fill, stroke, egui::StrokeKind::Inside);
        } else {
            let r = f32::from(radius).min(tab.width() / 2.0).min(tab.height() / 2.0).floor() as u8;
            painter.rect_filled(tab, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, fill);
            if stroke.width > 0.0 {
                // the active tab joins its pane (the outline of the pane goes around it): only the others have one here
                if !is_active {
                    painter.add(egui::Shape::line(tab_outline(tab.shrink(stroke.width / 2.0), (f32::from(r) - stroke.width / 2.0).max(0.0), 0.0), stroke));
                }
            }
            if is_active {
                active_rect = Some((tab, r));
            }
        }
        let text_pos = egui::Align2::LEFT_CENTER.align_size_within_rect(galley.size(), tab.shrink(x_margin)).min;
        painter.galley(text_pos, galley, color);
        if opts.closable {
            let c = egui::Align2::RIGHT_CENTER.align_size_within_rect(Vec2::splat(cross), tab.shrink(x_margin));
            let hit = ui.interact(c, ui.auto_id_with(("close", i)), Sense::click());
            let ink = if hit.hovered() { ui.visuals().widgets.hovered.fg_stroke } else { Stroke::new(1.0, color) };
            let mark = c.shrink(4.0);
            painter.line_segment([mark.left_top(), mark.right_bottom()], ink);
            painter.line_segment([mark.right_top(), mark.left_bottom()], ink);
            if hit.clicked() || response.clicked_by(egui::PointerButton::Middle) {
                events.closed = Some(i);
                continue;
            }
        }
        if response.clicked() && !is_active {
            events.selected = Some(i);
        }
    }
    if opts.add_button {
        let size = look.bar_height - 6.0;
        let rect = Rect::from_min_size(egui::pos2(x + 2.0, bar.top() + 3.0), Vec2::splat(size));
        let response = ui.interact(rect, ui.auto_id_with("add"), Sense::click());
        if response.hovered() {
            painter.rect_filled(rect, CornerRadius::same((size / 2.0) as u8), look.hovered_fill);
        }
        let c = rect.center();
        let ink = Stroke::new(1.0, look.inactive_text);
        painter.line_segment([c - Vec2::new(4.0, 0.0), c + Vec2::new(4.0, 0.0)], ink);
        painter.line_segment([c - Vec2::new(0.0, 4.0), c + Vec2::new(0.0, 4.0)], ink);
        events.added = response.clicked();
    }

    // the pane under the bar, with the content of the active tab
    let inner_width = (width - 2.0 * opts.margin).max(0.0);
    let inner = egui::Frame::new().inner_margin(opts.margin).show(ui, |ui| {
        ui.set_min_width(inner_width);
        content(ui);
    });
    let area = inner.response.rect;
    if !opts.pill {
        let rr = f32::from(radius).max(4.0);
        let tab = active_rect.filter(|(t, _)| t.min.x >= area.left() && t.max.x <= area.right()).map(|(t, r)| (t, r));
        let stroke = look.active_stroke;
        if stroke.width > 0.0 {
            // the outline runs half its width inside the area
            let pts = pane_outline(area.shrink(stroke.width / 2.0), tab.map(|(t, r)| (Rect::from_min_max(egui::pos2(t.min.x, area.top() - t.height()), egui::pos2(t.max.x, area.top())), r)), true, rr);
            let _ = tab_foot; // the feet of the tabs are given by the outline of the pane
            ui.painter().add(egui::Shape::closed_line(pts, stroke));
        }
    } else if look.active_stroke.width > 0.0 {
        ui.painter().rect_stroke(area, CornerRadius::same(radius), look.active_stroke, egui::StrokeKind::Inside);
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the tokens are global: the tests that change them run one after the other
    use crate::TOKENS_TEST_LOCK as ONE_AT_A_TIME;

    #[test]
    fn tokens_are_per_theme_and_style() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        reset_tokens();
        update_tokens(true, false, |t| t.spacing = Some(9.0));
        assert_eq!(tokens(true, false).spacing, Some(9.0));
        assert_eq!(tokens(false, false).spacing, None);
        assert_eq!(tokens(true, true).spacing, None);
        reset_tokens();
    }

    #[test]
    fn given_wins_over_token_over_default() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        reset_tokens();
        let visuals = egui::Visuals::dark();
        assert_eq!(look(&visuals, false, TabSet::default()).spacing, 2.0);
        assert_eq!(look(&visuals, true, TabSet::default()).spacing, 4.0);
        update_tokens(true, false, |t| t.spacing = Some(7.0));
        assert_eq!(look(&visuals, false, TabSet::default()).spacing, 7.0);
        let given = TabSet { spacing: Some(11.0), ..TabSet::default() };
        assert_eq!(look(&visuals, false, given).spacing, 11.0);
        reset_tokens();
    }

    #[test]
    fn the_outline_token_picks_the_defaults_of_its_variant() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        reset_tokens();
        let dark = egui::Visuals::dark();
        let separate = look(&dark, false, TabSet::default());
        assert_eq!(separate.outline, TabOutline::Separate);
        assert_eq!(separate.top_gap, 0.0);
        assert_eq!(separate.inactive_fill, inactive_tint(true, false));
        assert_eq!(separate.arrow_hovered_fill, None);
        update_tokens(true, false, |t| t.outline = Some(TabOutline::Joined));
        let joined = look(&dark, false, TabSet::default());
        assert_eq!(joined.outline, TabOutline::Joined);
        assert_eq!(joined.inactive_fill, Color32::TRANSPARENT);
        assert_eq!(joined.hovered_fill, Color32::from_white_alpha(14));
        assert_eq!(joined.arrow_hovered_fill, Some(Color32::from_white_alpha(20)));
        assert_eq!(joined.arrow_active_fill, Some(Color32::from_white_alpha(32)));
        assert_eq!(joined.top_gap, crate::columns::TAB_TOP_GAP);
        assert!(joined.radius >= 6.0);
        // light: the hover is the sheet of the active tab, fainter
        let light = egui::Visuals::light();
        let given = TabSet { outline: Some(TabOutline::Joined), active_fill: Some(Color32::from_rgb(200, 100, 50)), ..TabSet::default() };
        assert_eq!(look(&light, false, given).hovered_fill, Color32::from_rgb(200, 100, 50).gamma_multiply(0.4));
        // the option of a call wins over the token
        let given = TabSet { outline: Some(TabOutline::Separate), ..TabSet::default() };
        assert_eq!(look(&dark, false, given).outline, TabOutline::Separate);
        reset_tokens();
    }

    #[test]
    fn the_new_tokens_follow_the_lookup_order() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        reset_tokens();
        let v = egui::Visuals::light();
        assert_eq!(look(&v, false, TabSet::default()).icon_size, 14.0);
        assert_eq!(look(&v, false, TabSet::default()).foot, None);
        assert_eq!(look(&v, false, TabSet::default()).foot(6), tab_foot(6));
        update_tokens(false, false, |t| {
            t.radius = Some(10.0);
            t.foot = Some(5.0);
            t.top_gap = Some(3.0);
            t.icon_size = Some(16.0);
            t.close_stroke = Some(Stroke::new(1.5, Color32::RED));
        });
        let l = look(&v, false, TabSet::default());
        assert_eq!((l.radius, l.foot(6), l.top_gap, l.icon_size), (10.0, 5.0, 3.0, 16.0));
        assert_eq!(l.close_stroke, Some(Stroke::new(1.5, Color32::RED)));
        let given = TabSet { radius: Some(2.0), icon_size: Some(12.0), ..TabSet::default() };
        let l = look(&v, false, given);
        assert_eq!((l.radius, l.icon_size, l.top_gap), (2.0, 12.0, 3.0));
        // the pill tabs have their own tokens
        assert_eq!(look(&v, true, TabSet::default()).radius, f32::from(v.widgets.inactive.corner_radius.nw));
        assert_eq!(TabOutline::parse("joined"), Some(TabOutline::Joined));
        assert_eq!(TabOutline::parse(TabOutline::Separate.name()), Some(TabOutline::Separate));
        assert_eq!(TabOutline::parse("other"), None);
        reset_tokens();
    }
}
