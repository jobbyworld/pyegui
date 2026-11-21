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
}

pub const PANES: [Pane; 4] = [Pane::Preview, Pane::Actions, Pane::Appearance, Pane::Activity];

impl Pane {
    pub fn title(self) -> &'static str {
        match self {
            Pane::Preview => "Aperçu",
            Pane::Actions => "Actions",
            Pane::Appearance => "Apparence",
            Pane::Activity => "Activités",
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

struct Behavior<'a> {
    app: &'a mut FinderApp,
}

impl egui_tiles::Behavior<Pane> for Behavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        pane.title().into()
    }

    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile: TileId, pane: &mut Pane) -> UiResponse {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match pane {
            Pane::Preview => self.app.preview(ui),
            Pane::Actions => actions_pane(self.app, ui),
            Pane::Appearance => self.app.appearance_menu(ui),
            Pane::Activity => activity_pane(self.app, ui),
        });
        UiResponse::None
    }

    fn is_tab_closable(&self, _tiles: &Tiles<Pane>, _tile: TileId) -> bool {
        true
    }
}

pub fn show(app: &mut FinderApp, tree: &mut Tree<Pane>, ui: &mut egui::Ui) {
    let missing: Vec<Pane> = PANES.into_iter().filter(|p| !present(&tree.tiles).contains(p)).collect();
    if !missing.is_empty() || tree.root().is_none() {
        ui.horizontal(|ui| {
            ui.menu_button("＋ Volets", |ui| {
                for pane in &missing {
                    if ui.button(pane.title()).clicked() {
                        add(tree, *pane);
                        ui.close();
                    }
                }
            });
        });
    }
    if tree.root().is_some() {
        tree.ui(&mut Behavior { app }, ui);
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
            let button = egui::Button::new(format!("{}  {}", action.icon, action.label)).min_size(egui::vec2(ui.available_width() - 90.0, 0.0));
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
