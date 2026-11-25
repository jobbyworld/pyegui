//! The cards of the list panes (Récents, Index): the icon, the name and, on the right, the date; under
//! them, in a smaller font, the folder and what is known of the item. A card is clickable as a whole.

use std::path::Path;

use eframe::egui::{self, Align, Layout, RichText};

/// The date as the views of the Finder say it (`pyegui_core::human_date`): "à l'instant", "il y a 5 min",
/// "Aujourd'hui à 14:32", "Hier à 09:10", "lundi dernier à 09:10", "12 mars à 09:10", "12 mars 2025".
/// `when` and `now` are seconds since the epoch, the days are the ones of the local time zone.
pub fn human_date(when: f64, now: f64) -> String {
    use chrono::TimeZone;
    let local = |s: f64| chrono::Local.timestamp_opt(s as i64, 0).single().map(|t| t.naive_local());
    match (local(when), local(now)) {
        (Some(at), Some(now)) => pyegui_core::human_date::human_datetime(at, now),
        _ => String::new(),
    }
}

pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// What a card shows
pub struct Card<'a> {
    pub path: &'a Path,
    pub is_dir: bool,
    pub name: &'a str,
    pub date: String,
    /// the lines under the name, the first one in the normal color, the others weaker
    pub lines: Vec<String>,
}

const MARGIN: f32 = 8.0;
const ICON: f32 = 20.0;

/// Draws the card and returns its response (click, double click, hover). `height` fixes the height of the
/// card, for the lists that only draw the cards in view.
pub fn show(ui: &mut egui::Ui, card: &Card, selected: bool, height: Option<f32>) -> egui::Response {
    let visuals = ui.visuals();
    let text = visuals.text_color();
    let fill = text.gamma_multiply(if selected { 0.16 } else { 0.07 });
    let stroke = visuals.widgets.noninteractive.bg_stroke;
    let width = ui.available_width();
    let shown = egui::Frame::new().fill(fill).stroke(stroke).corner_radius(8.0).inner_margin(MARGIN).show(ui, |ui| {
        ui.set_width(width - 2.0 * MARGIN - 2.0 * stroke.width);
        if let Some(h) = height {
            ui.set_height(h - 2.0 * MARGIN - 2.0 * stroke.width);
        }
        ui.horizontal(|ui| {
            if let Some(uri) = pyegui_core::icons::entry_icon_uri(ui.ctx(), card.path, card.is_dir, ICON) {
                ui.add(egui::Image::new(uri).fit_to_exact_size(egui::vec2(ICON, ICON)));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(&card.date).small().weak());
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(card.name).truncate().selectable(false));
                });
            });
        });
        for (i, line) in card.lines.iter().enumerate() {
            let rich = RichText::new(line).small();
            ui.add(egui::Label::new(if i == 0 { rich } else { rich.weak() }).truncate().selectable(false));
        }
    });
    let response = ui.interact(shown.response.rect, ui.make_persistent_id(("card", card.path)), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(shown.response.rect, 8.0, text.gamma_multiply(0.06));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// An empty card with a text, clickable ("Charger plus")
pub fn empty(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let width = ui.available_width();
    let shown = egui::Frame::new().stroke(stroke).corner_radius(8.0).inner_margin(MARGIN).show(ui, |ui| {
        ui.set_width(width - 2.0 * MARGIN - 2.0 * stroke.width);
        ui.vertical_centered(|ui| ui.label(RichText::new(text).weak()));
    });
    let response = ui.interact(shown.response.rect, ui.make_persistent_id(("empty-card", text)), egui::Sense::click());
    if response.hovered() {
        let color = ui.visuals().text_color().gamma_multiply(0.06);
        ui.painter().rect_filled(shown.response.rect, 8.0, color);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> f64 {
        chrono::Local.with_ymd_and_hms(y, m, d, h, min, 0).unwrap().timestamp() as f64
    }

    #[test]
    fn dates_are_told_in_french() {
        let now = at(2026, 11, 25, 15, 0);
        assert_eq!(human_date(now - 20.0, now), "à l'instant");
        assert_eq!(human_date(now - 5.0 * 60.0, now), "il y a 5 min");
        assert_eq!(human_date(at(2026, 11, 25, 9, 0), now), "Aujourd'hui à 09:00");
        assert_eq!(human_date(at(2026, 11, 24, 23, 0), now), "Hier à 23:00");
        assert_eq!(human_date(at(2026, 11, 3, 12, 0), now), "3 novembre à 12:00");
        assert_eq!(human_date(at(2025, 2, 3, 12, 0), now), "3 février 2025");
    }
}
