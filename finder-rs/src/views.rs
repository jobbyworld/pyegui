//! The views of a folder: a table (list and tree) and a grid of tiles (icons).

use std::path::PathBuf;

use eframe::egui::{self, Align, RichText, Sense};
use egui_extras::{Column, TableBuilder};
use pyegui_core::icons;

use crate::actions::{self, ACTIONS};
use crate::app::{FinderApp, ViewMode};
use crate::model::{self, SortKey};
use crate::tagsui;

const ROW_HEIGHT: f32 = 24.0;
const INDENT: f32 = 16.0;

thread_local! {
    /// the first file seen of each extension: its icon is the one of every file of that extension
    static TYPE_ICONS: std::cell::RefCell<std::collections::HashMap<String, String>> = Default::default();
}

/// The icon of an item. The system draws one icon per path, which is slow (a few milliseconds
/// each, on the thread of the window): the files that have an extension share the icon of the
/// first one seen, so a folder of a thousand files makes a few requests, not a thousand
pub fn entry_icon(ctx: &egui::Context, e: &pyegui_core::watcher::Entry, size: f32) -> Option<String> {
    let path = &e.path;
    let shared = if e.is_dir { None } else { path.extension().map(|x| x.to_string_lossy().to_lowercase()) };
    let key = match shared {
        Some(ext) => TYPE_ICONS.with(|t| t.borrow_mut().entry(ext).or_insert_with(|| path.to_string_lossy().into_owned()).clone()),
        None => path.to_string_lossy().into_owned(),
    };
    icons::icon_uri(ctx, &key, size)
}

/// What the user did in a view during a frame, applied once the items are no longer borrowed
#[derive(Default)]
struct Outcome {
    clicked: Option<(usize, egui::Modifiers)>,
    right_clicked: Option<usize>,
    opened: Option<usize>,
    sort: Option<SortKey>,
    toggled: Option<PathBuf>,
    action: Option<usize>,
    tag: Option<(i64, bool)>,
    blank_click: bool,
}

impl Outcome {
    fn apply(self, app: &mut FinderApp, ctx: &egui::Context) {
        if let Some(key) = self.sort {
            app.sort_by(key);
        }
        if let Some(path) = self.toggled {
            app.toggle_folder(&path);
        }
        if self.blank_click {
            app.selected.clear();
        }
        if let Some(index) = self.right_clicked {
            // a right click on an item outside the selection selects it
            if !app.selected.contains(&app.items[index].path) {
                app.click_item(index, egui::Modifiers::NONE);
            }
        }
        if let Some((index, modifiers)) = self.clicked {
            app.click_item(index, modifiers);
        }
        if let Some(index) = self.opened {
            let path = app.items[index].path.clone();
            app.open(&path);
        }
        if let Some(action) = self.action {
            (ACTIONS[action].run)(app, ctx);
        }
        if let Some((tag, on)) = self.tag {
            let paths = app.selected_paths();
            if let Some(store) = app.tags.as_mut() {
                for path in &paths {
                    if on { store.tag(path, tag) } else { store.untag(path, tag) }
                }
            }
        }
    }
}

fn header_label(app: &FinderApp, key: SortKey, title: &str) -> String {
    if app.sort_key == key {
        format!("{title} {}", if app.ascending { "▲" } else { "▼" })
    } else {
        title.to_string()
    }
}

pub fn list(app: &mut FinderApp, ui: &mut egui::Ui) {
    let mut out = Outcome::default();
    let reveal = app.reveal.take();
    let ctx = ui.ctx().clone();
    let tree = app.view == ViewMode::Tree;
    let enabled = actions::enabled_flags(app);
    let tag_menu = crate::tagsui::menu_tags(app, ui.ctx());
    let has_selection = !app.selected.is_empty();

    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(Sense::click())
        .cell_layout(egui::Layout::left_to_right(Align::Center))
        .column(Column::remainder().at_least(160.0).clip(true))
        .column(Column::initial(140.0).at_least(80.0))
        .column(Column::initial(80.0).at_least(60.0))
        .column(Column::initial(140.0).at_least(80.0).clip(true))
        .column(Column::initial(90.0).at_least(40.0).clip(true))
        .column(Column::initial(140.0).at_least(80.0));
    if let Some(row) = reveal {
        table = table.scroll_to_row(row, None);
    }
    table
        .header(ROW_HEIGHT, |mut header| {
            for (key, title) in [(SortKey::Name, "Nom"), (SortKey::Modified, "Modifié"), (SortKey::Size, "Taille"), (SortKey::Kind, "Type")] {
                header.col(|ui| {
                    if ui.add(egui::Button::new(RichText::new(header_label(app, key, title)).strong()).frame(false)).clicked() {
                        out.sort = Some(key);
                    }
                });
            }
            header.col(|ui| {
                ui.label(RichText::new("Étiquettes").strong());
            });
            header.col(|ui| {
                ui.label(RichText::new("Dernière ouverture").strong());
            });
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, app.items.len(), |mut row| {
                let index = row.index();
                let e = &app.items[index];
                let depth = app.depths.get(index).copied().unwrap_or(0);
                row.set_selected(app.selected.contains(&e.path));
                row.col(|ui| {
                    if tree {
                        ui.add_space(depth as f32 * INDENT);
                        if e.is_dir {
                            let open = app.open_folders.contains(&e.path);
                            if ui.add(egui::Button::new(if open { "▾" } else { "▸" }).frame(false).min_size(egui::vec2(14.0, 0.0))).clicked() {
                                out.toggled = Some(e.path.clone());
                            }
                        } else {
                            ui.add_space(18.0);
                        }
                    }
                    if let Some(uri) = entry_icon(&ctx, e, 18.0) {
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
                row.col(|ui| {
                    tagsui::dots(ui, &app.row_tags(&e.path));
                });
                row.col(|ui| {
                    ui.add(egui::Label::new(RichText::new(model::format_date(app.opened_at(&e.path))).weak()).selectable(false));
                });
                let response = row.response();
                if response.double_clicked() {
                    out.opened = Some(index);
                } else if response.clicked() {
                    out.clicked = Some((index, ctx.input(|i| i.modifiers)));
                }
                if response.secondary_clicked() {
                    out.right_clicked = Some(index);
                }
                response.context_menu(|ui| {
                    if let Some(action) = actions::menu(ui, &enabled) {
                        out.action = Some(action);
                    }
                    ui.separator();
                    if let Some(change) = crate::tagsui::menu(ui, &tag_menu, has_selection) {
                        out.tag = Some(change);
                    }
                });
            });
        });
    out.apply(app, &ctx);
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
    let enabled = actions::enabled_flags(app);
    let tag_menu = crate::tagsui::menu_tags(app, ui.ctx());
    let has_selection = !app.selected.is_empty();
    let mut out = Outcome::default();

    let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
    if let Some(index) = reveal {
        // the row of the item is brought into view
        area = area.vertical_scroll_offset(((index / columns) as f32 * TILE_H - 2.0 * TILE_H).max(0.0));
    }
    let output = area.show_rows(ui, TILE_H, rows, |ui, range| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
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
                    if let Some(uri) = entry_icon(&ctx, e, ICON) {
                        egui::Image::new(uri).fit_to_exact_size(egui::vec2(ICON, ICON)).paint_at(ui, icon_rect);
                    }
                    let color = if selected { ui.visuals().selection.stroke.color } else { ui.visuals().text_color() };
                    let color = if e.hidden { color.gamma_multiply(0.5) } else { color };
                    // two lines at most: the end of a long name is cut
                    let mut job = egui::text::LayoutJob::single_section(e.name.clone(), egui::TextFormat::simple(egui::FontId::proportional(12.5), color));
                    job.wrap = egui::text::TextWrapping { max_width: TILE_W - 12.0, max_rows: 2, break_anywhere: true, overflow_character: Some('…') };
                    job.halign = Align::Center;
                    let galley = ui.painter().layout_job(job);
                    let pos = egui::pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 12.0 + ICON + 6.0);
                    ui.painter().galley(pos, galley, color);
                    if response.double_clicked() {
                        out.opened = Some(index);
                    } else if response.clicked() {
                        out.clicked = Some((index, ctx.input(|i| i.modifiers)));
                    }
                    if response.secondary_clicked() {
                        out.right_clicked = Some(index);
                    }
                    response.context_menu(|ui| {
                        if let Some(action) = actions::menu(ui, &enabled) {
                            out.action = Some(action);
                        }
                        ui.separator();
                        if let Some(change) = crate::tagsui::menu(ui, &tag_menu, has_selection) {
                            out.tag = Some(change);
                        }
                    });
                }
            });
        }
    });
    // a click in the empty space clears the selection
    let background = ui.interact(output.inner_rect, ui.id().with("icons-background"), Sense::click());
    if background.clicked() && out.clicked.is_none() && out.opened.is_none() {
        out.blank_click = true;
    }
    out.apply(app, &ctx);
    columns
}
