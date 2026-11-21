//! The two views of a folder: a table (list) and a grid of tiles (icons).

use eframe::egui::{self, Align, RichText, Sense};
use egui_extras::{Column, TableBuilder};
use pyegui_core::icons;

use crate::app::FinderApp;
use crate::model::{self, SortKey};

const ROW_HEIGHT: f32 = 24.0;

fn header_label(app: &FinderApp, key: SortKey, title: &str) -> String {
    if app.sort_key == key {
        format!("{title} {}", if app.ascending { "▲" } else { "▼" })
    } else {
        title.to_string()
    }
}

pub fn list(app: &mut FinderApp, ui: &mut egui::Ui) {
    let mut clicked: Option<(usize, egui::Modifiers)> = None;
    let mut opened: Option<usize> = None;
    let mut sort: Option<SortKey> = None;
    let reveal = app.reveal.take();
    let ctx = ui.ctx().clone();

    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(Sense::click())
        .cell_layout(egui::Layout::left_to_right(Align::Center))
        .column(Column::remainder().at_least(160.0).clip(true))
        .column(Column::initial(140.0).at_least(80.0))
        .column(Column::initial(80.0).at_least(60.0))
        .column(Column::initial(140.0).at_least(80.0).clip(true));
    if let Some(row) = reveal {
        table = table.scroll_to_row(row, None);
    }
    table
        .header(ROW_HEIGHT, |mut header| {
            for (key, title) in [(SortKey::Name, "Nom"), (SortKey::Modified, "Modifié"), (SortKey::Size, "Taille"), (SortKey::Kind, "Type")] {
                header.col(|ui| {
                    if ui.add(egui::Button::new(RichText::new(header_label(app, key, title)).strong()).frame(false)).clicked() {
                        sort = Some(key);
                    }
                });
            }
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, app.items.len(), |mut row| {
                let index = row.index();
                let e = &app.items[index];
                row.set_selected(app.selected.contains(&e.path));
                row.col(|ui| {
                    if let Some(uri) = icons::icon_uri(&ctx, &e.path.to_string_lossy(), 18.0) {
                        ui.add(egui::Image::new(uri).fit_to_exact_size(egui::vec2(18.0, 18.0)));
                    }
                    let name = RichText::new(&e.name);
                    ui.add(egui::Label::new(if e.hidden { name.weak() } else { name }).truncate().selectable(false));
                });
                row.col(|ui| {
                    ui.add(egui::Label::new(RichText::new(model::format_date(e.modified)).weak()).selectable(false));
                });
                row.col(|ui| {
                    if !e.is_dir {
                        ui.add(egui::Label::new(RichText::new(model::human_size(e.size)).weak()).selectable(false));
                    }
                });
                row.col(|ui| {
                    ui.add(egui::Label::new(RichText::new(model::kind_of(e)).weak()).truncate().selectable(false));
                });
                let response = row.response();
                if response.double_clicked() {
                    opened = Some(index);
                } else if response.clicked() {
                    clicked = Some((index, ctx.input(|i| i.modifiers)));
                }
            });
        });

    if let Some(key) = sort {
        app.sort_by(key);
    }
    if let Some((index, modifiers)) = clicked {
        app.click_item(index, modifiers);
    }
    if let Some(index) = opened {
        let path = app.items[index].path.clone();
        app.open(&path);
    }
}

/// Draws the tiles and returns how many there are in a row (for the arrows of the keyboard)
pub fn icons(app: &mut FinderApp, ui: &mut egui::Ui) -> usize {
    const TILE_W: f32 = 104.0;
    const TILE_H: f32 = 108.0;
    const ICON: f32 = 56.0;
    let width = ui.available_width();
    let columns = ((width / TILE_W).floor() as usize).max(1);
    let rows = app.items.len().div_ceil(columns);
    let reveal = app.reveal.take();
    let ctx = ui.ctx().clone();
    let mut clicked: Option<(usize, egui::Modifiers)> = None;
    let mut opened: Option<usize> = None;
    let mut blank_click = false;

    let area = egui::ScrollArea::vertical().auto_shrink([false, false]);
    let output = area.show_rows(ui, TILE_H, rows, |ui, range| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        if let Some(index) = reveal {
            let row = index / columns;
            if !range.contains(&row) {
                ui.scroll_to_rect(
                    egui::Rect::from_min_size(ui.min_rect().min + egui::vec2(0.0, (row as f32 - range.start as f32) * TILE_H), egui::vec2(TILE_W, TILE_H)),
                    None,
                );
            }
        }
        for row in range {
            ui.horizontal(|ui| {
                for column in 0..columns {
                    let index = row * columns + column;
                    let Some(e) = app.items.get(index) else { break };
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(TILE_W, TILE_H), Sense::click());
                    let selected = app.selected.contains(&e.path);
                    if selected {
                        ui.painter().rect_filled(rect.shrink(3.0), 8.0, ui.visuals().selection.bg_fill);
                    } else if response.hovered() {
                        ui.painter().rect_filled(rect.shrink(3.0), 8.0, ui.visuals().widgets.hovered.weak_bg_fill);
                    }
                    let icon_rect = egui::Rect::from_center_size(rect.center_top() + egui::vec2(0.0, 12.0 + ICON / 2.0), egui::vec2(ICON, ICON));
                    if let Some(uri) = icons::icon_uri(&ctx, &e.path.to_string_lossy(), ICON) {
                        egui::Image::new(uri).fit_to_exact_size(egui::vec2(ICON, ICON)).paint_at(ui, icon_rect);
                    }
                    let color = if selected { ui.visuals().selection.stroke.color } else { ui.visuals().text_color() };
                    let color = if e.hidden { color.gamma_multiply(0.5) } else { color };
                    let galley = ui.painter().layout(e.name.clone(), egui::FontId::proportional(12.5), color, TILE_W - 12.0);
                    let galley = if galley.rows.len() > 2 {
                        // two lines at most: the end of the name is cut
                        let mut job = egui::text::LayoutJob::single_section(e.name.clone(), egui::TextFormat::simple(egui::FontId::proportional(12.5), color));
                        job.wrap = egui::text::TextWrapping { max_width: TILE_W - 12.0, max_rows: 2, break_anywhere: true, overflow_character: Some('…') };
                        job.halign = Align::Center;
                        ui.painter().layout_job(job)
                    } else {
                        let mut job = egui::text::LayoutJob::single_section(e.name.clone(), egui::TextFormat::simple(egui::FontId::proportional(12.5), color));
                        job.wrap.max_width = TILE_W - 12.0;
                        job.halign = Align::Center;
                        ui.painter().layout_job(job)
                    };
                    let pos = egui::pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 12.0 + ICON + 6.0);
                    ui.painter().galley(pos, galley, color);
                    if response.double_clicked() {
                        opened = Some(index);
                    } else if response.clicked() {
                        clicked = Some((index, ctx.input(|i| i.modifiers)));
                    }
                }
            });
        }
    });
    // a click in the empty space clears the selection
    let background = ui.interact(output.inner_rect, ui.id().with("icons-background"), Sense::click());
    if background.clicked() && clicked.is_none() {
        blank_click = true;
    }
    if blank_click {
        app.selected.clear();
    }
    if let Some((index, modifiers)) = clicked {
        app.click_item(index, modifiers);
    }
    if let Some(index) = opened {
        let path = app.items[index].path.clone();
        app.open(&path);
    }
    columns
}
