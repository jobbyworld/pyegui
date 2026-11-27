//! A property set: the property grid of an IDE (the "Properties" window of Visual Studio). A table of two
//! columns, the name and the editor of the value, the rows kept in groups that fold, or sorted by name; a
//! filter field, a splitter between the columns, and a description of the row selected at the bottom.
//!
//! The widget keeps no value: it is given the properties, draws one editor for each, and returns what the
//! person changed (`Change`), for the caller to apply. The state (`PropertySetState`) holds what is only
//! about the look: the order, the groups folded, the filter, the row selected, the text being typed.
//! Shared by the Python extension (`property_set`) and the Rust applications.

use std::collections::{HashMap, HashSet};

use egui::{Align, Color32, CursorIcon, FontId, Id, Layout, Rect, RichText, Sense, Stroke, TextStyle, Ui, UiBuilder};

/// A choice of a list
#[derive(Clone, PartialEq, Debug, Default)]
pub struct PropOption {
    pub id: String,
    pub label: String,
    /// a dot of this color in front of the label
    pub color: Option<Color32>,
}

/// What a property holds, so which editor it gets
#[derive(Clone, PartialEq, Debug)]
pub enum Kind {
    /// one line
    Text,
    /// several lines
    LongText,
    /// typed like a text, checked by the caller
    Number,
    Bool,
    /// typed like a text ("12/03/2026", "tomorrow"), read by the caller
    Date,
    /// a list: `open` lets the person add an option
    Choice { options: Vec<PropOption>, multiple: bool, open: bool },
    /// shown, never edited (a computed value)
    Readonly,
    /// a color button with its picker: the value is "#rrggbb", or "#rrggbbaa" when the transparency is edited
    Color { alpha: bool },
    /// a slider: the value is the number (a whole one when `integer`)
    Range { min: f64, max: f64, integer: bool },
    /// a button: a click is a `Change::Set` with the value "1"
    Action { label: String },
}

/// "#rrggbb" or "#rrggbbaa" as a color (not premultiplied)
pub fn parse_hex(text: &str) -> Option<Color32> {
    let t = text.trim().strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(t.get(i..i + 2)?, 16).ok();
    match t.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

/// A color as the value of a property
pub fn to_hex(color: Color32, alpha: bool) -> String {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    if alpha { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}") }
}

/// One row
#[derive(Clone, PartialEq, Debug)]
pub struct Property {
    /// what a `Change` names
    pub id: String,
    pub name: String,
    /// the group (category) of the row; empty for the default group
    pub group: String,
    /// shown at the bottom when the row is selected, and as a tooltip of the name
    pub description: String,
    pub kind: Kind,
    /// the value as a text: what is typed, "1" or "0" for a bool
    pub value: String,
    /// a choice: the ids of the options chosen
    pub selected: Vec<String>,
    /// what the row has without a value of its own ("↳ inherited from X"): shown, weak, where the value would be
    pub note: String,
    /// shown in an empty field
    pub hint: String,
    /// a suffix after a number ("kg")
    pub unit: String,
    pub readonly: bool,
}

impl Property {
    pub fn new(id: &str, name: &str, kind: Kind) -> Self {
        Property {
            id: id.to_owned(),
            name: name.to_owned(),
            group: String::new(),
            description: String::new(),
            kind,
            value: String::new(),
            selected: Vec::new(),
            note: String::new(),
            hint: String::new(),
            unit: String::new(),
            readonly: false,
        }
    }

    fn has_value(&self) -> bool {
        match self.kind {
            Kind::Choice { .. } => !self.selected.is_empty(),
            Kind::Bool => !self.value.is_empty(),
            _ => !self.value.is_empty(),
        }
    }

    fn editable(&self) -> bool {
        !self.readonly && self.kind != Kind::Readonly
    }

    /// a value that can be cleared (a color, a number on a slider or a button cannot)
    fn clearable(&self) -> bool {
        self.editable() && self.has_value() && !matches!(self.kind, Kind::Color { .. } | Kind::Range { .. } | Kind::Action { .. } | Kind::Bool)
    }
}

/// What the person changed
#[derive(Clone, PartialEq, Debug)]
pub enum Change {
    /// a new text (or "1" / "0" for a bool): the caller reads it, and keeps the old value if it does not
    Set { id: String, value: String },
    /// the value was cleared
    Clear { id: String },
    /// the options chosen (all of them, for a list that takes several)
    Choose { id: String, ids: Vec<String> },
    /// a new option of an open list, to add and choose
    AddOption { id: String, label: String },
}

/// The look of the set that the person changes: kept between frames, create it once
pub struct PropertySetState {
    /// by group (true) or by name
    pub grouped: bool,
    pub collapsed: HashSet<String>,
    pub filter: String,
    pub selected: Option<String>,
    /// the width of the name column, as a fraction of the width
    pub split: f32,
    buffers: HashMap<String, String>,
    /// the colors being edited: the value last given or sent, and the color in hue, saturation, value and alpha
    /// (a color changed through its transparency would lose its hue in the bytes of a color with alpha)
    colors: HashMap<String, (String, egui::ecolor::Hsva)>,
    new_option: HashMap<String, String>,
    key: String,
}

impl PropertySetState {
    pub fn new(grouped: bool) -> Self {
        PropertySetState { grouped, ..Default::default() }
    }
}

impl Default for PropertySetState {
    fn default() -> Self {
        PropertySetState { grouped: true, collapsed: HashSet::new(), filter: String::new(), selected: None, split: 0.42, buffers: HashMap::new(), colors: HashMap::new(), new_option: HashMap::new(), key: String::new() }
    }
}

/// How a set is drawn
pub struct PropertySetOptions {
    /// tells two sets apart in one ui
    pub id: Id,
    /// what the properties are about (a path, say): when it changes the texts being typed are dropped and the
    /// selection is forgotten
    pub key: String,
    pub toolbar: bool,
    pub description: bool,
    pub row_height: f32,
    /// the name of the group of the rows that have none
    pub default_group: String,
    pub grouped_label: String,
    pub alphabetical_label: String,
    pub filter_hint: String,
    pub clear_tip: String,
    pub add_hint: String,
    pub empty_text: String,
}

impl PropertySetOptions {
    pub fn new(id: Id) -> Self {
        PropertySetOptions {
            id,
            key: String::new(),
            toolbar: true,
            description: true,
            row_height: 26.0,
            default_group: "Misc".to_owned(),
            grouped_label: "Categorized".to_owned(),
            alphabetical_label: "Alphabetical".to_owned(),
            filter_hint: "Filter".to_owned(),
            clear_tip: "Clear the value".to_owned(),
            add_hint: "Add an option".to_owned(),
            empty_text: "No property".to_owned(),
        }
    }
}

enum Item<'a> {
    Header { group: String, count: usize },
    Row(&'a Property),
}

fn matches(p: &Property, filter: &str) -> bool {
    filter.is_empty() || p.name.to_lowercase().contains(filter) || p.group.to_lowercase().contains(filter) || p.description.to_lowercase().contains(filter) || p.value.to_lowercase().contains(filter)
}

/// The rows in the order they are shown: with the headers of the groups (in the order of first appearance),
/// or sorted by name
fn layout<'a>(props: &'a [Property], state: &PropertySetState, default_group: &str) -> Vec<Item<'a>> {
    let filter = state.filter.trim().to_lowercase();
    let shown: Vec<&Property> = props.iter().filter(|p| matches(p, &filter)).collect();
    if !state.grouped {
        let mut sorted = shown;
        sorted.sort_by_key(|p| p.name.to_lowercase());
        return sorted.into_iter().map(Item::Row).collect();
    }
    let group_of = |p: &Property| if p.group.trim().is_empty() { default_group.to_owned() } else { p.group.trim().to_owned() };
    let mut order: Vec<String> = Vec::new();
    for p in &shown {
        let g = group_of(p);
        if !order.contains(&g) {
            order.push(g);
        }
    }
    let mut out = Vec::new();
    for g in order {
        let rows: Vec<&Property> = shown.iter().copied().filter(|p| group_of(p) == g).collect();
        let folded = filter.is_empty() && state.collapsed.contains(&g);
        out.push(Item::Header { group: g, count: rows.len() });
        if !folded {
            out.extend(rows.into_iter().map(Item::Row));
        }
    }
    out
}

fn cross(ui: &Ui, center: egui::Pos2, color: Color32) {
    let r = 3.5;
    let stroke = Stroke::new(1.4, color);
    ui.painter().line_segment([center + egui::vec2(-r, -r), center + egui::vec2(r, r)], stroke);
    ui.painter().line_segment([center + egui::vec2(-r, r), center + egui::vec2(r, -r)], stroke);
}

fn chevron(ui: &Ui, center: egui::Pos2, open: bool, color: Color32) {
    let r = 3.5;
    let points = if open {
        vec![center + egui::vec2(-r, -r * 0.5), center + egui::vec2(r, -r * 0.5), center + egui::vec2(0.0, r * 0.7)]
    } else {
        vec![center + egui::vec2(-r * 0.5, -r), center + egui::vec2(r * 0.7, 0.0), center + egui::vec2(-r * 0.5, r)]
    };
    ui.painter().add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

fn row_height(p: &Property, base: f32) -> f32 {
    if p.kind == Kind::LongText {
        base * 2.8
    } else {
        base
    }
}

/// Draws the set and returns what the person changed in this frame
pub fn property_set(ui: &mut Ui, state: &mut PropertySetState, props: &[Property], o: &PropertySetOptions) -> Vec<Change> {
    let mut changes = Vec::new();
    if state.key != o.key {
        state.key = o.key.clone();
        state.buffers.clear();
        state.colors.clear();
        state.new_option.clear();
        state.selected = None;
    }
    if o.toolbar {
        toolbar(ui, state, o);
        ui.add_space(4.0);
    }
    let items = layout(props, state, &o.default_group);
    let pane_height = if o.description { 64.0 } else { 0.0 };
    let body_height = (ui.available_height() - pane_height - if o.description { 6.0 } else { 0.0 }).max(80.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical().id_salt(o.id.with("scroll")).max_height(body_height).auto_shrink([false, false]).show(ui, |ui| {
        if items.is_empty() {
            ui.add_space(6.0);
            ui.weak(&o.empty_text);
        }
        for item in &items {
            match item {
                Item::Header { group, count } => header(ui, state, group, *count, o),
                Item::Row(p) => row(ui, state, p, o, &mut changes),
            }
        }
    });
    if o.description {
        ui.add_space(6.0);
        description_pane(ui, state, props, pane_height);
    }
    changes
}

fn toolbar(ui: &mut Ui, state: &mut PropertySetState, o: &PropertySetOptions) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let visuals = ui.visuals().clone();
        let frame = egui::Frame::new().stroke(Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color)).corner_radius(crate::style::fit_radius(&visuals, 8.0) as u8).inner_margin(2);
        frame.show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if ui.selectable_label(state.grouped, &o.grouped_label).clicked() {
                state.grouped = true;
            }
            if ui.selectable_label(!state.grouped, &o.alphabetical_label).clicked() {
                state.grouped = false;
            }
        });
        let width = ui.available_width();
        ui.add(egui::TextEdit::singleline(&mut state.filter).hint_text(&o.filter_hint).desired_width(width));
    });
}

fn header(ui: &mut Ui, state: &mut PropertySetState, group: &str, count: usize, o: &PropertySetOptions) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), o.row_height), Sense::click());
    let visuals = ui.visuals().clone();
    let fill = visuals.widgets.noninteractive.weak_bg_fill.gamma_multiply(if response.hovered() { 1.0 } else { 0.7 });
    ui.painter().rect_filled(rect, crate::style::fit_radius(ui.visuals(), 0.0), fill);
    let filtering = !state.filter.trim().is_empty();
    let folded = !filtering && state.collapsed.contains(group);
    chevron(ui, rect.left_center() + egui::vec2(12.0, 0.0), !folded, visuals.text_color());
    let font = TextStyle::Body.resolve(ui.style());
    ui.painter().text(rect.left_center() + egui::vec2(26.0, 0.0), egui::Align2::LEFT_CENTER, group, FontId::new(font.size, font.family.clone()), visuals.strong_text_color());
    ui.painter().text(rect.right_center() - egui::vec2(10.0, 0.0), egui::Align2::RIGHT_CENTER, count.to_string(), TextStyle::Small.resolve(ui.style()), visuals.weak_text_color());
    if response.clicked() && !filtering && !state.collapsed.remove(group) {
        state.collapsed.insert(group.to_owned());
    }
}

fn row(ui: &mut Ui, state: &mut PropertySetState, p: &Property, o: &PropertySetOptions, changes: &mut Vec<Change>) {
    let width = ui.available_width();
    let height = row_height(p, o.row_height);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    let visuals = ui.visuals().clone();
    let selected = state.selected.as_deref() == Some(p.id.as_str());
    if selected {
        ui.painter().rect_filled(rect, crate::style::fit_radius(ui.visuals(), 0.0), visuals.selection.bg_fill.gamma_multiply(0.35));
    } else if response.hovered() {
        ui.painter().rect_filled(rect, crate::style::fit_radius(ui.visuals(), 0.0), visuals.widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
    }
    let line = visuals.widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6);
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, line));
    let split_x = rect.left() + state.split * width;
    ui.painter().vline(split_x, rect.y_range(), Stroke::new(1.0, line));
    if response.clicked() {
        state.selected = Some(p.id.clone());
    }

    // the name
    let indent = if state.grouped { 26.0 } else { 10.0 };
    let name_rect = Rect::from_min_max(egui::pos2(rect.left() + indent, rect.top()), egui::pos2(split_x - 6.0, rect.bottom()));
    let name_color = if p.editable() { visuals.text_color() } else { visuals.weak_text_color() };
    let top = if p.kind == Kind::LongText { name_rect.left_top() + egui::vec2(0.0, 6.0) } else { name_rect.left_center() };
    let anchor = if p.kind == Kind::LongText { egui::Align2::LEFT_TOP } else { egui::Align2::LEFT_CENTER };
    ui.painter().with_clip_rect(name_rect).text(top, anchor, &p.name, TextStyle::Body.resolve(ui.style()), name_color);
    let name_hit = ui.interact(name_rect, o.id.with(("name", &p.id)), Sense::hover());
    let tip = if p.note.is_empty() { p.description.clone() } else if p.description.is_empty() { p.note.clone() } else { format!("{}\n{}", p.description, p.note) };
    if !tip.is_empty() {
        name_hit.on_hover_text(tip);
    }

    // the splitter
    let handle = Rect::from_x_y_ranges((split_x - 3.0)..=(split_x + 3.0), rect.y_range());
    let drag = ui.interact(handle, o.id.with(("split", &p.id)), Sense::drag());
    if drag.hovered() || drag.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    if drag.dragged() {
        if let Some(pointer) = ui.ctx().pointer_latest_pos() {
            state.split = ((pointer.x - rect.left()) / width).clamp(0.2, 0.8);
        }
    }

    // the value
    let clear_room = if p.clearable() { 22.0 } else { 0.0 };
    let value_rect = Rect::from_min_max(egui::pos2(split_x + 6.0, rect.top() + 2.0), egui::pos2(rect.right() - 6.0 - clear_room, rect.bottom() - 2.0));
    let align = if p.kind == Kind::LongText { Align::Min } else { Align::Center };
    let mut vui = ui.new_child(UiBuilder::new().max_rect(value_rect).layout(Layout::left_to_right(align)));
    vui.set_clip_rect(value_rect.intersect(ui.clip_rect()));
    editor(&mut vui, state, p, o, changes);
    if clear_room > 0.0 {
        let spot = Rect::from_center_size(egui::pos2(rect.right() - 17.0, rect.center().y), egui::vec2(18.0, 18.0));
        let hit = ui.interact(spot, o.id.with(("clear", &p.id)), Sense::click());
        if response.hovered() || hit.hovered() || selected {
            cross(ui, spot.center(), if hit.hovered() { visuals.strong_text_color() } else { visuals.weak_text_color() });
        }
        if hit.on_hover_text(&o.clear_tip).on_hover_cursor(CursorIcon::PointingHand).clicked() {
            state.buffers.remove(&p.id);
            changes.push(Change::Clear { id: p.id.clone() });
        }
    }
}

/// What is typed as a color: "#rrggbbaa", "rrggbb", "#rgb", "#rgba", the # being optional
pub fn parse_hex_loose(text: &str) -> Option<[u8; 4]> {
    let t = text.trim().trim_start_matches('#');
    if !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let expanded: String = match t.len() {
        3 | 4 => t.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => t.to_owned(),
        _ => return None,
    };
    let byte = |i: usize| u8::from_str_radix(&expanded[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?, if expanded.len() == 8 { byte(6)? } else { 255 }])
}

fn hex_of_bytes([r, g, b, a]: [u8; 4], alpha: bool) -> String {
    if alpha { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}") }
}

/// A color button with its picker and a field with the color as HTML (#rrggbbaa) that can be copied and pasted.
/// The picker works on its own hue, saturation and value, kept here: it is not recomputed from the bytes at each
/// frame, or the tint would drift when the transparency is dragged.
fn color_editor(ui: &mut Ui, state: &mut PropertySetState, p: &Property, alpha: bool, changes: &mut Vec<Change>) {
    use egui::ecolor::Hsva;
    let bytes = |text: &str| parse_hex_loose(text).unwrap_or([128, 128, 128, 255]);
    let entry = state.colors.entry(p.id.clone()).or_insert_with(|| (p.value.clone(), Hsva::from_srgba_unmultiplied(bytes(&p.value))));
    if entry.0 != p.value {
        // changed by the caller: the color is read again
        *entry = (p.value.clone(), Hsva::from_srgba_unmultiplied(bytes(&p.value)));
    }
    let mut hsva = entry.1;
    let mode = if alpha { egui::color_picker::Alpha::OnlyBlend } else { egui::color_picker::Alpha::Opaque };
    let mut sent: Option<String> = None;
    ui.add_enabled_ui(p.editable(), |ui| {
        if egui::color_picker::color_edit_button_hsva(ui, &mut hsva, mode).changed() {
            sent = Some(hex_of_bytes(hsva.to_srgba_unmultiplied(), alpha));
        }
    });
    // the field: what is typed is read when it loses the focus
    let key = format!("{}#html", p.id);
    let shown = hex_of_bytes(hsva.to_srgba_unmultiplied(), alpha);
    let buffer = state.buffers.entry(key.clone()).or_insert_with(|| shown.clone());
    let width = (ui.available_width() - 4.0).clamp(60.0, 96.0);
    let edit = ui.add_enabled(p.editable(), egui::TextEdit::singleline(buffer).desired_width(width).font(TextStyle::Monospace).hint_text(if alpha { "#rrggbbaa" } else { "#rrggbb" }));
    if edit.lost_focus() {
        if let Some(b) = parse_hex_loose(buffer) {
            hsva = Hsva::from_srgba_unmultiplied(b);
            sent = Some(hex_of_bytes(b, alpha));
        }
    }
    if !edit.has_focus() {
        *buffer = sent.clone().unwrap_or(shown);
    }
    if let Some(value) = sent {
        changes.push(Change::Set { id: p.id.clone(), value: value.clone() });
        state.colors.insert(p.id.clone(), (value, hsva));
    } else if let Some(entry) = state.colors.get_mut(&p.id) {
        entry.1 = hsva;
    }
    if !p.note.is_empty() {
        ui.label(weak_note(ui, &p.note));
    }
}

fn weak_note(ui: &Ui, text: &str) -> RichText {
    RichText::new(text).italics().color(ui.visuals().weak_text_color())
}

fn editor(ui: &mut Ui, state: &mut PropertySetState, p: &Property, o: &PropertySetOptions, changes: &mut Vec<Change>) {
    let width = ui.available_width();
    match &p.kind {
        Kind::Readonly => {
            if p.value.is_empty() {
                ui.label(RichText::new("—").weak());
            } else {
                ui.label(&p.value);
            }
        }
        Kind::Bool => {
            let mut on = p.value == "1";
            if ui.add_enabled(p.editable(), egui::Checkbox::new(&mut on, "")).changed() {
                changes.push(Change::Set { id: p.id.clone(), value: if on { "1" } else { "0" }.to_owned() });
            }
            if p.value.is_empty() && !p.note.is_empty() {
                ui.label(weak_note(ui, &p.note));
            }
        }
        Kind::Color { alpha } => color_editor(ui, state, p, *alpha, changes),
        Kind::Range { min, max, integer } => {
            let mut v: f64 = p.value.trim().parse().unwrap_or(*min);
            ui.spacing_mut().slider_width = (width - 74.0).max(60.0);
            let mut slider = egui::Slider::new(&mut v, *min..=*max);
            if *integer {
                slider = slider.integer();
            }
            if !p.unit.is_empty() {
                slider = slider.suffix(p.unit.clone());
            }
            if ui.add_enabled(p.editable(), slider).changed() {
                let text = if *integer { format!("{}", v.round() as i64) } else { format!("{v}") };
                changes.push(Change::Set { id: p.id.clone(), value: text });
            }
        }
        Kind::Action { label } => {
            if ui.add_enabled(p.editable(), egui::Button::new(label)).clicked() {
                changes.push(Change::Set { id: p.id.clone(), value: "1".to_owned() });
            }
        }
        Kind::Choice { options, multiple, open } => choice(ui, state, p, options, *multiple, *open, width, o, changes),
        Kind::Text | Kind::Number | Kind::Date | Kind::LongText => {
            let buffer = state.buffers.entry(p.id.clone()).or_insert_with(|| p.value.clone());
            let hint = if !p.note.is_empty() { p.note.as_str() } else { p.hint.as_str() };
            let unit_room = if p.kind == Kind::Number && !p.unit.is_empty() && !p.value.is_empty() { 34.0 } else { 0.0 };
            let edit = if p.kind == Kind::LongText {
                egui::TextEdit::multiline(buffer).desired_rows(2).desired_width(width - unit_room)
            } else {
                egui::TextEdit::singleline(buffer).desired_width(width - unit_room)
            };
            let response = ui.add_enabled(p.editable(), edit.hint_text(RichText::new(hint).italics()).frame(egui::Frame::NONE));
            if response.has_focus() {
                ui.painter().rect_stroke(response.rect.expand(2.0), crate::style::fit_radius(ui.visuals(), 3.0), Stroke::new(1.0, ui.visuals().selection.stroke.color), egui::StrokeKind::Inside);
            }
            // a text or a number is kept as it is typed (a text that is not a number yet is refused by the caller): a click on another item changes the key of the set in the same frame
            // as the focus is lost, and what was typed would be dropped with the buffers
            let live = response.changed() && matches!(p.kind, Kind::Text | Kind::LongText | Kind::Number);
            if (live || response.lost_focus()) && *buffer != p.value {
                changes.push(Change::Set { id: p.id.clone(), value: buffer.clone() });
            }
            if !response.has_focus() && *buffer != p.value {
                // changed elsewhere, or refused by the caller
                *buffer = p.value.clone();
            }
            if unit_room > 0.0 {
                ui.label(RichText::new(&p.unit).weak());
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn choice(ui: &mut Ui, state: &mut PropertySetState, p: &Property, options: &[PropOption], multiple: bool, open: bool, width: f32, o: &PropertySetOptions, changes: &mut Vec<Change>) {
    let note = if p.note.is_empty() { None } else { Some(p.note.as_str()) };
    let new_option = state.new_option.entry(p.id.clone()).or_default();
    let spec = ChoiceSpec { salt: o.id.with(("choice", &p.id)), options, selected: &p.selected, multiple, open, editable: p.editable(), width, add_hint: &o.add_hint, note, flat: false, muted: false };
    match choice_combo(ui, &spec, new_option) {
        Some(ChoiceChange::Choose(ids)) => changes.push(Change::Choose { id: p.id.clone(), ids }),
        Some(ChoiceChange::Clear) => changes.push(Change::Clear { id: p.id.clone() }),
        Some(ChoiceChange::AddOption(label)) => changes.push(Change::AddOption { id: p.id.clone(), label }),
        None => {}
    }
}

/// What a list editor shows
pub struct ChoiceSpec<'a> {
    pub salt: egui::Id,
    pub options: &'a [PropOption],
    pub selected: &'a [String],
    pub multiple: bool,
    /// a field adds a new option (and chooses it)
    pub open: bool,
    pub editable: bool,
    pub width: f32,
    pub add_hint: &'a str,
    /// shown weak instead of the dash when nothing is chosen (e.g. the values of a folder differ)
    pub note: Option<&'a str>,
    /// no outline and no fill (the arrow shows while the mouse is over it), e.g. in a cell of a view
    pub flat: bool,
    /// the values are drawn faded: a summary of what is below a folder, or a value inherited from it
    pub muted: bool,
}

/// What the person did in a list editor
#[derive(Clone, PartialEq, Debug)]
pub enum ChoiceChange {
    Choose(Vec<String>),
    Clear,
    AddOption(String),
}

/// The editor of a list: a combo box with the chosen labels, which opens a list of the options (a check box
/// for each one when it takes several). It is the editor of the properties pane, also used in the cells of the
/// views. `new_option` is the text being typed for a new option of an open list
pub fn choice_combo(ui: &mut Ui, spec: &ChoiceSpec<'_>, new_option: &mut String) -> Option<ChoiceChange> {
    let mut change = None;
    // the list stays open for a click inside (a check box, the field of a new value) until a click outside (or Escape);
    // the choice of a list of one value closes it by itself
    let behavior = egui::PopupCloseBehavior::CloseOnClickOutside;
    if spec.flat {
        ui.add_enabled_ui(spec.editable, |ui| flat_choice(ui, spec, new_option, behavior, &mut change));
        return change;
    }
    let (options, selected) = (spec.options, spec.selected);
    let shown = if selected.is_empty() {
        match spec.note {
            None => RichText::new("—").weak(),
            Some(note) => weak_note(ui, note),
        }
    } else {
        let labels: Vec<&str> = selected.iter().filter_map(|id| options.iter().find(|o| &o.id == id)).map(|o| o.label.as_str()).collect();
        RichText::new(labels.join(", "))
    };
    ui.add_enabled_ui(spec.editable, |ui| {
        egui::ComboBox::from_id_salt(spec.salt)
            .popup_style(crate::popup_style::modifier(crate::popup_style::Kind::Dropdown))
            .selected_text(shown)
            .width((spec.width - 4.0).max(40.0))
            .close_behavior(behavior)
            .show_ui(ui, |ui| choice_rows(ui, spec, new_option, &mut change));
    });
    change
}

/// The flat editor of a cell: the values chosen as tags (a chip in the color of the option), no outline, no fill, an
/// arrow while the mouse is over it; a click opens the list of the options
fn flat_choice(ui: &mut Ui, spec: &ChoiceSpec<'_>, new_option: &mut String, behavior: egui::PopupCloseBehavior, change: &mut Option<ChoiceChange>) {
    use crate::tags::{tags_edit, Tags, TagsOptions};
    let neutral = egui::Color32::from_rgb(100, 116, 139);
    let clean = |label: &str| label.replace('/', "-");
    let mut tags = Tags::new(vec![], Default::default());
    for id in spec.selected {
        if let Some(option) = spec.options.iter().find(|o| &o.id == id) {
            let label = clean(&option.label);
            tags.colors.insert(label.clone(), option.color.unwrap_or(neutral));
            tags.insert(&label);
        }
    }
    let height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    let rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(spec.width.max(40.0), height));
    ui.allocate_rect(rect, Sense::hover());
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    inner.shrink_clip_rect(rect);
    if tags.tags.is_empty() {
        match spec.note {
            None => inner.weak("—"),
            Some(note) => inner.label(weak_note(&inner, note)),
        };
    } else {
        let mut options = TagsOptions::new(spec.salt.with("chips"));
        options.editable = false;
        options.small = true;
        options.fit = true;
        if spec.muted {
            options.muted = tags.tags.iter().cloned().collect();
        }
        tags_edit(&mut inner, &mut tags, &options);
    }
    // after the chips, so that it is above them and gets the click
    let response = ui.interact(rect, spec.salt.with("button"), Sense::click());
    if response.hovered() || egui::Popup::is_id_open(ui.ctx(), spec.salt.with("popup")) {
        let c = egui::pos2(rect.right() - 10.0, rect.center().y);
        let points = vec![c + egui::vec2(-4.0, -2.0), c + egui::vec2(4.0, -2.0), c + egui::vec2(0.0, 3.0)];
        ui.painter().add(egui::Shape::convex_polygon(points, ui.visuals().widgets.inactive.fg_stroke.color, egui::Stroke::NONE));
    }
    egui::Popup::menu(&response)
        .style(crate::style::menu_style())
        .id(spec.salt.with("popup"))
        .close_behavior(behavior)
        .show(|ui| choice_rows(ui, spec, new_option, change));
}

/// The options of a list: a check box for each (a round when only one is chosen) and, for an open list, a field
fn choice_rows(ui: &mut Ui, spec: &ChoiceSpec<'_>, new_option: &mut String, change: &mut Option<ChoiceChange>) {
    let (options, selected, multiple) = (spec.options, spec.selected, spec.multiple);
    for option in options {
        ui.horizontal(|ui| {
            if let Some(color) = option.color {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 5.0, color);
            }
            if multiple {
                let mut on = selected.contains(&option.id);
                if ui.checkbox(&mut on, &option.label).changed() {
                    let mut next = selected.to_vec();
                    if on {
                        next.push(option.id.clone());
                    } else {
                        next.retain(|i| *i != option.id);
                    }
                    // the order of the list, not the order of the clicks
                    next.sort_by_key(|id| options.iter().position(|o| &o.id == id));
                    *change = Some(if next.is_empty() { ChoiceChange::Clear } else { ChoiceChange::Choose(next) });
                }
            } else {
                // a round for each one, only one is chosen: a click on it chooses it, a click on the chosen one clears it
                let on = selected == [option.id.clone()];
                if ui.radio(on, &option.label).clicked() {
                    *change = Some(if on { ChoiceChange::Clear } else { ChoiceChange::Choose(vec![option.id.clone()]) });
                    ui.close();
                }
            }
        });
    }
    if options.is_empty() {
        ui.weak("—");
    }
    if spec.open {
        ui.separator();
        let edit = ui.add(egui::TextEdit::singleline(new_option).hint_text(spec.add_hint).desired_width(160.0));
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !new_option.trim().is_empty() {
            *change = Some(ChoiceChange::AddOption(std::mem::take(new_option)));
            edit.request_focus();
        }
    }
}

fn description_pane(ui: &mut Ui, state: &PropertySetState, props: &[Property], height: f32) {
    let visuals = ui.visuals().clone();
    let current = state.selected.as_deref().and_then(|id| props.iter().find(|p| p.id == id));
    let frame = egui::Frame::new().fill(visuals.widgets.noninteractive.weak_bg_fill.gamma_multiply(0.8)).corner_radius(crate::style::fit_radius(&visuals, 8.0) as u8).inner_margin(8);
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(height - 16.0);
        ui.set_max_height(height - 16.0);
        if let Some(p) = current {
            ui.label(RichText::new(&p.name).strong());
            if !p.description.is_empty() {
                ui.add(egui::Label::new(RichText::new(&p.description).weak()).truncate());
            }
            if !p.note.is_empty() {
                ui.add(egui::Label::new(weak_note(ui, &p.note)).truncate());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props() -> Vec<Property> {
        let mut a = Property::new("a", "Zeta", Kind::Text);
        a.group = "Général".into();
        let mut b = Property::new("b", "Alpha", Kind::Bool);
        b.group = "Avancé".into();
        let mut c = Property::new("c", "Beta", Kind::Number);
        c.group = "Général".into();
        let d = Property::new("d", "Delta", Kind::Text);
        vec![a, b, c, d]
    }

    fn names(items: &[Item]) -> Vec<String> {
        items.iter().map(|i| match i {
            Item::Header { group, count } => format!("[{group} {count}]"),
            Item::Row(p) => p.name.clone(),
        }).collect()
    }

    #[test]
    fn groups_keep_the_order_of_first_appearance_and_the_default_group_comes_where_it_appears() {
        let state = PropertySetState::default();
        let list = props();
        assert_eq!(names(&layout(&list, &state, "Divers")), vec!["[Général 2]", "Zeta", "Beta", "[Avancé 1]", "Alpha", "[Divers 1]", "Delta"]);
    }

    #[test]
    fn alphabetical_order_has_no_headers_and_ignores_case() {
        let state = PropertySetState { grouped: false, ..Default::default() };
        assert_eq!(names(&layout(&props(), &state, "Divers")), vec!["Alpha", "Beta", "Delta", "Zeta"]);
    }

    #[test]
    fn a_folded_group_shows_only_its_header_unless_a_filter_is_typed() {
        let mut state = PropertySetState::default();
        state.collapsed.insert("Général".into());
        assert_eq!(names(&layout(&props(), &state, "Divers")), vec!["[Général 2]", "[Avancé 1]", "Alpha", "[Divers 1]", "Delta"]);
        state.filter = "zet".into();
        assert_eq!(names(&layout(&props(), &state, "Divers")), vec!["[Général 1]", "Zeta"], "a filter opens the groups and drops the empty ones");
    }

    #[test]
    fn the_filter_reads_the_name_the_group_the_description_and_the_value() {
        let mut list = props();
        list[3].description = "le dernier".into();
        list[2].value = "42".into();
        let state = PropertySetState { filter: " DERNIER ".into(), grouped: false, ..Default::default() };
        assert_eq!(names(&layout(&list, &state, "Divers")), vec!["Delta"]);
        let state = PropertySetState { filter: "42".into(), grouped: false, ..Default::default() };
        assert_eq!(names(&layout(&list, &state, "Divers")), vec!["Beta"]);
    }
}
