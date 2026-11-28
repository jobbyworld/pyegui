//! The tree of the tags, the same in the sidebar and in the pane of the tags of both Finders: a tag per row with its
//! icon (or a dot) in its color, the chevron of a tree to fold what is under it (the `grid.*` tokens, like every tree
//! of the framework), the count of its items at the right, a trash in its place while the row is under the pointer
//! (a first click arms it, a second one deletes), an optional check box (to put a tag on the selected items), and a
//! drag to move a tag under another one, or before or after it.
//!
//! The rows are given flat, in the order of the tree, with their depth; the keys are the host's (a path of tags in
//! the Python extension, an id of the database in finder-rs). The widget keeps what it needs between the frames in a
//! [`TagTreeState`] and returns what was asked ([`TagTreeEvent`]); the host changes its data. The responses of the
//! rows are given back, for a context menu or a tooltip.
//!
//! ```ignore
//! let rows = vec![TagRow::new(1, "projet", 0, color).children(true).count(Some(12)), TagRow::new(2, "web", 1, color)];
//! let out = tag_tree::show(ui, &rows, selected.as_ref(), &mut state, &TagTreeOptions::default());
//! for event in out.events { match event { TagTreeEvent::Select(k) => ..., _ => ... } }
//! ```

use std::collections::HashSet;
use std::hash::Hash;

use egui::{Color32, Id, Rect, Response, Sense, Stroke, StrokeKind, TextStyle, Ui, vec2};

use crate::tags::paint_tag_icon;

const TRASH: &[u8] = include_bytes!("../../examples/icons/trash.svg");
const CHECK_ON: &[u8] = include_bytes!("../../examples/icons/checkbox-on.svg");
const CHECK_OFF: &[u8] = include_bytes!("../../examples/icons/checkbox-off.svg");
const CHECK_MIXED: &[u8] = include_bytes!("../../examples/icons/checkbox-mixed.svg");

/// The state of the check box of a row
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckState {
    #[default]
    Off,
    On,
    Mixed,
}

impl CheckState {
    pub fn parse(name: &str) -> CheckState {
        match name {
            "on" => CheckState::On,
            "mixed" => CheckState::Mixed,
            _ => CheckState::Off,
        }
    }

    fn glyph(self) -> (&'static str, &'static [u8]) {
        match self {
            CheckState::Off => ("checkbox-off", CHECK_OFF),
            CheckState::On => ("checkbox-on", CHECK_ON),
            CheckState::Mixed => ("checkbox-mixed", CHECK_MIXED),
        }
    }
}

/// One row of the tree
#[derive(Clone, Debug)]
pub struct TagRow<K> {
    pub key: K,
    /// the text of the row: the last level of the tag
    pub name: String,
    /// 0 for a tag at the top
    pub depth: usize,
    pub color: Color32,
    /// the row has rows under it: a chevron folds them
    pub has_children: bool,
    /// a number at the right (the items of the tag), hidden while the trash is shown
    pub count: Option<i64>,
    /// a check box before the icon, see [`TagTreeOptions::checks`]
    pub check: CheckState,
}

impl<K> TagRow<K> {
    pub fn new(key: K, name: impl Into<String>, depth: usize, color: Color32) -> Self {
        TagRow { key, name: name.into(), depth, color, has_children: false, count: None, check: CheckState::Off }
    }

    pub fn children(mut self, has_children: bool) -> Self {
        self.has_children = has_children;
        self
    }

    pub fn count(mut self, count: Option<i64>) -> Self {
        self.count = count;
        self
    }

    pub fn check(mut self, check: CheckState) -> Self {
        self.check = check;
        self
    }
}

/// What the tree keeps between the frames. Create it once; `folded` may be saved and restored.
#[derive(Clone, Debug)]
pub struct TagTreeState<K> {
    /// the rows folded shut: what is under them is not drawn
    pub folded: HashSet<K>,
    /// the row being moved with the mouse
    pub dragging: Option<K>,
    /// the row whose trash was clicked once: a second click deletes it
    pub armed: Option<K>,
}

impl<K> Default for TagTreeState<K> {
    fn default() -> Self {
        TagTreeState { folded: HashSet::new(), dragging: None, armed: None }
    }
}

/// The options of the tree. The colors that are None follow the theme.
#[derive(Clone, Debug)]
pub struct TagTreeOptions {
    /// the id of the tree (the rows and their trash and chevron are interacted under it)
    pub id: Id,
    /// a trash at the right of the row under the pointer
    pub deletable: bool,
    /// the rows can be dragged to move a tag
    pub movable: bool,
    /// the tag icon in the color of the tag; false draws a dot
    pub icon: bool,
    /// a check box per row, in the state of [`TagRow::check`]
    pub checks: bool,
    /// a double click on a row with children folds or opens it
    pub fold_on_double_click: bool,
    /// the height of a row
    pub row_height: f32,
    /// the indentation of a level
    pub indent: f32,
    /// the rounding of the fills of the rows (selected, hovered, being dragged); None: the rounding of the widgets
    pub rounding: Option<f32>,
    /// the fill of the selected row; None: the selection color of the theme
    pub selected_fill: Option<Color32>,
    /// the fill of a row under the pointer; None: the weak fill of the hovered widgets
    pub hover_fill: Option<Color32>,
    /// the color of the check boxes that are on and of the line where a tag will be dropped; None: the selection
    pub accent: Option<Color32>,
    /// the color of an armed trash; None: the error color of the theme
    pub danger: Option<Color32>,
    /// the tooltip of the trash, and of the trash once armed
    pub delete_tip: String,
    pub armed_tip: String,
}

impl Default for TagTreeOptions {
    fn default() -> Self {
        TagTreeOptions {
            id: Id::unique("tag_tree"),
            deletable: false,
            movable: false,
            icon: true,
            checks: false,
            fold_on_double_click: false,
            row_height: 26.0,
            indent: 14.0,
            rounding: None,
            selected_fill: None,
            hover_fill: None,
            accent: None,
            danger: None,
            delete_tip: "Supprimer l'étiquette".into(),
            armed_tip: "Cliquer encore pour supprimer".into(),
        }
    }
}

/// What was asked on the tree during the frame; the folds are already in the state
#[derive(Clone, Debug, PartialEq)]
pub enum TagTreeEvent<K> {
    /// a click on a row (not on its chevron nor on its trash); with check boxes, the host puts the tag on or off
    Select(K),
    /// a row was folded or opened (already done in the state)
    Toggle(K),
    /// the second click on the trash of a row
    Delete(K),
    /// a row was dropped: it goes under `parent` (None: at the top), before `before` (None: last)
    Move { key: K, parent: Option<K>, before: Option<K> },
}

/// What [`show`] gives back
pub struct TagTreeOutput<K> {
    pub events: Vec<TagTreeEvent<K>>,
    /// the response of each row drawn, for a context menu or a tooltip
    pub rows: Vec<(K, Response)>,
    /// the rectangle of the rows
    pub rect: Rect,
}

/// The rows that are seen: the ones under a folded row are left out
pub fn visible_rows<'a, K: Eq + Hash>(rows: &'a [TagRow<K>], state: &TagTreeState<K>) -> Vec<(usize, &'a TagRow<K>)> {
    let mut out = Vec::with_capacity(rows.len());
    let mut hidden_below: Option<usize> = None;
    for (index, row) in rows.iter().enumerate() {
        if let Some(depth) = hidden_below {
            if row.depth > depth {
                continue;
            }
            hidden_below = None;
        }
        if row.has_children && state.folded.contains(&row.key) {
            hidden_below = Some(row.depth);
        }
        out.push((index, row));
    }
    out
}

/// The parent of a row: the row before it one level up
pub fn parent_of<K: Clone>(rows: &[TagRow<K>], index: usize) -> Option<K> {
    let depth = rows[index].depth;
    if depth == 0 {
        return None;
    }
    rows[..index].iter().rev().find(|r| r.depth < depth).map(|r| r.key.clone())
}

/// The next row of the same parent after a row and what is under it
fn next_sibling<K: Clone>(rows: &[TagRow<K>], index: usize) -> Option<K> {
    let depth = rows[index].depth;
    let stop = rows[index + 1..].iter().position(|r| r.depth <= depth).map_or(rows.len(), |n| index + 1 + n);
    rows.get(stop).filter(|r| r.depth == depth).map(|r| r.key.clone())
}

fn glyph(name: &str, bytes: &'static [u8], size: f32, tint: Color32) -> egui::Image<'static> {
    egui::Image::from_bytes(format!("bytes://glyph/{name}.svg"), bytes).tint(tint).fit_to_exact_size(vec2(size, size))
}

/// Draws the tree: the rows not hidden by a folded row, then the line where a row being moved will go
pub fn show<K: Clone + Eq + Hash + std::fmt::Debug + Send + Sync + 'static>(
    ui: &mut Ui,
    rows: &[TagRow<K>],
    selected: Option<&K>,
    state: &mut TagTreeState<K>,
    o: &TagTreeOptions,
) -> TagTreeOutput<K> {
    let mut events = Vec::new();
    let mut responses = Vec::new();
    // the rows drawn: (index in `rows`, rectangle), where a row being moved can be dropped
    let mut drawn: Vec<(usize, Rect)> = Vec::new();
    let look = crate::grid_style::look(ui.visuals());
    let rounding = o.rounding.unwrap_or_else(|| f32::from(ui.visuals().widgets.inactive.corner_radius.nw));
    let selected_fill = o.selected_fill.unwrap_or(ui.visuals().selection.bg_fill);
    let hover_fill = o.hover_fill.unwrap_or(ui.visuals().widgets.hovered.weak_bg_fill);
    let accent = o.accent.unwrap_or(ui.visuals().selection.bg_fill);
    let danger = o.danger.unwrap_or(ui.visuals().error_fg_color);
    let top = ui.cursor().top();
    let mut disarm = false;

    for (index, row) in visible_rows(rows, state) {
        let key = &row.key;
        let id = o.id.with(("row", key));
        let sense = if o.movable { Sense::click_and_drag() } else { Sense::click() };
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), o.row_height), Sense::hover());
        let response = ui.interact(rect, id, sense);
        drawn.push((index, rect));
        if o.movable && response.drag_started() {
            state.dragging = Some(key.clone());
        }
        let folded = state.folded.contains(key);
        let x = rect.left() + 6.0 + row.depth as f32 * o.indent;
        let pre = if o.checks { 20.0 } else { 0.0 };
        let text_color = ui.visuals().text_color();

        // the chevron: its own click folds, the rest of the row selects
        let chevron = row.has_children.then(|| {
            let hit = Rect::from_center_size(egui::pos2(x + 7.0, rect.center().y), vec2(20.0, o.row_height.min(24.0)));
            ui.interact(hit, id.with("chevron"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand)
        });
        // the trash, in place of the count: seen while the row is under the pointer (and no row is moved), or armed
        let armed = state.armed.as_ref() == Some(key);
        let shown_trash = o.deletable && ((ui.rect_contains_pointer(rect) && state.dragging.is_none()) || armed);
        let trash_rect = Rect::from_center_size(egui::pos2(rect.right() - 16.0, rect.center().y), vec2(24.0, 22.0));
        let trash = shown_trash.then(|| ui.interact(trash_rect, id.with("trash"), Sense::click()));
        let over_trash = trash.as_ref().is_some_and(Response::hovered);

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            if selected == Some(key) {
                painter.rect_filled(rect, rounding, selected_fill);
            } else if response.hovered() {
                painter.rect_filled(rect, rounding, hover_fill);
            }
            if let Some(chevron) = &chevron {
                crate::grid::draw_chevron(painter, chevron.rect.center(), !folded, &look, chevron.hovered());
            }
            if o.checks {
                let tint = if row.check == CheckState::Off { text_color } else { accent };
                let (name, bytes) = row.check.glyph();
                glyph(name, bytes, 16.0, tint).paint_at(ui, Rect::from_center_size(egui::pos2(x + 24.0, rect.center().y), vec2(16.0, 16.0)));
            }
            let icon_center = egui::pos2(x + 20.0 + pre, rect.center().y);
            if o.icon {
                paint_tag_icon(ui, icon_center, 14.0, row.color);
            } else {
                painter.circle_filled(icon_center, 4.5, row.color);
            }
            // the count (or the trash) at the right; the name is cut where it begins
            let mut right = rect.right() - 8.0;
            if let Some(count) = row.count.filter(|_| !shown_trash) {
                let weak = ui.visuals().weak_text_color();
                let galley = painter.layout_no_wrap(count.to_string(), TextStyle::Small.resolve(ui.style()), weak);
                let at = egui::pos2(rect.right() - 10.0 - galley.size().x, rect.center().y - galley.size().y / 2.0);
                right = at.x - 6.0;
                painter.galley(at, galley, weak);
            }
            if shown_trash {
                right = trash_rect.left() - 4.0;
            }
            let galley = painter.layout_no_wrap(row.name.clone(), TextStyle::Body.resolve(ui.style()), text_color);
            let at = egui::pos2(x + 34.0 + pre, rect.center().y - galley.size().y / 2.0);
            painter.with_clip_rect(Rect::from_min_max(rect.min, egui::pos2(right, rect.max.y)).intersect(ui.clip_rect())).galley(at, galley, text_color);
            if state.dragging.as_ref() == Some(key) {
                // the row being moved is faded
                painter.rect_filled(rect, rounding, ui.visuals().panel_fill.gamma_multiply(0.6));
            }
            if let Some(trash) = &trash {
                let tint = if armed { danger } else { ui.visuals().weak_text_color() };
                if armed || over_trash {
                    painter.rect_filled(trash_rect, rounding, if armed { danger.gamma_multiply(0.14) } else { hover_fill });
                }
                glyph("trash", TRASH, 14.0, tint).paint_at(ui, Rect::from_center_size(trash.rect.center(), vec2(14.0, 14.0)));
            }
        }
        if let Some(trash) = trash {
            let trash = trash.on_hover_text(if armed { &o.armed_tip } else { &o.delete_tip }).on_hover_cursor(egui::CursorIcon::PointingHand);
            if trash.clicked() {
                if armed {
                    events.push(TagTreeEvent::Delete(key.clone()));
                    disarm = true;
                } else {
                    state.armed = Some(key.clone());
                }
            }
        }
        if armed && !over_trash {
            disarm = true;
        }
        let chevron_clicked = chevron.as_ref().is_some_and(Response::clicked);
        if chevron_clicked || (o.fold_on_double_click && row.has_children && response.double_clicked()) {
            if !state.folded.remove(key) {
                state.folded.insert(key.clone());
            }
            events.push(TagTreeEvent::Toggle(key.clone()));
        } else if response.clicked() && !over_trash {
            events.push(TagTreeEvent::Select(key.clone()));
        }
        responses.push((key.clone(), response));
    }
    if disarm {
        state.armed = None;
    }
    let bottom = drawn.last().map_or(top, |(_, r)| r.bottom());
    let rect = Rect::from_min_max(egui::pos2(ui.max_rect().left(), top), egui::pos2(ui.max_rect().right(), bottom));
    if let Some(event) = dragging(ui, rows, &drawn, state, o, accent, rounding) {
        events.push(event);
    }
    TagTreeOutput { events, rows: responses, rect }
}

/// Moving a row with the mouse: dropped on the middle of another one it goes under it, on its upper or lower edge it
/// goes before or after it (same level), below the last row it goes to the top, last. Draws where it goes, and
/// returns the move once the button is let go.
fn dragging<K: Clone + Eq + Hash>(
    ui: &mut Ui,
    rows: &[TagRow<K>],
    drawn: &[(usize, Rect)],
    state: &mut TagTreeState<K>,
    o: &TagTreeOptions,
    accent: Color32,
    rounding: f32,
) -> Option<TagTreeEvent<K>> {
    let moving = state.dragging.clone()?;
    let ctx = ui.ctx().clone();
    let Some(from) = rows.iter().position(|r| r.key == moving) else {
        state.dragging = None;
        return None;
    };
    // the row and what is under it cannot receive it
    let end = rows[from + 1..].iter().position(|r| r.depth <= rows[from].depth).map_or(rows.len(), |n| from + 1 + n);
    let inside = |i: usize| i >= from && i < end;
    enum Drop {
        Into(usize),
        Before(usize),
        After(usize),
        TopEnd,
    }
    let pointer = ctx.pointer_latest_pos();
    let mut drop = None;
    if let Some(p) = pointer.filter(|p| ui.max_rect().x_range().contains(p.x)) {
        if let Some((i, rect)) = drawn.iter().find(|(_, r)| r.y_range().contains(p.y)) {
            if !inside(*i) {
                let f = (p.y - rect.top()) / rect.height();
                drop = Some(if f < 0.25 { Drop::Before(*i) } else if f > 0.75 { Drop::After(*i) } else { Drop::Into(*i) });
            }
        } else if drawn.last().is_some_and(|(_, r)| p.y > r.bottom() && p.y < r.bottom() + 60.0) {
            drop = Some(Drop::TopEnd);
        }
    }
    if ctx.input(|i| i.pointer.primary_down()) {
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        let stroke = Stroke::new(2.0, accent);
        let row = |i: usize| drawn.iter().find(|(k, _)| *k == i).map(|(_, r)| *r);
        let indent = |i: usize| 6.0 + rows[i].depth as f32 * o.indent;
        let painter = ui.painter();
        match &drop {
            Some(Drop::Into(i)) => {
                if let Some(r) = row(*i) {
                    painter.rect_stroke(r, rounding, stroke, StrokeKind::Inside);
                }
            }
            Some(Drop::Before(i)) => {
                if let Some(r) = row(*i) {
                    painter.line_segment([egui::pos2(r.left() + indent(*i), r.top()), egui::pos2(r.right() - 6.0, r.top())], stroke);
                }
            }
            Some(Drop::After(i)) => {
                if let Some(r) = row(*i) {
                    painter.line_segment([egui::pos2(r.left() + indent(*i), r.bottom()), egui::pos2(r.right() - 6.0, r.bottom())], stroke);
                }
            }
            Some(Drop::TopEnd) => {
                if let Some((_, r)) = drawn.last() {
                    painter.line_segment([egui::pos2(r.left() + 6.0, r.bottom() + 2.0), egui::pos2(r.right() - 6.0, r.bottom() + 2.0)], stroke);
                }
            }
            None => {}
        }
        if let Some(p) = pointer {
            // the name follows the pointer
            let layer = egui::LayerId::new(egui::Order::Tooltip, o.id.with("ghost"));
            ctx.layer_painter(layer).text(p + vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, &rows[from].name, TextStyle::Body.resolve(ui.style()), ui.visuals().text_color());
        }
        return None;
    }
    // let go: the row goes where the line was
    state.dragging = None;
    let (parent, before) = match drop? {
        Drop::Into(i) => (Some(rows[i].key.clone()), None),
        Drop::Before(i) => (parent_of(rows, i), Some(rows[i].key.clone())),
        Drop::After(i) => (parent_of(rows, i), next_sibling(rows, i)),
        Drop::TopEnd => (None, None),
    };
    // dropped where it already is: nothing to do
    if parent == parent_of(rows, from) && before.as_ref() == next_sibling(rows, from).as_ref() {
        return None;
    }
    Some(TagTreeEvent::Move { key: moving, parent, before })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<TagRow<&'static str>> {
        let c = Color32::RED;
        vec![
            TagRow::new("projet", "projet", 0, c).children(true),
            TagRow::new("projet/web", "web", 1, c).children(true),
            TagRow::new("projet/web/css", "css", 2, c),
            TagRow::new("projet/app", "app", 1, c),
            TagRow::new("perso", "perso", 0, c),
        ]
    }

    #[test]
    fn a_folded_row_hides_what_is_under_it() {
        let rows = rows();
        let mut state = TagTreeState::default();
        assert_eq!(visible_rows(&rows, &state).len(), 5);
        state.folded.insert("projet/web");
        let seen: Vec<_> = visible_rows(&rows, &state).into_iter().map(|(_, r)| r.key).collect();
        assert_eq!(seen, ["projet", "projet/web", "projet/app", "perso"]);
        state.folded.insert("projet");
        assert_eq!(visible_rows(&rows, &state).len(), 2);
        // a leaf that is in the folded set hides nothing
        state.folded.clear();
        state.folded.insert("projet/app");
        assert_eq!(visible_rows(&rows, &state).len(), 5);
    }

    #[test]
    fn parents_and_siblings_come_from_the_depths() {
        let rows = rows();
        assert_eq!(parent_of(&rows, 0), None);
        assert_eq!(parent_of(&rows, 2), Some("projet/web"));
        assert_eq!(parent_of(&rows, 3), Some("projet"));
        assert_eq!(next_sibling(&rows, 1), Some("projet/app"));
        assert_eq!(next_sibling(&rows, 0), Some("perso"));
        assert_eq!(next_sibling(&rows, 3), None);
    }

    #[test]
    fn the_rows_are_drawn_and_given_back() {
        let ctx = egui::Context::default();
        let rows = rows();
        let mut state = TagTreeState::default();
        state.folded.insert("projet/web");
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let out = show(ui, &rows, Some(&"perso"), &mut state, &TagTreeOptions { checks: true, deletable: true, ..Default::default() });
            assert_eq!(out.rows.len(), 4);
            assert!(out.events.is_empty());
            assert_eq!(out.rect.height(), out.rows.last().unwrap().1.rect.bottom() - out.rows[0].1.rect.top());
        });
        output.textures_delta.clear();
    }
}
