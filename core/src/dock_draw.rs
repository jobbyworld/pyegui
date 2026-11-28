//! The geometry and the meshes that draw a dock in glass: the outline of a tab and of its pane (one line
//! around both, with the feet that join them), the corners of the sheets smoothed over one pixel, and the
//! rectangles snapped to the pixels. Shared by the Rust applications and the Python extension, so every
//! dock draws its outlines and its rounded corners the same way.

/// The outline of an inactive tab stops this far above the bottom of the bar
pub const INACTIVE_GAP: f32 = 4.0;

/// A path without the points that repeat the previous one (the end of an arc is the start of the next
/// segment): a stroke of egui doubles its width there, which shows as dots when the color is semi transparent
pub fn dedup(mut pts: Vec<egui::Pos2>, closed: bool) -> Vec<egui::Pos2> {
    pts.dedup_by(|b, a| a.distance(*b) < 0.05);
    if closed && pts.len() > 1 && pts[0].distance(pts[pts.len() - 1]) < 0.05 {
        pts.pop();
    }
    pts
}

/// The outline of a tab, open at the bottom so it joins its pane
pub fn tab_outline(rect: egui::Rect, radius: f32, foot: f32) -> Vec<egui::Pos2> {
    use std::f32::consts::FRAC_PI_2;
    let radius = radius.min(rect.width() / 2.0).min(rect.height());
    let steps = (radius.ceil() as usize).clamp(1, 8);
    let mut points = Vec::new();
    if foot > 0.0 {
        // the concave foot joins the side of the tab to the top of the pane
        let center = rect.left_bottom() + egui::vec2(-foot, -foot);
        for i in (0..=12).rev() {
            let a = FRAC_PI_2 * i as f32 / 12.0;
            points.push(egui::pos2(center.x + foot * a.cos(), center.y + foot * a.sin()));
        }
    } else {
        points.push(rect.left_bottom() - egui::vec2(0.0, INACTIVE_GAP));
    }
    let mut corner = |center: egui::Pos2, start: f32| {
        for i in 0..=steps {
            let angle = start + FRAC_PI_2 * i as f32 / steps as f32;
            points.push(center + radius * egui::vec2(angle.cos(), angle.sin()));
        }
    };
    corner(rect.left_top() + egui::vec2(radius, radius), 2.0 * FRAC_PI_2);
    corner(rect.right_top() + egui::vec2(-radius, radius), 3.0 * FRAC_PI_2);
    if foot > 0.0 {
        let center = rect.right_bottom() + egui::vec2(foot, -foot);
        for i in 0..=12 {
            let a = FRAC_PI_2 * i as f32 / 12.0;
            points.push(egui::pos2(center.x - foot * a.cos(), center.y + foot * a.sin()));
        }
    } else {
        points.push(rect.right_bottom() - egui::vec2(0.0, INACTIVE_GAP));
    }
    points
}

/// The radius of the concave feet of a tab whose top corners have the radius `r` (variant `Joined`)
pub fn tab_foot(r: u8) -> f32 {
    f32::from(r).clamp(4.0, 8.0)
}

/// One outline around a pane and its active tab (variant `Joined`), the feet of the tab given by [`tab_foot`]
pub fn pane_outline(area: egui::Rect, tab: Option<(egui::Rect, u8)>, square_tl: bool, rr: f32) -> Vec<egui::Pos2> {
    pane_outline_with_foot(area, tab, square_tl, rr, None)
}

/// [`pane_outline`] with the radius of the feet of the tab given (`None`: [`tab_foot`] of its radius)
pub fn pane_outline_with_foot(area: egui::Rect, tab: Option<(egui::Rect, u8)>, square_tl: bool, rr: f32, foot: Option<f32>) -> Vec<egui::Pos2> {
    let tab_foot = |r: u8| foot.unwrap_or_else(|| tab_foot(r));
    use std::f32::consts::{FRAC_PI_2, PI};
    let arc = |pts: &mut Vec<egui::Pos2>, c: egui::Pos2, start: f32| {
        for i in 0..=8 {
            let a = start + FRAC_PI_2 * i as f32 / 8.0;
            pts.push(c + rr * egui::vec2(a.cos(), a.sin()));
        }
    };
    let mut pts = Vec::new();
    // a tab at the very left of the pane is flush with its side: no corner nor foot there
    let flush = tab.is_some_and(|(t, r)| t.min.x - tab_foot(r) < area.left() + if square_tl { 0.0 } else { rr });
    if square_tl {
        if !flush {
            pts.push(area.left_top());
        }
    } else {
        arc(&mut pts, area.left_top() + egui::vec2(rr, rr), PI);
    }
    if let Some((tab, r)) = tab {
        let all = tab_outline(tab, f32::from(r), tab_foot(r));
        let mut part = if flush { all[13..].to_vec() } else { all };
        // a tab scrolled out of the bar (its rectangle goes past a side of the pane) must not pull the
        // outline outside the pane: its points stay between the sides
        for p in &mut part {
            p.x = p.x.clamp(area.left(), area.right());
        }
        pts.extend(part);
    }
    arc(&mut pts, area.right_top() + egui::vec2(-rr, rr), 3.0 * FRAC_PI_2);
    arc(&mut pts, area.right_bottom() + egui::vec2(-rr, -rr), 0.0);
    arc(&mut pts, area.left_bottom() + egui::vec2(rr, -rr), FRAC_PI_2);
    pts
}

/// Variant `Separate`: the outline of a tab, open at the bottom, with no feet: its sides go straight down to
/// the bottom of the bar, where the outline of the pane ([`separate_pane_outline`]) takes them up
pub fn separate_tab_outline(rect: egui::Rect, radius: f32) -> Vec<egui::Pos2> {
    use std::f32::consts::FRAC_PI_2;

    let radius = radius.min(rect.width() / 2.0).min(rect.height());
    let steps = (radius.ceil() as usize).clamp(1, 8);
    let mut points = vec![rect.left_bottom()];
    let mut corner = |center: egui::Pos2, start: f32| {
        for i in 0..=steps {
            let angle = start + FRAC_PI_2 * i as f32 / steps as f32;
            points.push(center + radius * egui::vec2(angle.cos(), angle.sin()));
        }
    };
    // angles grow clockwise on screen, as y points down
    corner(rect.left_top() + egui::vec2(radius, radius), 2.0 * FRAC_PI_2);
    corner(rect.right_top() + egui::vec2(-radius, radius), 3.0 * FRAC_PI_2);
    points.push(rect.right_bottom());
    // the end of an arc is the start of the next one when the tab is as wide as two radii: a stroke of egui
    // doubles its width at a repeated point, which shows as a dot when the color is semi transparent
    dedup(points, false)
}

/// The top corners of a pane that stay square: those under a tab of its bar (active or not), so the tabs sit
/// on a straight edge, without a notch of background between a tab and a rounded corner
pub fn square_top_corners(tabs: &[egui::Rect], rect: egui::Rect, radius: f32) -> (bool, bool) {
    let mut square = (false, false);
    for tab in tabs {
        let on_top = tab.width() > 0.0 && tab.max.y <= rect.min.y + 0.5 && tab.max.y >= rect.min.y - 2.0;
        if !on_top || tab.right() <= rect.left() || tab.left() >= rect.right() {
            continue;
        }
        square.0 |= tab.left() - rect.left() < radius;
        square.1 |= rect.right() - tab.right() < radius;
    }
    square
}

/// Variant `Separate`: the outline of a pane, rounded corners, open at the top under its tab (`tab`), the
/// line half its `width` inside `rect` (concentric with the corners of the fill). A top corner touched by a
/// tab (`square`, see [`square_top_corners`]) stays square, so the sides of the tab continue into the pane.
pub fn separate_pane_outline(rect: egui::Rect, tab: Option<egui::Rect>, square: (bool, bool), radius: f32, width: f32) -> Vec<egui::Pos2> {
    use egui::pos2;
    use std::f32::consts::FRAC_PI_2;

    let inner = rect.shrink(width / 2.0);
    let radius = (radius - width / 2.0).min(inner.width() / 2.0).min(inner.height() / 2.0).max(0.0);
    let steps = (radius.ceil() as usize).clamp(1, 8);
    let mut points = vec![];

    // y points down, so the angles of the arcs decrease to go counterclockwise on screen
    let arc = |points: &mut Vec<egui::Pos2>, center: egui::Pos2, start: f32| {
        for i in 0..=steps {
            let angle = start - FRAC_PI_2 * i as f32 / steps as f32;
            points.push(center + radius * egui::vec2(angle.cos(), angle.sin()));
        }
    };

    // without a tab the outline is closed: it starts and ends in the middle of the top side
    let (gap_left, gap_right) = match tab {
        Some(tab) => (
            (tab.left() + width / 2.0).clamp(inner.left(), inner.right()),
            (tab.right() - width / 2.0).clamp(inner.left(), inner.right()),
        ),
        None => (inner.center().x, inner.center().x),
    };
    let round_left = !square.0 && gap_left - inner.left() >= radius;
    let round_right = !square.1 && inner.right() - gap_right >= radius;

    if tab.is_some() {
        // joins the bottom of the left side of the tab
        points.push(pos2(gap_left, rect.top() - width / 2.0));
    }
    points.push(pos2(gap_left, inner.top()));
    if round_left {
        arc(&mut points, inner.left_top() + egui::vec2(radius, radius), 3.0 * FRAC_PI_2);
    } else {
        points.push(inner.left_top());
    }
    arc(&mut points, inner.left_bottom() + egui::vec2(radius, -radius), 2.0 * FRAC_PI_2);
    arc(&mut points, inner.right_bottom() + egui::vec2(-radius, -radius), FRAC_PI_2);
    if round_right {
        arc(&mut points, inner.right_top() + egui::vec2(-radius, radius), 0.0);
    } else {
        points.push(inner.right_top());
    }
    points.push(pos2(gap_right, inner.top()));
    if tab.is_some() {
        // joins the bottom of the right side of the tab
        points.push(pos2(gap_right, rect.top() - width / 2.0));
    }
    dedup(points, false)
}

/// A fan of triangles from `apex` to an arc, whose edge on the arc is smoothed over one pixel (the colour
/// fades from full to nothing across the circle), like the edge of a rounded rectangle of egui: the
/// pieces that meet on a curve then blend with no dotted fringe. `point(radius, i)` is the point `i` of the
/// arc at that radius; `outside` is the side of the circle that is filled.
pub fn feathered_fan(apex: egui::Pos2, radius: f32, outside: bool, grow: f32, color: egui::Color32, point: impl Fn(f32, usize) -> egui::Pos2) -> egui::Mesh {
    const STEPS: usize = 8;
    let half = 0.5;
    // `grow` pushes the edge that far into the neighbour, which lies under it: the glass of the window does
    // not show through the seam where two smoothed edges meet
    let (full, none) = if outside { (radius + half - grow, radius - half - grow) } else { ((radius - half + grow).max(0.0), radius + half + grow) };
    let mut m = egui::Mesh::default();
    m.colored_vertex(apex, color);
    for i in 0..=STEPS {
        m.colored_vertex(point(full, i), color);
    }
    for i in 0..=STEPS {
        m.colored_vertex(point(none, i), egui::Color32::TRANSPARENT);
    }
    let (a, b) = (1u32, 2 + STEPS as u32);
    for i in 0..STEPS as u32 {
        m.add_triangle(0, a + i, a + i + 1);
        m.add_triangle(a + i, b + i, a + i + 1);
        m.add_triangle(a + i + 1, b + i, b + i + 1);
    }
    m
}

/// What the round corner of a sheet leaves out: a fan from the corner, `sx` and `sy` (+1 or -1) point inside
pub fn corner_mesh(outer: egui::Pos2, sx: f32, sy: f32, r: f32, tint: egui::Color32) -> egui::Mesh {
    let center = outer + egui::vec2(sx * r, sy * r);
    feathered_fan(outer, r, true, SEAM, tint, |rad, i| {
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / 8.0;
        egui::pos2(center.x - sx * rad * a.cos(), center.y - sy * rad * a.sin())
    })
}

/// How far (in points) the glass of the bands goes under the curves of the sheets
pub const SEAM: f32 = 0.35;


/// A rectangle on the pixels: the pieces of glass that meet leave no seam where an edge falls inside a pixel
pub fn snap(ui: &egui::Ui, r: egui::Rect) -> egui::Rect {
    let k = ui.ctx().pixels_per_point();
    let f = |v: f32| (v * k).round() / k;
    egui::Rect::from_min_max(egui::pos2(f(r.min.x), f(r.min.y)), egui::pos2(f(r.max.x), f(r.max.y)))
}

/// What is left of `r` once `hole` is taken out
pub fn minus(r: egui::Rect, hole: egui::Rect) -> Vec<egui::Rect> {
    if !r.intersects(hole) {
        return vec![r];
    }
    let h = r.intersect(hole);
    [
        egui::Rect::from_min_max(r.min, egui::pos2(r.max.x, h.min.y)),
        egui::Rect::from_min_max(egui::pos2(r.min.x, h.max.y), r.max),
        egui::Rect::from_min_max(egui::pos2(r.min.x, h.min.y), egui::pos2(h.min.x, h.max.y)),
        egui::Rect::from_min_max(egui::pos2(h.max.x, h.min.y), egui::pos2(r.max.x, h.max.y)),
    ]
    .into_iter()
    .filter(|p| p.width() > 0.0 && p.height() > 0.0)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, Rect};

    #[test]
    fn dedup_drops_repeated_points_and_the_closing_one() {
        let pts = vec![pos2(0.0, 0.0), pos2(0.0, 0.01), pos2(5.0, 0.0), pos2(5.0, 5.0), pos2(0.0, 0.0)];
        assert_eq!(dedup(pts.clone(), false).len(), 4);
        assert_eq!(dedup(pts, true).len(), 3);
    }

    #[test]
    fn a_pane_outline_stays_between_the_sides_of_the_pane() {
        let area = Rect::from_min_max(pos2(0.0, 40.0), pos2(300.0, 200.0));
        // a tab scrolled out of the bar, past both sides
        let tab = Rect::from_min_max(pos2(-80.0, 10.0), pos2(380.0, 40.0));
        let points = pane_outline(area, Some((tab, 8)), true, 8.0);
        assert!(points.iter().all(|p| p.x >= area.left() - 0.01 && p.x <= area.right() + 0.01));
    }

    #[test]
    fn a_separate_tab_outline_is_open_at_the_bottom_with_straight_sides() {
        let tab = Rect::from_min_max(pos2(10.0, 0.0), pos2(70.0, 24.0));
        let pts = separate_tab_outline(tab, 6.0);
        assert_eq!(pts[0], tab.left_bottom());
        assert_eq!(*pts.last().unwrap(), tab.right_bottom());
        assert!(pts.iter().all(|p| tab.expand(0.01).contains(*p)));
        // the top is reached between the two corners
        assert!(pts.iter().any(|p| (p.y - tab.top()).abs() < 0.01));
        // no repeated point
        assert!(pts.windows(2).all(|w| w[0].distance(w[1]) >= 0.05));
    }

    #[test]
    fn a_separate_pane_outline_joins_the_sides_of_its_tab() {
        let pane = Rect::from_min_max(pos2(0.0, 24.0), pos2(200.0, 150.0));
        let tab = Rect::from_min_max(pos2(20.0, 0.0), pos2(80.0, 24.0));
        let w = 2.0;
        let pts = separate_pane_outline(pane, Some(tab), (false, false), 8.0, w);
        // it starts and ends at the bottom of the sides of the tab outline (drawn on the tab shrunk by w/2)
        assert_eq!(pts[0], pos2(tab.left() + w / 2.0, pane.top() - w / 2.0));
        assert_eq!(*pts.last().unwrap(), pos2(tab.right() - w / 2.0, pane.top() - w / 2.0));
        // the line stays half its width inside the pane below the bar
        assert!(pts.iter().skip(1).take(pts.len() - 2).all(|p| pane.shrink(w / 2.0 - 0.01).contains(*p)));
        // closed (no tab): it starts and ends in the middle of the top side
        let closed = separate_pane_outline(pane, None, (false, false), 8.0, w);
        assert_eq!(closed[0].x, pane.center().x);
        assert_eq!(closed.last().unwrap().x, pane.center().x);
    }

    #[test]
    fn a_tab_at_the_left_squares_the_top_left_corner() {
        let pane = Rect::from_min_max(pos2(0.0, 24.0), pos2(200.0, 150.0));
        let tab = Rect::from_min_max(pos2(2.0, 0.0), pos2(60.0, 24.0));
        assert_eq!(square_top_corners(&[tab], pane, 8.0), (true, false));
        let far = Rect::from_min_max(pos2(40.0, 0.0), pos2(90.0, 24.0));
        assert_eq!(square_top_corners(&[far], pane, 8.0), (false, false));
    }

    #[test]
    fn the_foot_of_a_joined_pane_outline_can_be_given() {
        let area = Rect::from_min_max(pos2(0.0, 30.0), pos2(300.0, 200.0));
        let tab = Rect::from_min_max(pos2(50.0, 0.0), pos2(120.0, 30.0));
        let auto = pane_outline(area, Some((tab, 6)), true, 8.0);
        assert_eq!(auto, pane_outline_with_foot(area, Some((tab, 6)), true, 8.0, Some(tab_foot(6))));
        // a wider foot starts further left of the tab
        let wide = pane_outline_with_foot(area, Some((tab, 6)), true, 8.0, Some(12.0));
        let left = |pts: &[egui::Pos2]| pts.iter().filter(|p| (p.y - area.top()).abs() < 0.01).map(|p| p.x).filter(|x| *x > area.left() + 0.5 && *x < tab.left()).fold(f32::MAX, f32::min);
        assert!(left(&wide) < left(&auto));
    }
}
