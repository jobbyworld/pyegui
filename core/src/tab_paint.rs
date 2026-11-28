//! One painter for the tabs of a dock, whatever holds them (`egui_tiles` in the Python extension and in
//! finder-rs): the fill, the outline, the icon, the title and the close cross of a tab, from a [`TabLook`]
//! (see [`crate::tabs::look`]). The outline has two variants ([`TabOutline`]): `Separate`, the tab and its pane
//! drawn as two open lines that meet at the bottom of the bar, and `Joined`, one line around the active tab and
//! its pane with concave feet and corners smoothed over a pixel. The outline of the pane is drawn by the host
//! with [`crate::dock_draw::separate_pane_outline`] or [`crate::dock_draw::pane_outline_with_foot`].
//!
//! A host computes the width of a tab ([`tab_width`]), allocates it, places the tab in it ([`place`]), reads
//! the mouse on the tab and on its cross ([`close_rect`]), then calls [`paint_tab`].

use std::sync::Arc;

use egui::{Color32, CornerRadius, Galley, Rect, Response, Shape, Ui, Vec2};

use crate::dock_draw::{corner_mesh, dedup, feathered_fan, separate_tab_outline, snap, tab_outline, SEAM};
use crate::tabs::{TabLook, TabOutline};

/// Space between the icon of a tab and its title
pub const ICON_GAP: f32 = 5.0;
/// Space between the title and the close cross
pub const CLOSE_GAP: f32 = 4.0;
/// The size of the close button of a tab, and how much smaller its cross is (the values of `egui_tiles`)
pub const CLOSE_SIZE: f32 = 12.0;
pub const CLOSE_MARGIN: f32 = 2.0;

/// What a tab is doing this frame
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct TabState {
    pub active: bool,
    /// the mouse is on the tab
    pub hovered: bool,
    /// the tab is dragged: nothing is drawn where it was
    pub dragging: bool,
    pub closable: bool,
}

/// What the painter needs for one tab
pub struct TabPaint<'a> {
    /// the rectangle of the tab, from [`place`]
    pub rect: Rect,
    pub look: &'a TabLook,
    pub state: TabState,
    pub galley: Arc<Galley>,
    /// draws the icon in the given square with the given color (the color of the title)
    pub icon: Option<&'a dyn Fn(&Ui, Rect, Color32)>,
    /// space between the sides of the tab and its content, from [`x_margin`]
    pub x_margin: f32,
    /// the response of the close button (interacted at [`close_rect`]), for its hover and press states
    pub close: Option<&'a Response>,
    /// `Joined`: the tint of the bar, laid in the corners that the round top of the active tab and its feet
    /// leave clear (the glass behind does not show through the seams)
    pub bar_tint: Color32,
}

/// What a host does with a painted tab: the shapes of the active tab that go with its pane
#[derive(Default)]
pub struct TabPainted {
    /// the active tab as it is drawn and its corner radius: the outline of its pane goes around it
    pub active: Option<(Rect, u8)>,
    /// `Joined`: the fill of the active tab and of its feet, painted with the sheet of the pane
    pub fills: Vec<Shape>,
    /// `Joined`: the rectangles covered by the active tab and the squares of its feet (taken out of the glass of
    /// the bands by the host)
    pub sheets: Vec<Rect>,
    /// `Joined`: the corners of the round top and of the feet, filled with the tint of the bar
    pub corners: Vec<egui::Mesh>,
}

/// The space between the sides of a tab and its content. `stroke_width` is the width of the outline of this
/// tab, `height` the height of the bar.
pub fn x_margin(look: &TabLook, height: f32, stroke_width: f32) -> f32 {
    if look.pill {
        look.title_spacing
    } else {
        match look.outline {
            // the title keeps clear of the rounded corners and of the outline, however thick
            TabOutline::Separate => look.title_spacing.max(look.radius.min(height / 2.0) * 0.6) + stroke_width,
            TabOutline::Joined => look.title_spacing.max(look.radius * 0.6),
        }
    }
}

/// The width of a tab whose title is `title_width` wide
pub fn tab_width(look: &TabLook, title_width: f32, icon: bool, closable: bool, x_margin: f32) -> f32 {
    title_width + if icon { look.icon_size + ICON_GAP } else { 0.0 } + 2.0 * x_margin + if closable { CLOSE_GAP + CLOSE_SIZE } else { 0.0 }
}

/// The rectangle of a tab of `width` allocated at the top left of `outer` (the space taken in the bar): it
/// stands on the bottom of the bar, `top_gap` below its top; `Joined` puts it on the pixels.
pub fn place(ui: &Ui, look: &TabLook, outer: Rect, width: f32) -> Rect {
    if look.top_gap == 0.0 && look.outline == TabOutline::Separate {
        return Rect::from_min_size(outer.min, egui::vec2(width, outer.height()));
    }
    let rect = Rect::from_min_max(egui::pos2(outer.min.x, outer.min.y + look.top_gap), egui::pos2(outer.min.x + width, outer.max.y));
    if look.outline == TabOutline::Joined {
        snap(ui, rect)
    } else {
        rect
    }
}

/// The button of the close cross of a tab
pub fn close_rect(rect: Rect, x_margin: f32) -> Rect {
    egui::Align2::RIGHT_CENTER.align_size_within_rect(Vec2::splat(CLOSE_SIZE), rect.shrink(x_margin))
}

/// The corner radius of the top of a tab: egui rounds a rectangle at most by half its smaller side, the outline
/// uses the same radius so the fill and the line stay together
pub fn corner_radius(look: &TabLook, rect: Rect) -> u8 {
    look.radius.round().clamp(0.0, 255.0).min(rect.width() / 2.0).min(rect.height() / 2.0).floor() as u8
}

/// Paints a tab: fill, outline, icon, title and close cross. Returns the shapes of the active tab that the host
/// paints with its pane (`Joined`) and the rectangle the outline of the pane goes around.
pub fn paint_tab(ui: &Ui, tab: &TabPaint<'_>) -> TabPainted {
    let mut out = TabPainted::default();
    let look = tab.look;
    let rect = tab.rect;
    let state = tab.state;
    if !ui.is_rect_visible(rect) || state.dragging {
        return out;
    }
    let painter = ui.painter();
    let fill = if state.active {
        look.active_fill
    } else if state.hovered {
        look.hovered_fill
    } else {
        look.inactive_fill
    };
    let stroke = if state.active { look.active_stroke } else { look.inactive_stroke };
    if look.pill {
        // a capsule floating in the bar
        let pill = rect.shrink2(egui::vec2(0.0, 3.0));
        let radius = (pill.height() / 2.0).min(255.0) as u8;
        painter.rect(pill, CornerRadius::same(radius), fill, stroke, egui::StrokeKind::Inside);
    } else {
        let r = corner_radius(look, rect);
        let top = CornerRadius { nw: r, ne: r, sw: 0, se: 0 };
        match look.outline {
            TabOutline::Separate => {
                painter.rect_filled(rect, top, fill);
                if stroke.width > 0.0 {
                    // open at the bottom, so the tab connects with its pane; the line runs half its width inside
                    // the fill: its corners are concentric with the ones of the fill
                    painter.add(Shape::line(separate_tab_outline(rect.shrink(stroke.width / 2.0), (f32::from(r) - stroke.width / 2.0).max(0.0)), stroke));
                }
                if state.active {
                    out.active = Some((rect, r));
                }
            }
            TabOutline::Joined if !state.active => {
                painter.rect_filled(rect, top, fill);
                painter.add(Shape::line(dedup(tab_outline(rect, f32::from(r), 0.0), false), stroke));
            }
            TabOutline::Joined => joined_active(ui, look, rect, r, fill, tab.bar_tint, &mut out),
        }
    }

    let color = if state.active { look.active_text } else { look.inactive_text };
    let inner = rect.shrink(tab.x_margin);
    let mut text_area = inner;
    if let Some(icon) = tab.icon {
        let spot = Rect::from_center_size(egui::pos2(inner.min.x + look.icon_size / 2.0, inner.center().y), Vec2::splat(look.icon_size));
        icon(ui, spot, color);
        text_area = Rect::from_min_max(inner.min + egui::vec2(look.icon_size + ICON_GAP, 0.0), inner.max);
    }
    let at = egui::Align2::LEFT_CENTER.align_size_within_rect(tab.galley.size(), text_area).min;
    painter.galley(at, tab.galley.clone(), color);

    if state.closable {
        let button = close_rect(rect, tab.x_margin);
        let (stroke, expansion) = match tab.close {
            Some(response) => {
                let visuals = ui.style().interact(response);
                let idle = !response.hovered() && !response.is_pointer_button_down_on();
                (look.close_stroke.filter(|_| idle).unwrap_or(visuals.fg_stroke), visuals.expansion)
            }
            None => (look.close_stroke.unwrap_or(ui.visuals().widgets.inactive.fg_stroke), 0.0),
        };
        let cross = button.shrink(CLOSE_MARGIN).expand(expansion);
        painter.line_segment([cross.left_top(), cross.right_bottom()], stroke);
        painter.line_segment([cross.right_top(), cross.left_bottom()], stroke);
    }
    out
}

/// `Joined`: the active tab, its feet and the corners that its curves leave to the tint of the bar
fn joined_active(ui: &Ui, look: &TabLook, rect: Rect, r: u8, fill: Color32, tint: Color32, out: &mut TabPainted) {
    // an active tab scrolled out of the bar (partly hidden) is drawn as the part that is seen: a narrower tab,
    // so its feet, corners and outline stay inside the bar and join the pane
    let rect = rect.intersect(ui.clip_rect());
    let rect = if rect.width() < 2.0 * f32::from(r) { Rect::from_min_size(rect.min, egui::vec2(2.0 * f32::from(r), rect.height())) } else { rect };
    out.sheets.push(rect);
    out.fills.push(Shape::rect_filled(rect, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, fill));
    out.active = Some((rect, r));
    let foot = look.foot(r);
    // the first tab is flush with the side of the pane: no foot on that side
    let flush = rect.min.x - foot < ui.max_rect().left() - 0.01;
    let feet = [(rect.left_bottom(), -1.0f32), (rect.right_bottom(), 1.0f32)];
    let feet = feet.into_iter().filter(|(_, dir)| !(*dir < 0.0 && flush));
    for (corner, dir) in feet.clone() {
        let center = corner + egui::vec2(dir * foot, -foot);
        out.sheets.push(Rect::from_two_pos(corner, center));
        // the quarter of disc on the far side of the curve
        out.corners.push(feathered_fan(center, foot, false, SEAM, tint, |rad, i| {
            let a = std::f32::consts::FRAC_PI_2 * i as f32 / 8.0;
            egui::pos2(center.x - dir * rad * a.cos(), center.y + rad * a.sin())
        }));
    }
    for (outer, dir) in [(rect.left_top(), 1.0f32), (rect.right_top(), -1.0f32)] {
        out.corners.push(corner_mesh(outer, dir, 1.0, f32::from(r), tint));
    }
    // concave feet at both bottom corners join the tab to its pane
    for (corner, dir) in feet {
        let center = corner + egui::vec2(dir * foot, -foot);
        out.fills.push(Shape::mesh(feathered_fan(corner, foot, true, 0.0, fill, |rad, i| {
            let a = std::f32::consts::FRAC_PI_2 * i as f32 / 8.0;
            egui::pos2(center.x - dir * rad * a.cos(), center.y + rad * a.sin())
        })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabs::{look, TabSet};
    use egui::{pos2, Color32, Stroke};

    fn given(outline: TabOutline) -> TabSet {
        TabSet {
            outline: Some(outline),
            radius: Some(8.0),
            title_spacing: Some(8.0),
            active_fill: Some(Color32::from_rgb(10, 20, 30)),
            active_stroke: Some(Stroke::new(1.0, Color32::WHITE)),
            inactive_stroke: Some(Stroke::new(1.0, Color32::GRAY)),
            top_gap: Some(if outline == TabOutline::Joined { 6.0 } else { 0.0 }),
            ..TabSet::default()
        }
    }

    /// runs `f` in a ui of a context with no window
    fn in_ui(f: impl FnOnce(&mut Ui)) {
        let ctx = egui::Context::default();
        let mut f = Some(f);
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))), ..Default::default() }, |ui| {
            if let Some(f) = f.take() {
                f(ui);
            }
        });
        output.textures_delta.clear();
    }

    #[test]
    fn margins_and_widths_follow_the_variant() {
        let v = egui::Visuals::dark();
        let sep = look(&v, false, given(TabOutline::Separate));
        // separate: the radius (at most half the bar) * 0.6, plus the stroke
        assert_eq!(x_margin(&sep, 24.0, 1.0), 8.0f32.max(8.0 * 0.6) + 1.0);
        let big = look(&v, false, TabSet { radius: Some(30.0), ..given(TabOutline::Separate) });
        // the radius counts for half the bar at most: 12 * 0.6 < 8
        assert_eq!(x_margin(&big, 24.0, 2.0), 10.0);
        let joined = look(&v, false, TabSet { radius: Some(30.0), ..given(TabOutline::Joined) });
        assert_eq!(x_margin(&joined, 24.0, 2.0), 18.0);
        let pill = look(&v, true, given(TabOutline::Separate));
        assert_eq!(x_margin(&pill, 24.0, 1.0), 8.0);
        assert_eq!(tab_width(&sep, 40.0, false, false, 9.0), 58.0);
        assert_eq!(tab_width(&sep, 40.0, true, true, 9.0), 58.0 + sep.icon_size + ICON_GAP + CLOSE_GAP + CLOSE_SIZE);
        let r = Rect::from_min_max(pos2(0.0, 0.0), pos2(60.0, 24.0));
        assert_eq!(close_rect(r, 9.0), Rect::from_min_max(pos2(39.0, 6.0), pos2(51.0, 18.0)));
        assert_eq!(corner_radius(&big, r), 12);
        assert_eq!(corner_radius(&sep, r), 8);
    }

    #[test]
    fn separate_and_joined_place_the_tab() {
        in_ui(|ui| {
            let v = ui.visuals().clone();
            let outer = Rect::from_min_max(pos2(10.3, 20.0), pos2(200.0, 46.0));
            let sep = look(&v, false, given(TabOutline::Separate));
            assert_eq!(place(ui, &sep, outer, 50.0), Rect::from_min_size(outer.min, egui::vec2(50.0, 26.0)));
            let joined = look(&v, false, given(TabOutline::Joined));
            let p = place(ui, &joined, outer, 50.0);
            assert_eq!(p.top(), 26.0);
            assert_eq!(p.bottom(), 46.0);
            // on the pixels (1 point per pixel here)
            assert_eq!(p.left(), 10.0);
        });
    }

    #[test]
    fn a_separate_active_tab_is_drawn_and_given_to_its_pane() {
        in_ui(|ui| {
            let v = ui.visuals().clone();
            let l = look(&v, false, given(TabOutline::Separate));
            let galley = ui.painter().layout_no_wrap("Files".into(), egui::FontId::default(), Color32::WHITE);
            let rect = Rect::from_min_max(pos2(10.0, 10.0), pos2(80.0, 34.0));
            let out = paint_tab(ui, &TabPaint { rect, look: &l, state: TabState { active: true, ..Default::default() }, galley, icon: None, x_margin: 9.0, close: None, bar_tint: Color32::TRANSPARENT });
            assert_eq!(out.active, Some((rect, 8)));
            assert!(out.fills.is_empty() && out.sheets.is_empty() && out.corners.is_empty());
        });
    }

    #[test]
    fn a_joined_active_tab_gives_its_fills_feet_and_corners() {
        in_ui(|ui| {
            let v = ui.visuals().clone();
            let l = look(&v, false, given(TabOutline::Joined));
            let galley = ui.painter().layout_no_wrap("Tags".into(), egui::FontId::default(), Color32::WHITE);
            // far from the left side of the ui: both feet
            let left = ui.max_rect().left();
            let rect = Rect::from_min_max(pos2(left + 50.0, 10.0), pos2(left + 120.0, 34.0));
            let out = paint_tab(ui, &TabPaint { rect, look: &l, state: TabState { active: true, ..Default::default() }, galley: galley.clone(), icon: None, x_margin: 8.0, close: None, bar_tint: Color32::BLACK });
            assert_eq!(out.active, Some((rect, 8)));
            // the tab and its two feet
            assert_eq!(out.fills.len(), 3);
            assert_eq!(out.sheets.len(), 3);
            // two feet and two round top corners
            assert_eq!(out.corners.len(), 4);
            // flush with the left side: no foot on the left
            let flush = Rect::from_min_max(pos2(left, 10.0), pos2(left + 70.0, 34.0));
            let out = paint_tab(ui, &TabPaint { rect: flush, look: &l, state: TabState { active: true, ..Default::default() }, galley: galley.clone(), icon: None, x_margin: 8.0, close: None, bar_tint: Color32::BLACK });
            assert_eq!((out.fills.len(), out.sheets.len(), out.corners.len()), (2, 2, 3));
            // a dragged tab is not drawn
            let out = paint_tab(ui, &TabPaint { rect, look: &l, state: TabState { active: true, dragging: true, ..Default::default() }, galley, icon: None, x_margin: 8.0, close: None, bar_tint: Color32::BLACK });
            assert!(out.active.is_none());
        });
    }

    #[test]
    fn the_icon_is_drawn_before_the_title_in_the_color_of_the_title() {
        in_ui(|ui| {
            let v = ui.visuals().clone();
            let l = look(&v, false, given(TabOutline::Joined));
            let galley = ui.painter().layout_no_wrap("Tags".into(), egui::FontId::default(), Color32::WHITE);
            let rect = Rect::from_min_max(pos2(50.0, 10.0), pos2(140.0, 34.0));
            let seen = std::cell::Cell::new(None);
            let icon = |_: &Ui, spot: Rect, color: Color32| seen.set(Some((spot, color)));
            paint_tab(ui, &TabPaint { rect, look: &l, state: TabState::default(), galley, icon: Some(&icon), x_margin: 8.0, close: None, bar_tint: Color32::BLACK });
            let (spot, color) = seen.get().unwrap();
            assert_eq!(spot.size(), Vec2::splat(l.icon_size));
            assert_eq!(spot.left(), 58.0);
            assert_eq!(color, l.inactive_text);
        });
    }
}
