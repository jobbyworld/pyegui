//! Settings window: the frame shared by the configuration windows of an application. A title with its close
//! cross, a list at the left (like a sidebar: rows that can be moved by drag and drop, a splitter to resize
//! it, a button at its foot to add one), the panel of the item chosen at the right, and a foot with the buttons
//! of the window always in the same place. The content of the panel and of the foot is given by the caller.

use egui::{
    self, pos2, vec2, Align, Align2, Color32, Context, CursorIcon, FontId, Frame, Id, Layout, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder,
};

/// A row of the list
#[derive(Clone, Debug, Default)]
pub struct SettingsItem {
    pub id: String,
    pub label: String,
    /// a small text at the right of the row (a type, a count...)
    pub hint: String,
}

/// What lives between two frames: the width of the list and the row being moved
#[derive(Clone, Debug)]
pub struct SettingsState {
    pub list_width: f32,
    dragging: Option<usize>,
    /// what the panel showed at the last frame: it starts again from its top when that changes
    shown: Option<(Option<String>, bool)>,
}

impl Default for SettingsState {
    fn default() -> Self {
        SettingsState { list_width: 210.0, dragging: None, shown: None }
    }
}

#[derive(Clone, Debug)]
pub struct SettingsOptions {
    pub id: Id,
    pub title: String,
    /// a line under the title
    pub subtitle: String,
    /// the small heading over the list
    pub list_title: String,
    /// the button at the foot of the list ("＋ " is put before it)
    pub add_label: String,
    pub close_label: String,
    pub empty_text: String,
    /// the size of the window: nothing inside may ask for more
    pub width: f32,
    /// the height of the list and of the panel
    pub height: f32,
    /// the rows can be moved by drag and drop
    pub reorderable: bool,
    /// the panel is the one of a new item: the add button is lit
    pub adding: bool,
    /// the look of the window, for the apps that have their own popups
    pub frame: Option<Frame>,
}

impl SettingsOptions {
    pub fn new(id: Id) -> Self {
        SettingsOptions {
            id,
            title: String::new(),
            subtitle: String::new(),
            list_title: String::new(),
            add_label: "Nouveau".into(),
            close_label: "Fermer".into(),
            empty_text: "Aucun élément".into(),
            width: 720.0,
            height: 400.0,
            reorderable: true,
            adding: false,
            frame: None,
        }
    }
}

/// What happened in the window
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsEvent {
    /// a row was clicked (or taken to be moved)
    Select(String),
    /// the row at `from` goes to `to`: the caller reorders its items
    Move { from: usize, to: usize },
    /// the add button was clicked: the panel shows an item to create
    Add,
    /// the cross, the close button or Escape
    Close,
}

const ROW_H: f32 = 32.0;
const SPLIT_W: f32 = 9.0;
const HEAD_H: f32 = 56.0;
const FOOT_H: f32 = 46.0;

/// Draws the window (a modal one) and returns what happened. `detail` fills the panel at the right (the item
/// chosen, or the form of a new one), `footer` the left of the foot; the close button is at its right.
pub fn settings_window(
    ctx: &Context,
    state: &mut SettingsState,
    o: &SettingsOptions,
    items: &[SettingsItem],
    selected: Option<&str>,
    detail: impl FnOnce(&mut Ui),
    footer: impl FnOnce(&mut Ui),
) -> Vec<SettingsEvent> {
    let mut events = Vec::new();
    let style = ctx.global_style();
    let frame = o.frame.unwrap_or_else(|| Frame::popup(&style));
    let modal = egui::Modal::new(o.id).frame(frame).show(ctx, |ui| {
        ui.set_width(o.width);
        ui.set_max_width(o.width);
        // the head: the title and its cross
        ui.allocate_ui(vec2(o.width, HEAD_H), |ui| {
            ui.horizontal(|ui| {
                ui.heading(&o.title);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let cross = ui.add(egui::Button::new(RichText::new("✕").size(15.0)).frame(false).min_size(vec2(26.0, 26.0)));
                    if cross.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("Fermer (Échap)").clicked() {
                        events.push(SettingsEvent::Close);
                    }
                });
            });
            if !o.subtitle.is_empty() {
                ui.add(egui::Label::new(RichText::new(&o.subtitle).weak().small()).wrap());
            }
        });
        ui.add_space(2.0);
        ui.separator();
        // the body: the list, the splitter, the panel
        let max_list = (o.width * 0.5).max(160.0);
        state.list_width = state.list_width.clamp(150.0, max_list);
        let (_, body) = ui.allocate_space(vec2(o.width, o.height));
        let list = Rect::from_min_size(body.min, vec2(state.list_width, o.height));
        let split = Rect::from_min_size(pos2(list.right(), body.top()), vec2(SPLIT_W, o.height));
        let panel = Rect::from_min_max(pos2(split.right(), body.top()), body.max);

        ui.scope_builder(UiBuilder::new().max_rect(list), |ui| {
            list_ui(ui, state, o, items, selected, &mut events);
        });

        let grip = ui.interact(split, o.id.with("split"), Sense::drag());
        if grip.hovered() || grip.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        if grip.dragged() {
            state.list_width = (state.list_width + grip.drag_delta().x).clamp(150.0, max_list);
        }
        let line = if grip.hovered() || grip.dragged() { ui.visuals().selection.bg_fill } else { ui.visuals().widgets.noninteractive.bg_stroke.color };
        let x = split.center().x;
        ui.painter().line_segment([pos2(x, split.top() + 2.0), pos2(x, split.bottom() - 2.0)], Stroke::new(if grip.dragged() { 2.0 } else { 1.0 }, line));

        ui.scope_builder(UiBuilder::new().max_rect(panel.shrink2(vec2(6.0, 0.0))), |ui| {
            let now = (selected.map(str::to_owned), o.adding);
            let changed = state.shown.as_ref() != Some(&now);
            state.shown = Some(now);
            let mut area = ScrollArea::vertical().id_salt(o.id.with("detail")).auto_shrink([false, false]);
            if changed {
                area = area.vertical_scroll_offset(0.0);
            }
            area.show(ui, |ui| {
                ui.set_width(ui.available_width());
                detail(ui);
            });
        });

        // the foot: what the window needs at the left, the close button at the right
        ui.separator();
        ui.allocate_ui(vec2(o.width, FOOT_H - 8.0), |ui| {
            ui.horizontal_centered(|ui| {
                footer(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(egui::Button::new(&o.close_label).min_size(vec2(92.0, 28.0))).clicked() {
                        events.push(SettingsEvent::Close);
                    }
                });
            });
        });
    });
    if modal.should_close() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        events.push(SettingsEvent::Close);
    }
    events
}

fn list_ui(ui: &mut Ui, state: &mut SettingsState, o: &SettingsOptions, items: &[SettingsItem], selected: Option<&str>, events: &mut Vec<SettingsEvent>) {
    let add_h = 34.0;
    let head_h = if o.list_title.is_empty() { 0.0 } else { 22.0 };
    if head_h > 0.0 {
        ui.allocate_ui(vec2(ui.available_width(), head_h), |ui| {
            ui.label(RichText::new(o.list_title.to_uppercase()).weak().small());
        });
    }
    let rows_h = (ui.available_height() - add_h - 6.0).max(40.0);
    let mut rects: Vec<Rect> = Vec::with_capacity(items.len());
    ui.allocate_ui(vec2(ui.available_width(), rows_h), |ui| {
        ScrollArea::vertical().id_salt(o.id.with("list")).auto_shrink([false, false]).show(ui, |ui| {
            if items.is_empty() {
                ui.add_space(6.0);
                ui.weak(&o.empty_text);
            }
            for (i, item) in items.iter().enumerate() {
                let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
                rects.push(rect);
                let is_selected = !o.adding && selected == Some(item.id.as_str());
                let moving = state.dragging == Some(i);
                let visuals = ui.visuals();
                let painter = ui.painter();
                if is_selected || moving {
                    painter.rect_filled(rect, 7.0, visuals.selection.bg_fill.gamma_multiply(if moving { 0.9 } else { 0.6 }));
                } else if response.hovered() && state.dragging.is_none() {
                    painter.rect_filled(rect, 7.0, visuals.widgets.hovered.weak_bg_fill);
                }
                let weak = visuals.weak_text_color();
                let mut left = rect.left() + 10.0;
                if o.reorderable {
                    let c = pos2(rect.left() + 11.0, rect.center().y);
                    let dots = if response.hovered() || moving { weak } else { weak.gamma_multiply(0.45) };
                    for dx in [-2.5f32, 2.5] {
                        for dy in [-4.5f32, 0.0, 4.5] {
                            painter.circle_filled(c + vec2(dx, dy), 1.3, dots);
                        }
                    }
                    left = rect.left() + 24.0;
                }
                let hint = painter.layout_no_wrap(item.hint.clone(), FontId::proportional(11.0), weak);
                let hint_w = if item.hint.is_empty() { 0.0 } else { hint.size().x + 12.0 };
                let color = if is_selected { visuals.strong_text_color() } else { visuals.text_color() };
                let label = painter.layout_no_wrap(item.label.clone(), FontId::proportional(14.0), color);
                let clip = Rect::from_min_max(pos2(left, rect.top()), pos2(rect.right() - hint_w - 4.0, rect.bottom()));
                painter.with_clip_rect(clip).galley(pos2(left, rect.center().y - label.size().y / 2.0), label, color);
                if hint_w > 0.0 {
                    painter.galley(pos2(rect.right() - hint_w + 2.0, rect.center().y - hint.size().y / 2.0), hint, weak);
                }
                if o.reorderable && (response.hovered() || moving) {
                    ui.ctx().set_cursor_icon(if moving { CursorIcon::Grabbing } else { CursorIcon::Grab });
                }
                if response.drag_started() && o.reorderable {
                    state.dragging = Some(i);
                    events.push(SettingsEvent::Select(item.id.clone()));
                } else if response.clicked() {
                    events.push(SettingsEvent::Select(item.id.clone()));
                }
            }
        });
    });
    // the row follows the pointer
    if let Some(from) = state.dragging {
        if ui.input(|i| i.pointer.any_down()) {
            if let (Some(p), true) = (ui.input(|i| i.pointer.interact_pos()), !rects.is_empty()) {
                let to = if p.y < rects[0].top() {
                    0
                } else if p.y > rects[rects.len() - 1].bottom() {
                    rects.len() - 1
                } else {
                    rects.iter().position(|r| r.y_range().contains(p.y)).unwrap_or(from)
                };
                if to != from && to < items.len() {
                    events.push(SettingsEvent::Move { from, to });
                    state.dragging = Some(to);
                }
            }
            ui.ctx().request_repaint();
        } else {
            state.dragging = None;
        }
    }
    ui.add_space(6.0);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), add_h - 4.0), Sense::click());
    let visuals = ui.visuals();
    if o.adding {
        ui.painter().rect_filled(rect, 7.0, visuals.selection.bg_fill.gamma_multiply(0.6));
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 7.0, visuals.widgets.hovered.weak_bg_fill);
    }
    ui.painter().rect_stroke(rect, 7.0, Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color), StrokeKind::Inside);
    let color: Color32 = if o.adding { visuals.strong_text_color() } else { visuals.text_color() };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, format!("＋  {}", o.add_label), FontId::proportional(14.0), color);
    if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        events.push(SettingsEvent::Add);
    }
}
