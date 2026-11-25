//! The dock on the right: panes in tabs that can be closed, moved by their tab and brought back
//! from the + menu. The layout is kept between runs.

use eframe::egui::{self, RichText};
use egui_tiles::{Container, Tile, TileId, Tiles, Tree, UiResponse};
use serde::{Deserialize, Serialize};

use crate::actions::{self, ACTIONS};
use crate::app::FinderApp;
use pyegui_core::dock_draw::{corner_mesh, dedup, feathered_fan, minus, pane_outline, snap, tab_foot, tab_outline, SEAM};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Pane {
    /// a folder (or the file open instead of it), by the number of its tab: with no title when it is alone
    Central(u64),
    Preview,
    Actions,
    Appearance,
    Activity,
    /// the rows of the database, read only
    Sql,
    Tags,
    Recent,
    Search,
    /// the search in the SQLite index (names, PDF criteria)
    Index,
    /// the shell of the terminal and its Python prompt
    Shell,
    Python,
}

pub const PANES: [Pane; 11] = [Pane::Preview, Pane::Actions, Pane::Tags, Pane::Recent, Pane::Search, Pane::Index, Pane::Appearance, Pane::Activity, Pane::Sql, Pane::Shell, Pane::Python];
/// the panes of the first dock at the right of the folder, and those of the terminals below
const SIDE: [Pane; 8] = [Pane::Preview, Pane::Actions, Pane::Tags, Pane::Recent, Pane::Search, Pane::Index, Pane::Appearance, Pane::Activity];
const TERMINALS: [Pane; 2] = [Pane::Shell, Pane::Python];

/// The size of the icon in a tab, and the space after it
const TAB_ICON: f32 = 14.0;
const TAB_ICON_GAP: f32 = 5.0;

impl Pane {
    /// The icon of the tab of a pane
    pub fn icon(self, app: &FinderApp) -> crate::glyphs::Glyph {
        let name = match self {
            Pane::Central(n) => if app.tab_has_file(n) { "document" } else { "folder" },
            Pane::Preview => "eye",
            Pane::Actions => "panes",
            Pane::Appearance => "sun",
            Pane::Activity => "refresh",
            Pane::Sql => "list",
            Pane::Tags => "tag",
            Pane::Recent => "list",
            Pane::Search => "search",
            Pane::Index => "search",
            Pane::Shell | Pane::Python => "terminal",
        };
        crate::glyphs::by_name(name)
    }

    pub fn title(self) -> &'static str {
        match self {
            Pane::Central(_) => "",
            Pane::Preview => "Aperçu",
            Pane::Actions => "Actions",
            Pane::Appearance => "Apparence",
            Pane::Activity => "Activités",
            Pane::Sql => "Base SQL",
            Pane::Shell => "Shell",
            Pane::Python => "Python",
            Pane::Tags => "Étiquettes",
            Pane::Recent => "Récents",
            Pane::Search => "Recherche",
            Pane::Index => "Index",
        }
    }
}

/// The folder with the panes in tabs at its right, and the terminals in tabs below
fn default_tree() -> Tree<Pane> {
    let mut tiles = Tiles::default();
    let central = tiles.insert_pane(Pane::Central(0));
    let side: Vec<TileId> = SIDE.iter().map(|p| tiles.insert_pane(*p)).collect();
    let side = tiles.insert_tab_tile(side);
    let top = tiles.insert_horizontal_tile(vec![central, side]);
    if let Some(Tile::Container(Container::Linear(linear))) = tiles.get_mut(top) {
        linear.shares.set_share(central, 3.0);
        linear.shares.set_share(side, 1.0);
    }
    let terminals: Vec<TileId> = TERMINALS.iter().map(|p| tiles.insert_pane(*p)).collect();
    let terminals = tiles.insert_tab_tile(terminals);
    let root = tiles.insert_vertical_tile(vec![top, terminals]);
    if let Some(Tile::Container(Container::Linear(linear))) = tiles.get_mut(root) {
        linear.shares.set_share(top, 3.0);
        linear.shares.set_share(terminals, 1.0);
    }
    Tree::new(egui::Id::unique("finder-dock"), root, tiles)
}

/// The layout saved, or the default one when there is none or it does not read
pub fn restore(saved: Option<&str>) -> Tree<Pane> {
    let mut tree = saved.and_then(|s| serde_json::from_str::<Tree<Pane>>(s).ok()).filter(|t| t.root().is_some() && t.tiles.find_pane(&Pane::Central(0)).is_some() && (t.tiles.find_pane(&Pane::Shell).is_some() || t.tiles.find_pane(&Pane::Python).is_some())).unwrap_or_else(default_tree);
    // the other tabs of folders are not kept between runs: their state is not
    let others: Vec<TileId> = tree.tiles.iter().filter_map(|(id, t)| matches!(t, Tile::Pane(Pane::Central(n)) if *n != 0).then_some(*id)).collect();
    for id in others {
        tree.remove_recursively(id);
    }
    // a layout saved before the two terminals were panes may miss one of them
    for pane in TERMINALS {
        if tree.tiles.find_pane(&pane).is_none() {
            add(&mut tree, pane);
        }
    }
    tree
}

/// The sheet of the dock: what the panes and their active tab are filled with
fn sheet(visuals: &egui::Visuals) -> egui::Color32 {
    // the active tab and its pane are one piece; the rest (bars, inactive tabs, gaps) is the tint of the bands
    // the glass of the active content
    pyegui_core::style::layer_fill(pyegui_core::style::Layer::Active, visuals)
}



/// The corners of the sheets of the panes
const PANE_RADIUS: f32 = 8.0;
/// Space kept at the left, right and bottom of the sheet of a pane
const PANE_MARGIN: f32 = 3.0;

const TAB_TOP_GAP: f32 = pyegui_core::columns::TAB_TOP_GAP;


struct Behavior<'a> {
    app: &'a mut FinderApp,
    /// containers whose only displayed tab is a folder: no title to show
    hidden_bars: std::collections::HashSet<TileId>,
    /// the folders whose title is shown in a tab bar
    titled: std::collections::HashSet<TileId>,
    /// what is painted with the sheet (active glass) this frame: the glass of the bands is laid under the rest only
    sheets: Vec<egui::Rect>,
    /// the corners round the active tab that the glass of the bands fills (what the curves leave out)
    corners: Vec<egui::Mesh>,
    /// the fill of the active tab and of its feet, painted with the sheets (not clipped by the bar of tabs)
    fills: Vec<egui::Shape>,
    /// the active tab of a pane (its rectangle and its radius), to draw one outline around tab and pane
    active_tabs: std::collections::HashMap<TileId, (egui::Rect, u8)>,
    /// the panes drawn in one sheet (their rectangle): the outline of tab and pane is drawn once the tabs are known
    panes: Vec<(TileId, egui::Rect)>,
    /// the panes under a bar of tabs: their top left corner is square, the active tab stands there
    barred: std::collections::HashSet<TileId>,
}



impl egui_tiles::Behavior<Pane> for Behavior<'_> {
    // a pane alone keeps its tab (to drag it); only a folder alone hides its title, see is_tab_bar_visible
    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions { all_panes_must_have_tabs: true, ..Default::default() }
    }

    // the title of a pane like the title bar of a dock of pyfinder: small, semi bold, muted
    // (the active one in the color of the text), on the color of the panel, a thin line below
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        let title = match pane {
            Pane::Central(n) => self.app.tab_title(*n),
            _ => pane.title().to_string(),
        };
        RichText::new(title).small().strong().into()
    }

    fn is_tab_bar_visible(&self, tile_id: TileId, _tabs: &egui_tiles::Tabs) -> bool {
        !self.hidden_bars.contains(&tile_id)
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        crate::app::TOP_H
    }

    fn tab_title_spacing(&self, _visuals: &egui::Visuals) -> f32 {
        10.0
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        1.0
    }

    // the bar behind the tabs is darkened: the active tab, in the fill of its pane, stands out of it
    // the tint of the bands, so the bars of the docks and the bands are one
    fn tab_bar_color(&self, _visuals: &egui::Visuals) -> egui::Color32 {
        egui::Color32::TRANSPARENT // the tint of the content behind is the one of the bands
    }

    // the tabs start where the sheet of the pane does, not at the edge of its tile
    fn tab_bar_left_ui(&mut self, _tiles: &Tiles<Pane>, ui: &mut egui::Ui, _tile_id: TileId, _tabs: &egui_tiles::Tabs) {
        ui.add_space(PANE_MARGIN);
    }

    // the scroll arrows of a bar of tabs are buttons of the size of the bar: made flat and small here,
    // this ui being the one they are drawn in
    fn top_bar_right_ui(&mut self, _tiles: &Tiles<Pane>, ui: &mut egui::Ui, _tile_id: TileId, _tabs: &egui_tiles::Tabs, _scroll_offset: &mut f32) {
        ui.spacing_mut().icon_width = 12.0;
        let w = &mut ui.visuals_mut().widgets;
        for state in [&mut w.inactive, &mut w.hovered, &mut w.active] {
            state.bg_stroke = egui::Stroke::NONE;
        }
        w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.inactive.bg_fill = egui::Color32::TRANSPARENT;
        w.hovered.weak_bg_fill = egui::Color32::from_white_alpha(20);
        w.active.weak_bg_fill = egui::Color32::from_white_alpha(32);
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
        let (width, color) = self.app.outline(visuals, state.active);
        egui::Stroke::new(width, color)
    }

    fn tab_ui(&mut self, tiles: &mut Tiles<Pane>, ui: &mut egui::Ui, id: egui::Id, tile_id: TileId, state: &egui_tiles::TabState) -> egui::Response {
        use egui::{CornerRadius, Sense};
        let text = self.tab_title_for_tile(tiles, tile_id);
        let close = egui::Vec2::splat(self.close_button_outer_size());
        let font_id = egui::TextStyle::Button.resolve(ui.style());
        let galley = text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, font_id);
        let radius = f32::from(ui.visuals().widgets.inactive.corner_radius.nw).max(6.0);
        let x_margin = self.tab_title_spacing(ui.visuals()).max(radius * 0.6);
        let icon = match tiles.get(tile_id) {
            Some(Tile::Pane(p)) => Some(p.icon(self.app)),
            _ => None,
        };
        let icon_room = if icon.is_some() { TAB_ICON + TAB_ICON_GAP } else { 0.0 };
        let width = galley.size().x + icon_room + 2.0 * x_margin + if state.closable { 4.0 + close.x } else { 0.0 };
        let (_, outer) = ui.allocate_space(egui::vec2(width + 2.0, ui.available_height()));
        // the tab stands on the bottom edge of the bar, with a margin above
        let rect = snap(ui, egui::Rect::from_min_max(egui::pos2(outer.min.x, outer.min.y + TAB_TOP_GAP), egui::pos2(outer.min.x + width, outer.max.y)));
        let draggable = self.is_tile_draggable(tiles, tile_id);
        let response = ui.interact(rect, id, if draggable { Sense::click_and_drag() } else { Sense::click() });
        let response = if draggable { response.on_hover_cursor(self.tab_hover_cursor_icon()) } else { response };
        if ui.is_rect_visible(rect) && !state.is_being_dragged {
            let mut fill = self.tab_bg_color(ui.visuals(), tiles, tile_id, state);
            if !state.active && response.hovered() {
                fill = if ui.visuals().dark_mode { egui::Color32::from_white_alpha(14) } else { sheet(ui.visuals()).gamma_multiply(0.4) };
            }
            let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).floor() as u8;
            if !state.active {
                ui.painter().rect_filled(rect, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, fill);
            }
            // an active tab scrolled out of the bar (partly hidden) is drawn as the part that is seen: a narrower
            // tab, so its feet, corners and outline stay inside the bar and join the pane
            if state.active {
                let rect = rect.intersect(ui.clip_rect());
                let rect = if rect.width() < 2.0 * f32::from(r) { egui::Rect::from_min_size(rect.min, egui::vec2(2.0 * f32::from(r), rect.height())) } else { rect };
                self.sheets.push(rect);
                self.fills.push(egui::Shape::rect_filled(rect, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, fill));
                self.active_tabs.insert(tile_id, (rect, r));
                let flush = rect.min.x - tab_foot(r) < ui.max_rect().left() - 0.01;
                let tint = crate::app::bar_tint(ui.visuals());
                let foot = tab_foot(r);
                let rf = f32::from(r);
                // the squares of the feet and of the round top corners are left to the glass of the bands
                // too: it is laid back where the curves leave them clear
                for (corner, dir) in [(rect.left_bottom(), -1.0f32), (rect.right_bottom(), 1.0f32)] {
                    if dir < 0.0 && flush {
                        continue; // the first tab is flush with the side of the pane: no foot on that side
                    }
                    let center = corner + egui::vec2(dir * foot, -foot);
                    self.sheets.push(egui::Rect::from_two_pos(corner, center));
                    // the quarter of disc on the far side of the curve
                    self.corners.push(feathered_fan(center, foot, false, SEAM, tint, |rad, i| {
                        let a = std::f32::consts::FRAC_PI_2 * i as f32 / 8.0;
                        egui::pos2(center.x - dir * rad * a.cos(), center.y + rad * a.sin())
                    }));
                }
                for (outer, dir) in [(rect.left_top(), 1.0f32), (rect.right_top(), -1.0f32)] {
                    self.corners.push(corner_mesh(outer, dir, 1.0, rf, tint));
                }
                // concave feet at both bottom corners join the tab to its pane
                for (corner, dir) in [(rect.left_bottom(), -1.0f32), (rect.right_bottom(), 1.0f32)] {
                    if dir < 0.0 && flush {
                        continue; // the first tab is flush with the side of the pane: no foot on that side
                    }
                    let center = corner + egui::vec2(dir * foot, -foot);
                    self.fills.push(egui::Shape::mesh(feathered_fan(corner, foot, true, 0.0, fill, |rad, i| {
                        let a = std::f32::consts::FRAC_PI_2 * i as f32 / 8.0;
                        egui::pos2(center.x - dir * rad * a.cos(), center.y + rad * a.sin())
                    })));
                }
            }
            let stroke = self.tab_outline_stroke(ui.visuals(), tiles, tile_id, state);
            let foot = if state.active { tab_foot(r) } else { 0.0 };
            if !state.active {
                ui.painter().add(egui::Shape::line(dedup(tab_outline(rect, f32::from(r), foot), false), stroke));
            }
            let color = self.tab_text_color(ui.visuals(), tiles, tile_id, state);
            let inner = rect.shrink(x_margin);
            if let Some(icon) = icon {
                let spot = egui::Rect::from_center_size(egui::pos2(inner.min.x + TAB_ICON / 2.0, inner.center().y), egui::Vec2::splat(TAB_ICON));
                crate::glyphs::image(icon, TAB_ICON, color).paint_at(ui, spot);
            }
            let text_area = egui::Rect::from_min_max(inner.min + egui::vec2(icon_room, 0.0), inner.max);
            let at = egui::Align2::LEFT_CENTER.align_size_within_rect(galley.size(), text_area).min;
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

    fn pane_ui(&mut self, ui: &mut egui::Ui, tile: TileId, pane: &mut Pane) -> UiResponse {
        let fill = sheet(ui.visuals());
        // the sheet and its outline keep a few pixels from the sides of the pane: two neighbours do not touch
        let full_area = snap(ui, ui.max_rect());
        let area = egui::Rect::from_min_max(
            egui::pos2(full_area.left() + PANE_MARGIN, full_area.top()),
            egui::pos2(full_area.right() - PANE_MARGIN, full_area.bottom() - PANE_MARGIN),
        );
        // the sheet is laid down under what is drawn, once the header of the columns (if any) is known.
        // A folder with its title shown: the active tab, the header and the view are one sheet. Without
        // a title the header is transparent (the tint of the bands) until the rows scroll under it: then
        // it takes the sheet, which hides them
        let slot = ui.painter().add(egui::Shape::Noop);
        let band_key = egui::Id::unique(pyegui_core::columns::HEADER_BAND);
        let scrolled_key = egui::Id::unique(pyegui_core::columns::HEADER_SCROLLED);
        ui.ctx().data_mut(|d| {
            d.remove::<egui::Rect>(band_key);
            d.remove::<egui::Rect>(egui::Id::unique(crate::app::TAG_BAND));
            d.remove::<bool>(scrolled_key);
        });
        let result = self.pane_content(ui, pane);
        let band = ui.ctx().data(|d| d.get_temp::<egui::Rect>(band_key)).filter(|_| matches!(*pane, Pane::Central(_)));
        let scrolled = ui.ctx().data(|d| d.get_temp::<bool>(scrolled_key)).unwrap_or(false);
        let titled = self.titled.contains(&tile);
        // the header of a tag, above the header of the columns, is left clear with it
        let tag_band = ui.ctx().data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(crate::app::TAG_BAND))).filter(|_| matches!(*pane, Pane::Central(_)) && !titled);
        let cleared = band.filter(|_| !titled && !scrolled);
        let cleared = match (cleared, tag_band) {
            (Some(b), Some(t)) => Some(b.union(t)),
            (b, t) => b.or(t),
        };
        let rects: Vec<egui::Rect> = match cleared {
            Some(b) => {
                let k = ui.ctx().pixels_per_point();
                let (mut top, bottom) = (((b.top().clamp(area.top(), area.bottom())) * k).round() / k, ((b.bottom().clamp(area.top(), area.bottom())) * k).round() / k);
                if top - area.top() < 16.0 {
                    top = area.top(); // the margin above the header is left clear too
                }
                vec![
                    egui::Rect::from_min_max(area.min, egui::pos2(area.max.x, top)),
                    egui::Rect::from_min_max(egui::pos2(area.min.x, bottom), area.max),
                ]
            }
            None => vec![area],
        };
        self.sheets.extend(rects.iter().copied());
        let tint = crate::app::bar_tint(ui.visuals());
        let full = rects.len() == 1;
        let rr = PANE_RADIUS;
        let mut shapes: Vec<egui::Shape> = Vec::new();
        for (i, r) in rects.iter().enumerate() {
            // a sheet in one piece has its four corners round, one with the header left clear only the bottom ones
            let (top, bottom) = (full, i == rects.len() - 1);
            let rad = |on: bool| if on { rr as u8 } else { 0 };
            let square_tl = self.barred.contains(&tile);
            shapes.push(egui::Shape::rect_filled(*r, egui::CornerRadius { nw: if square_tl { 0 } else { rad(top) }, ne: rad(top), sw: rad(bottom), se: rad(bottom) }, fill));
            if top {
                if !square_tl {
                    self.corners.push(corner_mesh(r.left_top(), 1.0, 1.0, rr, tint));
                }
                self.corners.push(corner_mesh(r.right_top(), -1.0, 1.0, rr, tint));
            }
            if bottom {
                self.corners.push(corner_mesh(r.left_bottom(), 1.0, -1.0, rr, tint));
                self.corners.push(corner_mesh(r.right_bottom(), -1.0, -1.0, rr, tint));
            }
        }
        if full {
            self.panes.push((tile, area));
        }
        ui.painter().set(slot, egui::Shape::Vec(shapes));
        result
    }

    // a tab of folder can be closed when it is not the last one
    fn is_tab_closable(&self, tiles: &Tiles<Pane>, tile: TileId) -> bool {
        match tiles.get(tile) {
            Some(Tile::Pane(Pane::Central(_))) => tiles.iter().filter(|(_, t)| matches!(t, Tile::Pane(Pane::Central(_)))).count() > 1,
            _ => true,
        }
    }

    fn on_tab_close(&mut self, tiles: &mut Tiles<Pane>, tile: TileId) -> bool {
        if let Some(Tile::Pane(Pane::Central(n))) = tiles.get(tile) {
            self.app.close_tab(*n);
        }
        true
    }
}

impl Behavior<'_> {
    fn pane_content(&mut self, ui: &mut egui::Ui, pane: &mut Pane) -> UiResponse {
        if let Pane::Central(n) = *pane {
            // the state of this tab is the one of the app while it is drawn
            self.app.swap_tab(n);
            // the header of the columns is the first thing in the pane, at the top, like the tab bars
            let margin = egui::Margin { left: 8, right: 8, top: 0, bottom: 8 };
            egui::Frame::NONE.inner_margin(margin).show(ui, |ui| self.app.central_pane(ui));
            return UiResponse::None;
        }
        if matches!(pane, Pane::Shell | Pane::Python) {
            // a terminal fills its pane
            self.app.terminal_pane(ui, *pane == Pane::Shell);
            return UiResponse::None;
        }
        egui::Frame::NONE.inner_margin(egui::Margin::same(8)).show(ui, |ui| egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match pane {
            Pane::Preview => self.app.preview(ui),
            Pane::Actions => actions_pane(self.app, ui),
            Pane::Appearance => self.app.appearance_menu(ui),
            Pane::Activity => activity_pane(self.app, ui),
            Pane::Sql => crate::sqlpane::pane(&mut self.app.sql, ui),
            Pane::Tags => crate::tagsui::pane(self.app, ui),
            Pane::Recent => crate::tagsui::recent_pane(self.app, ui),
            Pane::Search => crate::search::pane(self.app, ui),
            Pane::Index => crate::indexsearch::pane(self.app, ui),
            Pane::Central(_) | Pane::Shell | Pane::Python => {}
        }));
        UiResponse::None
    }

}

pub fn show(app: &mut FinderApp, tree: &mut Tree<Pane>, ui: &mut egui::Ui) {
    // the glass of the bands is laid down once the sheets are known: it is not under them, so the
    // active glass does not depend on the inactive one
    let dock_rect = snap(ui, ui.max_rect());
    let slot = ui.painter().add(egui::Shape::Noop);
    if tree.root().is_none() {
        ui.painter().set(slot, egui::Shape::rect_filled(dock_rect, 0.0, crate::app::bar_tint(ui.visuals())));
        return;
    }
    // the dock toggle hides the panes (not the folder), the terminal toggle the two terminals
    let (shown, terminals) = (app.dock_shown(), app.terminals_shown());
    let want: Vec<(TileId, bool)> = tree.tiles.iter().filter_map(|(id, t)| match t {
        Tile::Pane(Pane::Central(_)) => None,
        Tile::Pane(Pane::Shell | Pane::Python) => Some((*id, terminals)),
        Tile::Pane(_) => Some((*id, shown)),
        _ => None,
    }).collect();
    for (id, visible) in want {
        if tree.tiles.is_visible(id) != visible {
            tree.tiles.set_visible(id, visible);
        }
    }
    // a folder alone in its tabs has no title; the other panes alone keep theirs
    let hidden_bars: std::collections::HashSet<TileId> = tree.tiles.iter().filter_map(|(id, t)| match t {
        Tile::Container(Container::Tabs(tabs)) => {
            let shown: Vec<&TileId> = tabs.children.iter().filter(|c| tree.tiles.is_visible(**c)).collect();
            (shown.len() == 1 && matches!(tree.tiles.get(*shown[0]), Some(Tile::Pane(Pane::Central(_))))).then_some(*id)
        }
        _ => None,
    }).collect();
    let titled = tree.tiles.iter().filter_map(|(id, t)| match t {
        Tile::Container(Container::Tabs(tabs)) if !hidden_bars.contains(id) => Some(tabs.children.clone()),
        _ => None,
    }).flatten().filter(|c| matches!(tree.tiles.get(*c), Some(Tile::Pane(Pane::Central(_))))).collect();
    let barred: std::collections::HashSet<TileId> = tree.tiles.iter().filter_map(|(id, t)| match t {
        Tile::Container(Container::Tabs(tabs)) if !hidden_bars.contains(id) => Some(tabs.children.clone()),
        _ => None,
    }).flatten().collect();
    let mut behavior = Behavior { app, hidden_bars, titled, sheets: Vec::new(), corners: Vec::new(), fills: Vec::new(), active_tabs: Default::default(), panes: Vec::new(), barred };
    tree.ui(&mut behavior, ui);
    let tint = crate::app::bar_tint(ui.visuals());
    let mut rest = vec![dock_rect];
    for hole in &behavior.sheets {
        rest = rest.into_iter().flat_map(|r| minus(r, *hole)).collect();
    }
    let mut shapes: Vec<egui::Shape> = rest.into_iter().map(|r| egui::Shape::rect_filled(r, 0.0, tint)).collect();
    shapes.extend(behavior.corners.drain(..).map(egui::Shape::mesh));
    shapes.extend(behavior.fills.drain(..));
    {
        let (width, color) = behavior.app.outline(ui.visuals(), true);
        for (tile, area) in &behavior.panes {
            let pts = pane_outline(*area, behavior.active_tabs.get(tile).copied(), behavior.barred.contains(tile), PANE_RADIUS);
            ui.painter().add(egui::Shape::closed_line(dedup(pts, true), egui::Stroke::new(width, color)));
        }
    }
    ui.painter().set(slot, egui::Shape::Vec(shapes));
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

/// A pane brought back goes in the tabs of the panes, or in new tabs at the right of the root
fn add(tree: &mut Tree<Pane>, pane: Pane) {
    let id = tree.tiles.insert_pane(pane);
    // next to the panes of its kind: a terminal with the terminals, any other with the panes
    let kind = |p: &Pane| TERMINALS.contains(p);
    let tabs = tree.tiles.iter().find_map(|(tid, t)| match t {
        Tile::Container(Container::Tabs(tabs)) if tabs.children.iter().any(|c| matches!(tree.tiles.get(*c), Some(Tile::Pane(p)) if !matches!(p, Pane::Central(_)) && kind(p) == kind(&pane))) => Some(*tid),
        _ => None,
    });
    if let Some(Tile::Container(Container::Tabs(tabs))) = tabs.and_then(|t| tree.tiles.get_mut(t)) {
        tabs.add_child(id);
        tabs.set_active(id);
        return;
    }
    let new_tabs = tree.tiles.insert_tab_tile(vec![id]);
    match tree.root() {
        Some(root) => {
            if let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(root) {
                linear.add_child(new_tabs);
                linear.shares.set_share(new_tabs, 1.0);
            } else {
                let new_root = tree.tiles.insert_horizontal_tile(vec![root, new_tabs]);
                tree.root = Some(new_root);
            }
        }
        None => tree.root = Some(new_tabs),
    }
}

/// The widest a button of the Actions pane gets, so that the buttons of a wide dock keep the same width
const ACTION_BUTTON_MAX_WIDTH: f32 = 230.0;

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
            let button = crate::glyphs::action_button(ui, action.icon, action.label).min_size(egui::vec2((ui.available_width() - 90.0).min(ACTION_BUTTON_MAX_WIDTH), 0.0));
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
    // an indexation in progress: the share of the folders read (an estimate: the walk learns of its
    // sub-folders as it goes), the entries recorded and the folder being read
    let mut cancel = false;
    if let Some((folder, progress)) = app.index_progress() {
        use std::sync::atomic::Ordering::Relaxed;
        let (done, known) = (progress.folders_done.load(Relaxed), progress.folders_known.load(Relaxed));
        let fraction = progress.fraction();
        ui.strong(format!("Indexation de {}", folder.display()));
        ui.add(egui::ProgressBar::new(fraction).text(format!("{:.0} %", fraction * 100.0)).animate(true));
        ui.horizontal(|ui| {
            ui.weak(format!("{done} / {known} dossiers · {} éléments", progress.entries.load(Relaxed)));
            if ui.small_button("Annuler").clicked() {
                cancel = true;
            }
        });
        let current = progress.current.lock().unwrap_or_else(|e| e.into_inner()).clone();
        ui.add(egui::Label::new(RichText::new(current).monospace().small().weak()).truncate());
        ui.separator();
    }
    if cancel {
        app.cancel_indexing();
    }
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

/// The tab of a new folder: in the tabs of the tab `after`, or in new tabs that replace it
pub fn add_central(tree: &mut Tree<Pane>, new: u64, after: u64) {
    let id = tree.tiles.insert_pane(Pane::Central(new));
    let Some(current) = tree.tiles.find_pane(&Pane::Central(after)) else {
        tree.tiles.remove(id);
        return;
    };
    match tree.tiles.parent_of(current) {
        Some(parent) if matches!(tree.tiles.get(parent), Some(Tile::Container(Container::Tabs(_)))) => {
            if let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(parent) {
                tabs.add_child(id);
                tabs.set_active(id);
            }
        }
        parent => {
            let tabs = tree.tiles.insert_tab_tile(vec![current, id]);
            if let Some(Tile::Container(Container::Tabs(t))) = tree.tiles.get_mut(tabs) {
                t.set_active(id);
            }
            match parent {
                Some(parent) => {
                    if let Some(Tile::Container(container)) = tree.tiles.get_mut(parent) {
                        match container {
                            Container::Linear(l) => {
                                for child in l.children.iter_mut() {
                                    if *child == current {
                                        *child = tabs;
                                    }
                                }
                                let share = l.shares[current];
                                l.shares.set_share(tabs, share);
                            }
                            other => {
                                other.remove_child(current);
                                other.add_child(tabs);
                            }
                        }
                    }
                }
                None => tree.root = Some(tabs),
            }
        }
    }
}
