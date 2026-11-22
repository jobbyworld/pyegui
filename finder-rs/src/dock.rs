//! The dock on the right: panes in tabs that can be closed, moved by their tab and brought back
//! from the + menu. The layout is kept between runs.

use eframe::egui::{self, RichText};
use egui_tiles::{Container, Tile, TileId, Tiles, Tree, UiResponse};
use serde::{Deserialize, Serialize};

use crate::actions::{self, ACTIONS};
use crate::app::FinderApp;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Pane {
    Preview,
    Actions,
    Appearance,
    Activity,
    Tags,
    Recent,
    Search,
}

pub const PANES: [Pane; 7] = [Pane::Preview, Pane::Actions, Pane::Tags, Pane::Recent, Pane::Search, Pane::Appearance, Pane::Activity];

impl Pane {
    pub fn title(self) -> &'static str {
        match self {
            Pane::Preview => "Aperçu",
            Pane::Actions => "Actions",
            Pane::Appearance => "Apparence",
            Pane::Activity => "Activités",
            Pane::Tags => "Étiquettes",
            Pane::Recent => "Récents",
            Pane::Search => "Recherche",
        }
    }
}

fn default_tree() -> Tree<Pane> {
    Tree::new_tabs(egui::Id::new("finder-dock"), PANES.to_vec())
}

/// The layout saved, or the default one when there is none or it does not read
pub fn restore(saved: Option<&str>) -> Tree<Pane> {
    saved.and_then(|s| serde_json::from_str::<Tree<Pane>>(s).ok()).filter(|t| t.root().is_some()).unwrap_or_else(default_tree)
}

fn present(tiles: &Tiles<Pane>) -> Vec<Pane> {
    tiles.iter().filter_map(|(_, t)| if let Tile::Pane(p) = t { Some(*p) } else { None }).collect()
}

/// The sheet of the dock: what the panes and their active tab are filled with
fn sheet(visuals: &egui::Visuals) -> egui::Color32 {
    // the fill of the panel of the dock, so the active tab and its pane are one piece
    visuals.panel_fill
}

/// The outline of a tab, open at the bottom so it joins its pane
fn tab_outline(rect: egui::Rect, radius: f32) -> Vec<egui::Pos2> {
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
    corner(rect.left_top() + egui::vec2(radius, radius), 2.0 * FRAC_PI_2);
    corner(rect.right_top() + egui::vec2(-radius, radius), 3.0 * FRAC_PI_2);
    points.push(rect.right_bottom());
    points
}

struct Behavior<'a> {
    app: &'a mut FinderApp,
}

impl egui_tiles::Behavior<Pane> for Behavior<'_> {
    // the title of a pane like the title bar of a dock of pyfinder: small, semi bold, muted
    // (the active one in the color of the text), on the color of the panel, a thin line below
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        RichText::new(pane.title()).small().strong().into()
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        28.0
    }

    fn tab_title_spacing(&self, _visuals: &egui::Visuals) -> f32 {
        10.0
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        1.0
    }

    // the bar behind the tabs is darkened: the active tab, in the fill of its pane, stands out of it
    fn tab_bar_color(&self, visuals: &egui::Visuals) -> egui::Color32 {
        if visuals.dark_mode {
            egui::Color32::from_white_alpha(18) // lighter in the dark theme: black would vanish
        } else {
            egui::Color32::from_black_alpha(70)
        }
    }

    fn tab_bg_color(&self, visuals: &egui::Visuals, _tiles: &Tiles<Pane>, _tile_id: TileId, state: &egui_tiles::TabState) -> egui::Color32 {
        // the sheet of the dock for the active tab, a fainter one for the others, like finder.py
        let sheet = sheet(visuals);
        if state.active {
            sheet
        } else {
            egui::Color32::TRANSPARENT // the darkened bar shows through
        }
    }

    fn tab_outline_stroke(&self, visuals: &egui::Visuals, _tiles: &Tiles<Pane>, _tile_id: TileId, state: &egui_tiles::TabState) -> egui::Stroke {
        let line = visuals.widgets.noninteractive.bg_stroke.color;
        egui::Stroke::new(1.0, if state.active { line } else { line.gamma_multiply(0.5) })
    }

    fn tab_ui(&mut self, tiles: &mut Tiles<Pane>, ui: &mut egui::Ui, id: egui::Id, tile_id: TileId, state: &egui_tiles::TabState) -> egui::Response {
        use egui::{CornerRadius, Sense};
        let text = self.tab_title_for_tile(tiles, tile_id);
        let close = egui::Vec2::splat(self.close_button_outer_size());
        let font_id = egui::TextStyle::Button.resolve(ui.style());
        let galley = text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, font_id);
        let radius = f32::from(ui.visuals().widgets.inactive.corner_radius.nw).max(6.0);
        let x_margin = self.tab_title_spacing(ui.visuals()).max(radius * 0.6);
        let width = galley.size().x + 2.0 * x_margin + if state.closable { 4.0 + close.x } else { 0.0 };
        let (_, outer) = ui.allocate_space(egui::vec2(width + 2.0, ui.available_height()));
        let rect = egui::Rect::from_min_size(outer.min, egui::vec2(width, outer.height()));
        let draggable = self.is_tile_draggable(tiles, tile_id);
        let response = ui.interact(rect, id, if draggable { Sense::click_and_drag() } else { Sense::click() });
        let response = if draggable { response.on_hover_cursor(self.tab_hover_cursor_icon()) } else { response };
        if ui.is_rect_visible(rect) && !state.is_being_dragged {
            let mut fill = self.tab_bg_color(ui.visuals(), tiles, tile_id, state);
            if !state.active && response.hovered() {
                fill = if ui.visuals().dark_mode { egui::Color32::from_white_alpha(14) } else { sheet(ui.visuals()).gamma_multiply(0.4) };
            }
            let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).floor() as u8;
            ui.painter().rect_filled(rect, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, fill);
            let stroke = self.tab_outline_stroke(ui.visuals(), tiles, tile_id, state);
            ui.painter().add(egui::Shape::line(tab_outline(rect.shrink(stroke.width / 2.0), (f32::from(r) - stroke.width / 2.0).max(0.0)), stroke));
            let color = self.tab_text_color(ui.visuals(), tiles, tile_id, state);
            let at = egui::Align2::LEFT_CENTER.align_size_within_rect(galley.size(), rect.shrink(x_margin)).min;
            ui.painter().galley(at, galley, color);
            if state.closable {
                let button = egui::Align2::RIGHT_CENTER.align_size_within_rect(close, rect.shrink(x_margin));
                let click = ui.interact(button, ui.auto_id_with("tab_close_btn"), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Default);
                let visuals = ui.style().interact(&click);
                let cross = button.shrink(self.close_button_inner_margin()).expand(visuals.expansion);
                ui.painter().line_segment([cross.left_top(), cross.right_bottom()], visuals.fg_stroke);
                ui.painter().line_segment([cross.right_top(), cross.left_bottom()], visuals.fg_stroke);
                if (click.clicked() || response.clicked_by(egui::PointerButton::Middle)) && self.on_tab_close(tiles, tile_id) {
                    tiles.remove(tile_id);
                }
            }
        }
        self.on_tab_button(tiles, tile_id, response)
    }

    fn tab_text_color(&self, visuals: &egui::Visuals, _tiles: &Tiles<Pane>, _tile_id: TileId, state: &egui_tiles::TabState) -> egui::Color32 {
        if state.active {
            visuals.text_color()
        } else {
            // the titles of the other tabs are darkened
            visuals.weak_text_color().gamma_multiply(if visuals.dark_mode { 1.0 } else { 0.7 })
        }
    }

    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile: TileId, pane: &mut Pane) -> UiResponse {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match pane {
            Pane::Preview => self.app.preview(ui),
            Pane::Actions => actions_pane(self.app, ui),
            Pane::Appearance => self.app.appearance_menu(ui),
            Pane::Activity => activity_pane(self.app, ui),
            Pane::Tags => crate::tagsui::pane(self.app, ui),
            Pane::Recent => crate::tagsui::recent_pane(self.app, ui),
            Pane::Search => crate::search::pane(self.app, ui),
        });
        UiResponse::None
    }

    fn is_tab_closable(&self, _tiles: &Tiles<Pane>, _tile: TileId) -> bool {
        true
    }
}

pub fn show(app: &mut FinderApp, tree: &mut Tree<Pane>, ui: &mut egui::Ui) {
    if tree.root().is_some() {
        tree.ui(&mut Behavior { app }, ui);
    }
}

/// The menu of the panes: a check box for each, to show it or close it, and the first layout
pub fn menu(tree: &mut Tree<Pane>, ui: &mut egui::Ui) {
    for pane in PANES {
        let found = tree.tiles.find_pane(&pane);
        let mut on = found.is_some();
        if ui.checkbox(&mut on, pane.title()).changed() {
            match found {
                Some(id) => {
                    tree.remove_recursively(id);
                }
                None => add(tree, pane),
            }
        }
    }
    ui.separator();
    if ui.button("Réinitialiser la disposition").clicked() {
        *tree = default_tree();
        ui.close();
    }
}

/// A pane brought back goes in the tabs of the root, or becomes the root
fn add(tree: &mut Tree<Pane>, pane: Pane) {
    let id = tree.tiles.insert_pane(pane);
    match tree.root() {
        Some(root) => {
            if let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(root) {
                tabs.add_child(id);
                tabs.set_active(id);
            } else {
                let new_root = tree.tiles.insert_tab_tile(vec![root, id]);
                tree.root = Some(new_root);
            }
        }
        None => tree.root = Some(tree.tiles.insert_tab_tile(vec![id])),
    }
}

fn actions_pane(app: &mut FinderApp, ui: &mut egui::Ui) {
    let enabled = actions::enabled_flags(app);
    let mut run = None;
    let mut category = "";
    for (index, action) in ACTIONS.iter().enumerate() {
        if action.category != category {
            category = action.category;
            ui.add_space(4.0);
            ui.label(RichText::new(category).strong().small());
        }
        ui.horizontal(|ui| {
            let button = crate::glyphs::action_button(ui, action.icon, action.label).min_size(egui::vec2(ui.available_width() - 90.0, 0.0));
            if ui.add_enabled(enabled[index], button).clicked() {
                run = Some(index);
            }
            if let Some(shortcut) = &action.shortcut {
                ui.weak(ui.ctx().format_shortcut(shortcut));
            }
        });
    }
    if let Some(index) = run {
        let ctx = ui.ctx().clone();
        (ACTIONS[index].run)(app, &ctx);
    }
}

fn activity_pane(app: &mut FinderApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.weak("Dossiers surveillés");
        if ui.small_button("Vider le journal").clicked() {
            app.activity.clear();
        }
    });
    for line in app.activity.iter().rev().take(200) {
        ui.add(egui::Label::new(RichText::new(line).monospace().small()).wrap());
    }
    if app.activity.is_empty() {
        ui.weak("Aucun changement vu pour l'instant");
    }
}
