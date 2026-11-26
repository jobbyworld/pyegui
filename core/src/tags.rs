//! Tags: a list of tags with several levels ("projet/web"), each level with its own color.
//!
//! `Tags` is the state (the tags and the colors), `tags_edit` draws it: a chip per tag showing its
//! last level, its full path in the colors of its levels on hover, a field to add one. Shared by the
//! Python extension (`Tags`, `tags_edit`) and the Rust applications.

use std::collections::{HashMap, HashSet};

use egui::{self, vec2, Color32, Id, Rect, Sense, Stroke, StrokeKind, TextStyle};

/// The colors given to the levels that have none, chosen from the path so they stay the same
pub const PALETTE: [u32; 10] = [
    0x3b82f6, 0x10b981, 0xf97316, 0x8b5cf6, 0xef4444, 0x14b8a6, 0xeab308, 0xec4899, 0x6366f1, 0x84cc16,
];

pub fn hex(value: u32) -> Color32 {
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

/// A color with most of its saturation taken away: the color of a muted tag (e.g. inherited)
pub fn muted(color: Color32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let gray = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    let mix = |c: u8| (c as f32 + (gray - c as f32) * 0.85).round() as u8;
    Color32::from_rgba_unmultiplied(mix(r), mix(g), mix(b), a)
}

pub fn to_hex(color: Color32) -> String {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// "a / b//c " -> "a/b/c": the levels without the spaces around them nor the empty ones
pub fn normalize(tag: &str) -> String {
    tag.split('/').map(str::trim).filter(|level| !level.is_empty()).collect::<Vec<_>>().join("/")
}

/// The paths of the levels of a tag: "a/b/c" -> ["a", "a/b", "a/b/c"]
pub fn level_paths(tag: &str) -> Vec<String> {
    let mut paths = vec![];
    for (index, _) in tag.match_indices('/') {
        paths.push(tag[..index].to_owned());
    }
    paths.push(tag.to_owned());
    paths
}

pub fn last_level(tag: &str) -> &str {
    tag.rsplit('/').next().unwrap_or(tag)
}

/// A list of tags and their colors
#[derive(Default, Clone)]
pub struct Tags {
    pub tags: Vec<String>,
    pub colors: HashMap<String, Color32>,
    /// what is typed in the field of the widget
    pub input: String,
}

impl Tags {
    pub fn new(tags: Vec<String>, colors: HashMap<String, Color32>) -> Self {
        let mut state = Tags { tags: vec![], colors, input: String::new() };
        for tag in tags {
            state.insert(&tag);
        }
        state
    }

    pub fn color_of(&self, path: &str) -> Color32 {
        self.colors.get(path).copied().unwrap_or_else(|| {
            let hash = path.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32);
            hex(PALETTE[hash as usize % PALETTE.len()])
        })
    }

    pub fn insert(&mut self, tag: &str) -> bool {
        let tag = normalize(tag);
        if tag.is_empty() || self.tags.contains(&tag) {
            return false;
        }
        self.tags.push(tag);
        true
    }

    /// Puts the tag on, or takes it off when it is there (the pick mode of the dropdown)
    pub fn toggle(&mut self, tag: &str) -> bool {
        let tag = normalize(tag);
        if self.tags.contains(&tag) {
            self.tags.retain(|t| *t != tag);
            true
        } else {
            self.insert(&tag)
        }
    }
}

/// The full path of a tag, each level in its color, separated by weak slashes
pub fn path_ui(ui: &mut egui::Ui, tags: &Tags, tag: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let levels = level_paths(tag);
        for (index, path) in levels.iter().enumerate() {
            if index > 0 {
                ui.weak("/");
            }
            let color = tags.color_of(path);
            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.0, color);
            ui.label(egui::RichText::new(last_level(path)).color(color));
        }
    });
}

/// The name of a level and a row of colors to pick from, "Auto" to go back to the one chosen from
/// its path. Some(the color picked) when one was clicked.
pub fn color_row(ui: &mut egui::Ui, tags: &Tags, path: &str) -> Option<Option<Color32>> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.label(last_level(path));
        for value in PALETTE {
            let swatch = hex(value);
            let (spot, response) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
            ui.painter().circle_filled(spot.center(), if response.hovered() { 7.5 } else { 6.5 }, swatch);
            if tags.color_of(path) == swatch {
                ui.painter().circle_stroke(spot.center(), 8.0, Stroke::new(1.5, ui.visuals().text_color()));
            }
            if response.clicked() {
                picked = Some(Some(swatch));
            }
        }
        if tags.colors.contains_key(path) && ui.small_button("Auto").clicked() {
            picked = Some(None);
        }
    });
    picked
}

/// The text of the chips: Body, or Small for the small ones (e.g. in a row of a list)
fn chip_style(small: bool) -> TextStyle {
    if small {
        TextStyle::Small
    } else {
        TextStyle::Body
    }
}

/// The background of the chips, of the empty tag and of the list: the one of a window pulled a
/// little toward the text, a grey that is not as black as the window in the dark theme
pub fn soft_fill(ui: &egui::Ui) -> Color32 {
    let visuals = ui.visuals();
    // lighter in the light theme, where the same amount of text color would look dirty
    let amount = if visuals.dark_mode { 0.14 } else { 0.03 };
    visuals.window_fill.to_opaque().lerp_to_gamma(visuals.text_color(), amount)
}

/// What can be done to a tag from its chip
enum ChipAction {
    Remove,
    Color(String, Option<Color32>),
}

/// The padding of a chip and the size of what is at the left of its text: a dot, or the tag icon
fn chip_dot(small: bool, icon: bool) -> (f32, f32) {
    match (small, icon) {
        (true, false) => (6.0, 6.0),
        (false, false) => (8.0, 8.0),
        (true, true) => (6.0, 11.0),
        (false, true) => (8.0, 13.0),
    }
}

/// The SVG of the tag icon (white: it is tinted)
pub const TAG_ICON: &[u8] = include_bytes!("../../examples/icons/tag.svg");

/// Paints the tag icon of a color
pub fn paint_tag_icon(ui: &egui::Ui, center: egui::Pos2, size: f32, color: Color32) {
    egui::Image::from_bytes("bytes://pyegui-tag-icon.svg", TAG_ICON)
        .tint(color)
        .fit_to_exact_size(vec2(size, size))
        .paint_at(ui, Rect::from_center_size(center, vec2(size, size)));
}

/// The width of the chip of a tag, with the room of its cross when it can be removed
fn chip_width(ui: &egui::Ui, tag: &str, removable: bool, small: bool, icon: bool) -> f32 {
    let font = chip_style(small).resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(last_level(tag).to_owned(), font, Color32::WHITE);
    let height = galley.size().y + if small { 2.0 } else { 6.0 };
    let (pad, dot) = chip_dot(small, icon);
    let cross = if removable { height.min(14.0) } else { 0.0 };
    pad + dot + 6.0 + galley.size().x + cross + pad
}

/// The chip of a tag: a dot and the last level in a pill of its color, a cross to remove it
#[allow(clippy::too_many_arguments)]
fn chip(
    ui: &mut egui::Ui,
    tags: &Tags,
    tag: &str,
    editable: bool,
    small: bool,
    dim: bool,
    icon: bool,
    id: Id,
) -> Option<ChipAction> {
    // faded: less color, and the color pulled toward the background of the chips
    let color = if dim { muted(tags.color_of(tag)).lerp_to_gamma(soft_fill(ui), 0.4) } else { tags.color_of(tag) };
    let font = chip_style(small).resolve(ui.style());
    // the text and the cross in the color of the outline
    // the text and the cross: the color pulled toward the text of the theme, so it reads on the
    // tint of the chip in the light and in the dark theme; the outline keeps the color
    let edge = color.lerp_to_gamma(ui.visuals().strong_text_color(), 0.45);
    let galley = ui.painter().layout_no_wrap(last_level(tag).to_owned(), font, edge);
    let height = galley.size().y + if small { 2.0 } else { 6.0 };
    let (pad, dot) = chip_dot(small, icon);
    let cross = if editable { height.min(14.0) } else { 0.0 };
    let width = pad + dot + 6.0 + galley.size().x + cross + pad;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let cross_rect =
        Rect::from_center_size(egui::pos2(rect.right() - pad - cross / 2.0 + 2.0, rect.center().y), vec2(14.0, 14.0));
    let cross_response = editable.then(|| ui.interact(cross_rect, id.with("remove"), Sense::click()));

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let hovered = response.hovered();
        // opaque: the tint of the color over the background of the window, so a chip stays readable
        // over a selected row
        let base = soft_fill(ui);
        let fill = base.lerp_to_gamma(color, if hovered { 0.28 } else { 0.18 });
        painter.rect(rect, height / 2.0, fill, Stroke::new(1.2, color), StrokeKind::Inside);
        let dot_center = egui::pos2(rect.left() + pad + dot / 2.0, rect.center().y);
        if icon {
            paint_tag_icon(ui, dot_center, dot, color);
        } else {
            painter.circle_filled(dot_center, dot / 2.0, color);
        }
        let text_pos = egui::pos2(dot_center.x + dot / 2.0 + 6.0, rect.center().y - galley.size().y / 2.0);
        painter.galley(text_pos, galley, edge);
        if let Some(cross) = &cross_response {
            let c = cross_rect.center();
            let r = 3.5;
            let stroke_color = if cross.hovered() { color } else { edge };
            if cross.hovered() {
                painter.circle_filled(c, 7.0, color.gamma_multiply(0.35));
            }
            let stroke = Stroke::new(1.4, stroke_color);
            painter.line_segment([c + vec2(-r, -r), c + vec2(r, r)], stroke);
            painter.line_segment([c + vec2(-r, r), c + vec2(r, -r)], stroke);
        }
    }

    let mut action = None;
    if cross_response.as_ref().is_some_and(|cross| cross.clicked()) {
        action = Some(ChipAction::Remove);
    }
    let response = response.on_hover_ui(|ui| path_ui(ui, tags, tag));
    if editable {
        response.context_menu(|ui| {
            // a row of colors for each level of the tag: its own one changes only that level
            for path in level_paths(tag) {
                if let Some(color) = color_row(ui, tags, &path) {
                    action = Some(ChipAction::Color(path, color));
                }
            }
            ui.separator();
            if ui.button("Remove").clicked() {
                action = Some(ChipAction::Remove);
                ui.close();
            }
        });
    }
    action
}

/// The badge of the tags that are not shown, "+5": the mouse over it lists them with their full
/// paths, a click shows them all. With expanded, a "−" that shows the first ones again.
fn more_badge(ui: &mut egui::Ui, tags: &Tags, hidden: &[String], expanded: bool, small: bool, id: Id) {
    let text = if expanded { "−".to_owned() } else { format!("+{}", hidden.len()) };
    let font = chip_style(small).resolve(ui.style());
    let height = ui.text_style_height(&chip_style(small)) + if small { 2.0 } else { 6.0 };
    let weak = ui.visuals().weak_text_color();
    let galley = ui.painter().layout_no_wrap(text, font, weak);
    let size = vec2((galley.size().x + 16.0).max(height), height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if response.hovered() {
            ui.visuals().widgets.hovered.weak_bg_fill
        } else {
            ui.visuals().widgets.inactive.weak_bg_fill
        };
        ui.painter().rect(rect, height / 2.0, fill, Stroke::new(1.0, weak.gamma_multiply(0.4)), StrokeKind::Inside);
        ui.painter().galley(rect.center() - galley.size() / 2.0, galley, weak);
    }
    let response = if expanded {
        response.on_hover_text("Show the first ones only")
    } else {
        response.on_hover_ui(|ui| {
            for tag in hidden {
                path_ui(ui, tags, tag);
            }
        })
    };
    if response.clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(id.with("expanded"), !expanded));
    }
}

/// What `tags_edit` draws and how; see the keyword args of `tags_edit` in Python
pub struct TagsOptions {
    pub editable: bool,
    pub hint_text: String,
    pub suggestions: Vec<String>,
    pub field_width: f32,
    /// tells apart two widgets of the same tags
    pub id: Id,
    pub max_visible: Option<usize>,
    pub small: bool,
    pub muted: HashSet<String>,
    pub on_hover: bool,
    pub dropdown: bool,
    /// the preferred (maximum) height of the list of suggestions, which scrolls beyond it
    pub list_height: f32,
    /// the dropdown lists all the suggestions with a check box, the ones on are checked; a click toggles one
    /// and the list stays open, to put several
    pub pick: bool,
    pub fit: bool,
    pub compact: bool,
    pub dot_size: f32,
    /// the tag icon in the color of the tag at the left of a chip, instead of a dot
    pub icon: bool,
    /// a flat cell: the tags as chips with no field, a click opens the tree of `suggestions` with a check box for
    /// each level (a field filters and creates), see `tags_pick`
    pub tree: bool,
}

impl TagsOptions {
    pub fn new(id: Id) -> Self {
        TagsOptions {
            editable: true,
            hint_text: "Add a tag".to_owned(),
            suggestions: vec![],
            field_width: 120.0,
            id,
            max_visible: None,
            small: false,
            muted: HashSet::new(),
            on_hover: false,
            dropdown: false,
            list_height: 220.0,
            pick: false,
            fit: false,
            compact: false,
            dot_size: 10.0,
            icon: false,
            tree: false,
        }
    }
}

/// Shows tags as chips, each one with its last level, its full path in the colors of its levels on
/// hover. With `editable` a cross removes a tag, a right click changes the color of each of its
/// levels, and a field adds one. Returns true when a tag was added or removed, or a color changed.
pub fn tags_edit(ui: &mut egui::Ui, tags: &mut Tags, o: &TagsOptions) -> bool {
    if o.tree {
        return tags_pick(ui, tags, o);
    }
    let editable = o.editable;
    let hint = o.hint_text.clone();
    let suggestions = &o.suggestions;
    let field_width = o.field_width;
    let id = o.id;
    let small = o.small;
    let muted_tags: HashSet<String> = o.muted.iter().map(|t| normalize(t)).collect();
    let on_hover = o.on_hover;
    let dropdown = o.dropdown;
    let list_height = o.list_height;
    let pick = o.pick;
    let field_id = id.with("field");
    let editing = editable
        && (!on_hover
            || ui.rect_contains_pointer(ui.max_rect().intersect(ui.clip_rect()))
            || ui.ctx().memory(|m| m.has_focus(field_id)));
    let expanded = ui.ctx().data(|d| d.get_temp::<bool>(id.with("expanded"))).unwrap_or(false);
    let count = tags.tags.len();
    // fit: as many chips as the room allows, the others in the "+N" badge. The room of the empty
    // tag and of the crosses is kept even when they do not show, so the chips do not come and go
    // with the mouse
    let fit = o.fit;
    let max_visible = if fit && !o.compact {
        let state = &*tags;
        let reserved = if editable { field_width + 16.0 + 6.0 } else { 0.0 };
        let room = (ui.available_width() - reserved).max(0.0);
        let (mut used, mut fitting) = (0.0, 0);
        for (i, tag) in state.tags.iter().enumerate() {
            let width = chip_width(ui, tag, editable, small, o.icon) + 6.0;
            let badge = if i + 1 < count { 40.0 } else { 0.0 };
            if used + width + badge > room {
                break;
            }
            used += width;
            fitting += 1;
        }
        Some(fitting.max(1))
    } else {
        o.max_visible
    };
    let shown = match max_visible {
        Some(max) if count > max && !expanded => max,
        _ => count,
    };
    let with_badge = max_visible.is_some_and(|max| count > max);

    if o.compact {
        let size = o.dot_size.max(2.0);
        let state = &*tags;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(size * 0.4, size * 0.4);
            for tag in &state.tags[..shown] {
                let (spot, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                let color = state.color_of(tag);
                let color = if muted_tags.contains(tag) { muted(color) } else { color };
                ui.painter().circle_filled(spot.center(), size / 2.0, color);
                if response.hovered() {
                    ui.painter().circle_stroke(spot.center(), size / 2.0 + 1.5, Stroke::new(1.0, color.gamma_multiply(0.6)));
                }
                response.on_hover_ui(|ui| path_ui(ui, state, tag));
            }
            if with_badge {
                more_badge(ui, state, &state.tags[shown..], expanded, true, id);
            }
        });
        return false;
    }

    let state = tags;
    let mut changed = false;
    // what is typed: with on_hover it is kept by the widget, so each row of a list has its own
    let input_id = id.with("input");
    let mut input = if on_hover {
        ui.ctx().data_mut(|d| d.get_temp::<String>(input_id)).unwrap_or_default()
    } else {
        std::mem::take(&mut state.input)
    };

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        // what the chips ask for, applied once they are drawn
        let actions: Vec<(String, ChipAction)> = state.tags[..shown]
            .iter()
            .filter_map(|tag| {
                chip(ui, &state, tag, editing && !muted_tags.contains(tag), small, muted_tags.contains(tag), o.icon, id.with(tag))
                    .map(|action| (tag.clone(), action))
            })
            .collect();
        if with_badge {
            more_badge(ui, &state, &state.tags[shown..], expanded, small, id);
        }
        for (tag, action) in actions {
            match action {
                ChipAction::Remove => state.tags.retain(|t| *t != tag),
                ChipAction::Color(path, Some(color)) => {
                    state.colors.insert(path, color);
                }
                ChipAction::Color(path, None) => {
                    state.colors.remove(&path);
                }
            }
            changed = true;
        }
        if !editing {
            return;
        }

        // the keyboard in the list: arrows move the highlighted row, Enter picks it, Escape clears
        // what is typed. The list is the one of the last frame (it is drawn after the field)
        let highlight_id = id.with("highlight");
        let count_id = id.with("count");
        let listed = ui.ctx().data(|d| d.get_temp::<usize>(count_id)).unwrap_or(0);
        let mut highlight = ui.ctx().data(|d| d.get_temp::<Option<usize>>(highlight_id)).flatten();
        let mut moved = false;
        if listed > 0 && ui.ctx().memory(|m| m.has_focus(field_id)) {
            let none = egui::Modifiers::NONE;
            if ui.input_mut(|i| i.consume_key(none, egui::Key::ArrowDown)) {
                highlight = Some(highlight.map_or(0, |h| (h + 1).min(listed - 1)));
                moved = true;
            }
            if ui.input_mut(|i| i.consume_key(none, egui::Key::ArrowUp)) {
                highlight = Some(highlight.map_or(listed - 1, |h| h.saturating_sub(1)));
                moved = true;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                input.clear();
                highlight = None;
            }
        }
        let typed_before = input.clone();
        let field = if on_hover {
            // the empty tag: a pill like the chips, with the field in it and a chevron that opens
            // the list
            let height = ui.text_style_height(&chip_style(small)) + if small { 2.0 } else { 6.0 };
            let chevron = 16.0;
            let (rect, _) = ui.allocate_exact_size(vec2(field_width + chevron, height), Sense::hover());
            let weak = ui.visuals().weak_text_color();
            let focused = ui.ctx().memory(|m| m.has_focus(field_id));
            ui.painter().rect_filled(rect, height / 2.0, soft_fill(ui));
            let inner = Rect::from_min_max(
                rect.min + vec2(if small { 10.0 } else { 12.0 }, 0.0),
                egui::pos2(rect.right() - chevron, rect.bottom()),
            );
            // the field centered in the pill, as high as its text
            let field = ui
                .scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut input)
                                .id(field_id)
                                .hint_text(hint)
                                .frame(egui::Frame::NONE)
                                .margin(vec2(0.0, 0.0))
                                .desired_width(inner.width())
                                .font(chip_style(small)),
                        )
                    },
                )
                .inner;
            let edge = if focused { ui.visuals().selection.stroke.color } else { ui.visuals().text_color().gamma_multiply(0.55) };
            ui.painter().rect_stroke(rect, height / 2.0, Stroke::new(1.0, edge), StrokeKind::Inside);
            let toggle = ui.interact(
                Rect::from_min_max(egui::pos2(rect.right() - chevron, rect.top()), rect.max),
                id.with("chevron"),
                Sense::click(),
            );
            let c = toggle.rect.center();
            let color = if toggle.hovered() { ui.visuals().text_color() } else { weak };
            let stroke = Stroke::new(1.3, color);
            ui.painter().line_segment([c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0)], stroke);
            ui.painter().line_segment([c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)], stroke);
            if toggle.clicked() {
                field.request_focus();
            }
            field
        } else {
            let mut field = egui::TextEdit::singleline(&mut input)
                .id(field_id)
                .hint_text(hint)
                .desired_width(field_width)
                .font(chip_style(small));
            if small {
                field = field.margin(vec2(4.0, 1.0)); // as high as the small chips
            }
            field.show(ui).response.response
        };
        // after a tag was added, the field that added it takes the focus back (two widgets can
        // show the same Tags, so it is kept by the id of the widget)
        let refocus_id = id.with("refocus");
        if ui.ctx().data_mut(|d| d.remove_temp::<bool>(refocus_id)).unwrap_or(false) {
            field.request_focus();
        }
        if input != typed_before {
            highlight = None; // the list changed
        }
        // the suggestions that match what is typed, under the field
        let typed = normalize(&input).to_lowercase();
        let matching: Vec<String> = if typed.is_empty() && !dropdown {
            vec![]
        } else {
            suggestions
                .iter()
                .map(|s| normalize(s))
                .filter(|s| (pick || !state.tags.contains(s)) && s.to_lowercase().contains(&typed))
                .take(if dropdown { 200 } else { 8 })
                .collect()
        };
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter {
            if let Some(chosen) = highlight.and_then(|h| matching.get(h)) {
                // the row chosen with the arrows
                changed |= if pick { state.toggle(chosen) } else { state.insert(chosen) };
                input.clear();
                highlight = None;
                ui.ctx().data_mut(|d| d.insert_temp(refocus_id, true));
            } else if !input.trim().is_empty() {
                let typed = std::mem::take(&mut input);
                changed |= state.insert(&typed);
                ui.ctx().data_mut(|d| d.insert_temp(refocus_id, true));
            }
        }
        if field.has_focus() && input.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Backspace)) {
            changed |= state.tags.pop().is_some();
        }

        let popup_id = id.with("suggestions");
        if field.has_focus() && !matching.is_empty() {
            let mut picked = None;
            // under the field, or over it when the window has not the room for the list
            let screen = ui.ctx().content_rect();
            let row_h = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
            let wanted = (matching.len() as f32 * row_h + 20.0).min(list_height + 20.0);
            let below = screen.bottom() - field.rect.bottom() - 12.0;
            let above = field.rect.top() - screen.top() - 12.0;
            // the place is chosen when the list opens and kept while it is open: the field moves when a chip
            // is added, the list must not follow it
            let anchor_id = popup_id.with("anchor");
            let (up, anchor) = match ui.ctx().data(|d| d.get_temp::<(bool, egui::Pos2)>(anchor_id)) {
                Some(kept) => kept,
                None => {
                    let up = below < wanted && above > below;
                    let at = if up { field.rect.left_top() - vec2(0.0, 4.0) } else { field.rect.left_bottom() + vec2(0.0, 4.0) };
                    ui.ctx().data_mut(|d| d.insert_temp(anchor_id, (up, at)));
                    (up, at)
                }
            };
            let room = if up { above } else { below };
            let list_max = list_height.min((room - 20.0).max(60.0));
            let area = egui::Area::new(popup_id).order(egui::Order::Foreground);
            let area = if up {
                area.pivot(egui::Align2::LEFT_BOTTOM).fixed_pos(anchor)
            } else {
                area.fixed_pos(anchor)
            };
            area
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).fill(soft_fill(ui)).show(ui, |ui| {
                        // the scroll bar takes its own room: it must not cover the long names
                        ui.spacing_mut().scroll.floating = false;
                        ui.set_min_width(field_width + 60.0);
                        // the whole list of a dropdown scrolls
                        // the height of the content seen the frame before: the area is never shorter than its rows
                        let height_id = popup_id.with("content-height");
                        let known: f32 = ui.ctx().data(|d| d.get_temp(height_id)).unwrap_or(0.0);
                        let scrolled = egui::ScrollArea::vertical()
                            .min_scrolled_height(known.min(list_max))
                            .max_height(list_max)
                            .auto_shrink([true, true])
                            .show(ui, |ui| {
                        for (index, suggestion) in matching.iter().enumerate() {
                            // the full path in the colors of its levels, over a row that lights up
                            let background = ui.painter().add(egui::Shape::Noop);
                            let row = ui
                                .horizontal(|ui| {
                                    if pick {
                                        let on = state.tags.contains(suggestion);
                                        let (boxed, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                                        let ink = ui.visuals().text_color();
                                        if on {
                                            ui.painter().rect_filled(boxed, 3.0, ui.visuals().hyperlink_color);
                                            let (m, white) = (boxed.center(), Stroke::new(1.6, Color32::WHITE));
                                            ui.painter().line_segment([m + vec2(-3.5, 0.5), m + vec2(-1.0, 3.0)], white);
                                            ui.painter().line_segment([m + vec2(-1.0, 3.0), m + vec2(3.8, -3.0)], white);
                                        } else {
                                            ui.painter().rect_stroke(boxed, 3.0, Stroke::new(1.3, ink.gamma_multiply(0.7)), StrokeKind::Inside);
                                        }
                                    }
                                    path_ui(ui, &state, suggestion)
                                })
                                .response;
                            let hit = ui.interact(row.rect, popup_id.with(suggestion), Sense::click());
                            let current = highlight == Some(index);
                            if current && moved {
                                row.scroll_to_me(None);
                            }
                            if current {
                                // the row of the keyboard: drawn from the color of the text, so it
                                // shows on a light and on a dark background
                                let visuals = ui.visuals();
                                let text = visuals.text_color();
                                let rect = row.rect.expand2(vec2(4.0, 2.0));
                                let mut shape =
                                    egui::epaint::RectShape::filled(rect, 4.0, text.gamma_multiply(0.16));
                                shape.stroke = Stroke::new(1.5, text.gamma_multiply(0.75));
                                shape.stroke_kind = StrokeKind::Inside;
                                ui.painter().set(background, egui::Shape::Rect(shape));
                            } else if hit.hovered() {
                                let fill = ui.visuals().widgets.hovered.weak_bg_fill;
                                ui.painter().set(background, egui::Shape::rect_filled(row.rect.expand(2.0), 4.0, fill));
                            }
                            // on the press: on the release the field has lost the focus and the
                            // list is gone
                            if hit.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
                                picked = Some(suggestion.clone());
                            }
                        }
                        });
                        let content = scrolled.content_size.y;
                        if (content - known).abs() > 0.5 {
                            ui.ctx().data_mut(|d| d.insert_temp(height_id, content));
                            ui.ctx().request_repaint();
                        }
                    });
                });
            if let Some(suggestion) = picked {
                changed |= if pick { state.toggle(&suggestion) } else { state.insert(&suggestion) };
                input.clear();
                ui.ctx().data_mut(|d| d.insert_temp(refocus_id, true));
                highlight = None;
            }
        }
        if !(field.has_focus() && !matching.is_empty()) {
            ui.ctx().data_mut(|d| d.remove_temp::<(bool, egui::Pos2)>(popup_id.with("anchor")));
        }
        // for the next frame: the rows of the list, the one highlighted
        let shown = if field.has_focus() { matching.len() } else { 0 };
        let highlight = highlight.filter(|h| *h < shown);
        ui.ctx().data_mut(|d| {
            d.insert_temp(count_id, shown);
            d.insert_temp(highlight_id, highlight);
        });
    });
    if on_hover {
        ui.ctx().data_mut(|d| d.insert_temp(input_id, input));
    } else {
        state.input = input;
    }
    changed
}

/// The flat editor of the tags of a cell: the tags as small chips, no outline, no fill, an arrow while the mouse is
/// over it. A click opens the tree of `suggestions` (the paths of all the tags, in the order of the tree): an indented
/// row per level with a check box, the one of the tags that are on. A field filters them (the full paths then) and
/// Enter puts the typed tag on, creating it when it is new. The `muted` tags (inherited) are shown but not changed.
/// Returns true when a tag was put on or taken off.
pub fn tags_pick(ui: &mut egui::Ui, tags: &mut Tags, o: &TagsOptions) -> bool {
    let id = o.id;
    let muted: HashSet<String> = o.muted.iter().map(|t| normalize(t)).collect();
    let height = ui.text_style_height(&chip_style(true)) + 8.0;
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width().max(60.0), height.max(ui.text_style_height(&TextStyle::Body) + 6.0)));
    ui.allocate_rect(rect, Sense::hover());
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    inner.shrink_clip_rect(rect);
    if tags.tags.is_empty() {
        inner.weak("—");
    } else {
        let mut shown = TagsOptions::new(id.with("chips"));
        shown.editable = false;
        shown.small = true;
        shown.icon = o.icon;
        shown.fit = true;
        shown.muted = o.muted.clone();
        tags_edit(&mut inner, tags, &shown);
    }
    // after the chips, so that it is above them and gets the click
    let response = ui.interact(rect, id.with("button"), Sense::click());
    let popup_id = id.with("popup");
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    if o.editable && (response.hovered() || open) {
        let c = egui::pos2(rect.right() - 10.0, rect.center().y);
        let points = vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)];
        ui.painter().add(egui::Shape::convex_polygon(points, ui.visuals().widgets.inactive.fg_stroke.color, Stroke::NONE));
    }
    if !o.editable {
        return false;
    }
    let (focus_id, input_id) = (id.with("focus"), id.with("input"));
    if !open {
        ui.ctx().data_mut(|d| d.remove_temp::<bool>(focus_id));
    }
    let mut changed = false;
    egui::Popup::menu(&response).id(popup_id).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(240.0);
        let mut input: String = ui.ctx().data(|d| d.get_temp(input_id)).unwrap_or_default();
        let field = ui.add(egui::TextEdit::singleline(&mut input).hint_text(&o.hint_text).desired_width(f32::INFINITY));
        if !ui.ctx().data(|d| d.get_temp::<bool>(focus_id)).unwrap_or(false) {
            field.request_focus();
            ui.ctx().data_mut(|d| d.insert_temp(focus_id, true));
        }
        ui.add_space(2.0);
        let typed = normalize(&input);
        let filter = typed.to_lowercase();
        let rows: Vec<&String> = o.suggestions.iter().filter(|s| filter.is_empty() || normalize(s).to_lowercase().contains(&filter)).collect();
        let mut toggled: Option<String> = None;
        egui::ScrollArea::vertical().max_height(o.list_height.max(120.0)).auto_shrink([true, true]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for path in &rows {
                let path = normalize(path);
                let depth = if filter.is_empty() { path.matches('/').count() } else { 0 };
                let on = tags.tags.contains(&path);
                let inherited = on && muted.contains(&path);
                let (row, row_response) = ui.allocate_exact_size(vec2(ui.available_width().max(220.0), 24.0), Sense::click());
                let ink = ui.visuals().text_color();
                if row_response.hovered() && !inherited {
                    ui.painter().rect_filled(row, 5.0, ui.visuals().widgets.hovered.weak_bg_fill);
                }
                let boxed = Rect::from_center_size(egui::pos2(row.left() + 12.0 + depth as f32 * 16.0, row.center().y), vec2(14.0, 14.0));
                if on {
                    ui.painter().rect_filled(boxed, 3.0, ui.visuals().hyperlink_color.gamma_multiply(if inherited { 0.5 } else { 1.0 }));
                    let (m, white) = (boxed.center(), Stroke::new(1.6, Color32::WHITE));
                    ui.painter().line_segment([m + vec2(-3.5, 0.5), m + vec2(-1.0, 3.0)], white);
                    ui.painter().line_segment([m + vec2(-1.0, 3.0), m + vec2(3.8, -3.0)], white);
                } else {
                    ui.painter().rect_stroke(boxed, 3.0, Stroke::new(1.3, ink.gamma_multiply(0.7)), StrokeKind::Inside);
                }
                let color = tags.color_of(&path);
                let icon_at = egui::pos2(boxed.right() + 12.0, row.center().y);
                paint_tag_icon(ui, icon_at, 14.0, color);
                let name = if filter.is_empty() { last_level(&path).to_owned() } else { path.clone() };
                let text_color = if inherited { color.gamma_multiply(0.6) } else { color };
                let galley = ui.painter().layout_no_wrap(name, TextStyle::Body.resolve(ui.style()), text_color);
                ui.painter().galley(egui::pos2(icon_at.x + 12.0, row.center().y - galley.size().y / 2.0), galley, text_color);
                if inherited {
                    row_response.on_hover_text("Hérité d'un dossier");
                } else if row_response.clicked() {
                    toggled = Some(path);
                }
            }
            if rows.is_empty() && typed.is_empty() {
                ui.weak("Aucune étiquette");
            }
        });
        if let Some(path) = toggled {
            changed |= tags.toggle(&path);
        }
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter && !typed.is_empty() {
            // an existing tag is toggled, a new one is created and put on
            let known = o.suggestions.iter().map(|s| normalize(s)).find(|s| s.to_lowercase() == filter);
            changed |= match known {
                Some(path) if muted.contains(&path) => false,
                Some(path) => tags.toggle(&path),
                None => tags.insert(&typed),
            };
            input.clear();
            field.request_focus();
        } else if !typed.is_empty() && rows.is_empty() {
            ui.weak(format!("Entrée : créer « {typed} »"));
        }
        ui.ctx().data_mut(|d| d.insert_temp(input_id, input));
    });
    changed
}
