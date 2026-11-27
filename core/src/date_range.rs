//! A date range picker: a button that shows the range and opens a calendar. A range can be open: from a
//! date on (>= a date, no end), up to a date (<= a date, no start), or between two dates.

use chrono::{Datelike, Duration, NaiveDate};
use egui::{Id, Response, RichText, Ui};

/// A range of days, both ends included. An end that is `None` is open (infinite).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DateRange {
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
}

impl DateRange {
    pub fn new(start: Option<NaiveDate>, end: Option<NaiveDate>) -> Self {
        DateRange { start, end }.ordered()
    }

    /// The ends in the right order (the start is not after the end)
    pub fn ordered(self) -> Self {
        match (self.start, self.end) {
            (Some(s), Some(e)) if s > e => DateRange { start: Some(e), end: Some(s) },
            _ => self,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start.is_none() && self.end.is_none()
    }

    pub fn contains(&self, day: NaiveDate) -> bool {
        self.start.is_none_or(|s| day >= s) && self.end.is_none_or(|e| day <= e)
    }

    /// "12/03/2026 → 20/03/2026", "≥ 12/03/2026", "≤ 20/03/2026" or "Toutes les dates"
    pub fn human_daterange_label(&self) -> String {
        let day = |d: NaiveDate| d.format("%d/%m/%Y").to_string();
        match (self.start, self.end) {
            (Some(s), Some(e)) if s == e => day(s),
            (Some(s), Some(e)) => format!("{} → {}", day(s), day(e)),
            (Some(s), None) => format!("≥ {}", day(s)),
            (None, Some(e)) => format!("≤ {}", day(e)),
            (None, None) => "Toutes les dates".to_string(),
        }
    }
}

/// What a click on a day does
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    /// the first click is the start, the second the end
    #[default]
    Between,
    /// a click is the start, the range has no end: >= a date
    From,
    /// a click is the end, the range has no start: <= a date
    Until,
}

#[derive(Clone, Default)]
struct Popup {
    /// the first day of the month shown
    month: Option<NaiveDate>,
    mode: Mode,
    /// the start already clicked in `Between`, waiting for the end
    pending: Option<NaiveDate>,
    /// the day under the pointer, to draw the range being made
    hover: Option<NaiveDate>,
    /// what is typed in the field of the popup
    text: String,
}

fn first_of_month(day: NaiveDate) -> NaiveDate {
    day.with_day(1).unwrap_or(day)
}

fn add_months(first: NaiveDate, months: i32) -> NaiveDate {
    let index = first.year() * 12 + first.month0() as i32 + months;
    NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1).unwrap_or(first)
}

const MONTHS: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];
const DAYS: [&str; 7] = ["lu", "ma", "me", "je", "ve", "sa", "di"];

/// The button of the range and its calendar. Returns the response of the button, `changed()` when the range
/// was changed during this frame. `id` tells apart the pickers of a ui.
pub fn date_range_picker(ui: &mut Ui, id: &str, range: &mut DateRange) -> Response {
    let id = Id::unique(("date_range_picker", id));
    let mut response = ui.button(format!("📅 {}", range.human_daterange_label()));
    let open_id = id.with("open");
    if response.clicked() {
        let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
        ui.data_mut(|d| d.insert_temp(open_id, !open));
        ui.data_mut(|d| d.remove::<Popup>(id));
    }
    let before = *range;
    if ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) {
        let mut popup: Popup = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        let area = egui::Area::new(id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(response.rect.left_bottom() + egui::vec2(0.0, 2.0))
            .show(ui.ctx(), |ui| {
                crate::popup_style::menu_frame(ui).show(ui, |ui| calendar(ui, &mut popup, range));
            });
        ui.data_mut(|d| d.insert_temp(id, popup));
        if area.response.clicked_elsewhere() && !response.clicked() {
            ui.data_mut(|d| d.insert_temp(open_id, false));
        }
    }
    if *range != before {
        response.mark_changed();
    }
    response
}

/// The calendar with its field, drawn where it is (not in a popup of its own): for a place that is already
/// a popup, like the filter of a column. Returns true when the range changed in this frame.
pub fn date_range_inline(ui: &mut Ui, id: &str, range: &mut DateRange) -> bool {
    let id = Id::unique(("date_range_inline", id));
    let mut popup: Popup = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let before = *range;
    calendar(ui, &mut popup, range);
    ui.data_mut(|d| d.insert_temp(id, popup));
    *range != before
}

fn calendar(ui: &mut Ui, popup: &mut Popup, range: &mut DateRange) {
    ui.set_width(250.0);
    let today = chrono::Local::now().date_naive();
    let shown = *popup.month.get_or_insert_with(|| first_of_month(range.start.or(range.end).unwrap_or(today)));

    // the field: a range said in French, applied with Enter ("cette semaine", "depuis lundi", "12/03 au 20/03")
    let typed = ui.add(egui::TextEdit::singleline(&mut popup.text).hint_text("ex : cette semaine, depuis lundi").desired_width(f32::INFINITY));
    let understood = if popup.text.trim().is_empty() { None } else { crate::date_input::parse_any_range(&popup.text, today) };
    if !popup.text.trim().is_empty() {
        match understood {
            Some((start, end)) => {
                ui.weak(format!("↵  {}", DateRange { start, end }.human_daterange_label()));
            }
            None => {
                ui.colored_label(egui::Color32::from_rgb(239, 68, 68), "Date non comprise");
            }
        }
    }
    if typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        if let Some((start, end)) = understood {
            *range = DateRange::new(start, end);
            popup.pending = None;
            popup.month = Some(first_of_month(range.start.or(range.end).unwrap_or(today)));
            popup.text.clear();
        }
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for (mode, text) in [(Mode::Between, "Entre"), (Mode::From, "À partir de"), (Mode::Until, "Jusqu'à")] {
            if ui.selectable_label(popup.mode == mode, text).clicked() {
                popup.mode = mode;
                popup.pending = None;
                // the open end is made now: it does not wait for a click
                match mode {
                    Mode::From => range.end = None,
                    Mode::Until => range.start = None,
                    Mode::Between => {}
                }
            }
        }
    });
    ui.separator();
    ui.horizontal(|ui| {
        if ui.small_button("«").clicked() {
            popup.month = Some(add_months(shown, -12));
        }
        if ui.small_button("‹").clicked() {
            popup.month = Some(add_months(shown, -1));
        }
        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
            ui.label(RichText::new(format!("{} {}", MONTHS[shown.month0() as usize], shown.year())).strong());
        });
    });
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("»").clicked() {
                popup.month = Some(add_months(shown, 12));
            }
            if ui.small_button("›").clicked() {
                popup.month = Some(add_months(shown, 1));
            }
            if ui.small_button("Aujourd'hui").clicked() {
                popup.month = Some(first_of_month(today));
            }
        });
    });

    // the range as it would be with the day under the pointer (the end of a range being made)
    let preview = match (popup.mode, popup.pending, popup.hover) {
        (Mode::Between, Some(start), Some(hover)) => DateRange::new(Some(start), Some(hover)),
        (Mode::Between, Some(start), None) => DateRange::new(Some(start), Some(start)),
        _ => *range,
    };

    let selection = ui.visuals().selection.bg_fill;
    let faint = selection.gamma_multiply(0.35);
    let offset = shown.weekday().num_days_from_monday() as i64;
    let mut clicked = None;
    let mut hover = None;
    egui::Grid::new("days").spacing(egui::vec2(2.0, 2.0)).min_col_width(32.0).show(ui, |ui| {
        for name in DAYS {
            ui.vertical_centered(|ui| ui.weak(name));
        }
        ui.end_row();
        for cell in 0..42i64 {
            let day = shown + Duration::days(cell - offset);
            let in_month = day.month() == shown.month();
            let is_end = preview.start == Some(day) || preview.end == Some(day);
            let inside = preview.contains(day) && !preview.is_empty();
            let mut text = RichText::new(day.day().to_string());
            if !in_month {
                text = text.weak();
            }
            if is_end {
                text = text.color(ui.visuals().selection.stroke.color).strong();
            }
            let fill = if is_end {
                selection
            } else if inside {
                faint
            } else {
                egui::Color32::TRANSPARENT
            };
            let mut button = egui::Button::new(text).fill(fill).min_size(egui::vec2(32.0, 24.0));
            if day == today {
                button = button.stroke(egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color));
            }
            let response = ui.add(button);
            if response.hovered() {
                hover = Some(day);
            }
            if response.clicked() {
                clicked = Some(day);
            }
            if cell % 7 == 6 {
                ui.end_row();
            }
        }
    });
    popup.hover = hover;

    if let Some(day) = clicked {
        match popup.mode {
            Mode::From => *range = DateRange { start: Some(day), end: None },
            Mode::Until => *range = DateRange { start: None, end: Some(day) },
            Mode::Between => match popup.pending.take() {
                None => {
                    popup.pending = Some(day);
                    *range = DateRange { start: Some(day), end: Some(day) };
                }
                Some(start) => *range = DateRange::new(Some(start), Some(day)),
            },
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new(range.human_daterange_label()).weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Effacer").clicked() {
                *range = DateRange::default();
                popup.pending = None;
            }
        });
    });
    if popup.mode == Mode::Between && popup.pending.is_some() {
        ui.weak("Cliquez la date de fin");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn the_ends_are_put_in_order() {
        let r = DateRange::new(Some(day(2026, 3, 20)), Some(day(2026, 3, 12)));
        assert_eq!((r.start, r.end), (Some(day(2026, 3, 12)), Some(day(2026, 3, 20))));
    }

    #[test]
    fn an_open_end_has_no_limit() {
        let from = DateRange::new(Some(day(2026, 3, 12)), None);
        assert!(from.contains(day(2999, 1, 1)) && !from.contains(day(2026, 3, 11)));
        let until = DateRange::new(None, Some(day(2026, 3, 12)));
        assert!(until.contains(day(1970, 1, 1)) && !until.contains(day(2026, 3, 13)));
        assert!(DateRange::default().contains(day(2000, 1, 1)));
    }

    #[test]
    fn the_label_tells_the_range() {
        assert_eq!(DateRange::new(Some(day(2026, 3, 12)), None).human_daterange_label(), "≥ 12/03/2026");
        assert_eq!(DateRange::new(None, Some(day(2026, 3, 12))).human_daterange_label(), "≤ 12/03/2026");
        assert_eq!(DateRange::default().human_daterange_label(), "Toutes les dates");
        assert_eq!(DateRange::new(Some(day(2026, 3, 12)), Some(day(2026, 3, 20))).human_daterange_label(), "12/03/2026 → 20/03/2026");
    }

    #[test]
    fn months_are_added_across_years() {
        assert_eq!(add_months(day(2026, 11, 1), 3), day(2027, 2, 1));
        assert_eq!(add_months(day(2026, 1, 1), -1), day(2025, 12, 1));
    }
}
