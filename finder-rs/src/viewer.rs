//! The document viewer that takes the place of the folder: markdown (rendered, or its source), source
//! code, images and PDF files (page, zoom, search, text selection). It reads the file again when it
//! changes on the disk, keeping its place.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText};
use pyegui_core::markdown::{self, MarkdownOptions, TableFit};
use pyegui_core::pdf_nav::{Bookmark, Target};
use pyegui_core::pdf_view::{self, PdfDocument, PdfView, PdfViewOptions, ThumbnailOptions};
use pyegui_core::watcher::{lock, Core};

use crate::model;

pub enum Body {
    Markdown { text: String, source: bool },
    Code { text: String, lang: String },
    Image,
    Pdf(Box<PdfBody>),
}

/// A PDF: the document, its view and what the tools hold (thumbnails, bookmarks, search)
pub struct PdfBody {
    doc: Arc<Mutex<PdfDocument>>,
    view: PdfView,
    query: String,
    password: String,
    /// the thumbnails of all the pages in place of the pages
    grid: bool,
    thumb_size: f32,
    /// the bookmarks in a column at the left (None until read from the document)
    outline_open: bool,
    outline: Option<Vec<Bookmark>>,
}

pub struct Viewer {
    pub path: PathBuf,
    pub body: Body,
}

fn open_pdf(path: &Path) -> Arc<Mutex<PdfDocument>> {
    Arc::new(Mutex::new(PdfDocument::open(path.to_string_lossy().into_owned(), None, false)))
}

impl Viewer {
    /// A viewer for a file, None when the file has no viewer here (the system opens it)
    pub fn open(path: &Path, core: &Arc<Mutex<Core>>) -> Option<Viewer> {
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let body = if ext == "pdf" {
            let doc = open_pdf(path);
            let view = PdfView::new(Some(doc.clone()), "width", 1.0, true, 1).ok()?;
            Body::Pdf(Box::new(PdfBody {
                doc,
                view,
                query: String::new(),
                password: String::new(),
                grid: false,
                thumb_size: 140.0,
                outline_open: false,
                outline: None,
            }))
        } else if model::is_image(path) {
            Body::Image
        } else if matches!(ext.as_str(), "md" | "markdown") {
            Body::Markdown { text: std::fs::read_to_string(path).ok()?, source: false }
        } else if model::is_text(path) {
            Body::Code { text: std::fs::read_to_string(path).ok()?, lang: ext }
        } else {
            return None;
        };
        lock(core).watch_path(path);
        Some(Viewer { path: path.to_path_buf(), body })
    }

    /// Reads the file again when it changed on the disk
    fn reload(&mut self) {
        match &mut self.body {
            Body::Markdown { text, .. } | Body::Code { text, .. } => {
                if let Ok(new) = std::fs::read_to_string(&self.path) {
                    *text = new;
                }
            }
            Body::Pdf(p) => {
                // same page, zoom, rotation and scroll: the view keeps its place
                let new = open_pdf(&self.path);
                p.view.replace_doc(Some(new.clone()));
                p.doc = new;
                p.outline = None;
            }
            Body::Image => {}
        }
    }
}

/// Draws the viewer; returns true when the user closes it
pub fn show(viewer: &mut Viewer, ui: &mut egui::Ui, core: &Arc<Mutex<Core>>) -> bool {
    if lock(core).has_changed(&viewer.path) {
        viewer.reload();
        ui.ctx().forget_image(&format!("file://{}", viewer.path.to_string_lossy()));
    }
    let mut close = false;
    ui.horizontal_wrapped(|ui| {
        if ui.button("← Dossier").on_hover_text("Fermer (Échap)").clicked() {
            close = true;
        }
        ui.label(RichText::new(viewer.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).strong());
        match &mut viewer.body {
            Body::Markdown { source, .. } => {
                ui.separator();
                ui.selectable_value(source, false, "Rendu");
                ui.selectable_value(source, true, "Source");
            }
            Body::Pdf(p) => pdf_bar(ui, p),
            _ => {}
        }
    });
    ui.separator();
    let path = viewer.path.clone();
    match &mut viewer.body {
        Body::Markdown { text, source } => {
            if *source {
                markdown::code_view(ui, text, "md", true, "viewer-source");
            } else {
                let options = MarkdownOptions {
                    max_width: Some(820.0),
                    align: 0.5,
                    policies: markdown::Policies { table: TableFit::Reformat, code_wide: false, image_wide: false, mermaid_wide: false },
                    ..Default::default()
                };
                // the scroll offset belongs to the file: it is kept when the file is read again
                egui::ScrollArea::vertical().id_salt(("viewer-md", &path)).auto_shrink([false, false]).show(ui, |ui| {
                    if let Some(url) = markdown::show(ui, text, &options) {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                });
            }
        }
        Body::Code { text, lang } => markdown::code_view(ui, text, lang, true, "viewer-code"),
        Body::Image => {
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                ui.add(egui::Image::new(format!("file://{}", path.to_string_lossy())).max_width(ui.available_width()));
            });
        }
        Body::Pdf(p) => pdf_body(ui, p),
    }
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) && !ui.ctx().egui_wants_keyboard_input() {
        close = true;
    }
    close
}

fn pdf_body(ui: &mut egui::Ui, p: &mut PdfBody) {
    let (loading, needs_password, error, pages) = {
        let mut d = p.doc.lock().unwrap_or_else(|e| e.into_inner());
        (d.loading(), d.needs_password(), d.error(), d.pages())
    };
    if loading {
        ui.label(RichText::new("Ouverture…").weak());
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        return;
    }
    if needs_password {
        ui.label("🔒 Ce PDF est protégé par un mot de passe.");
        return;
    }
    if let Some(error) = error {
        ui.colored_label(ui.visuals().error_fg_color, format!("⚠ {error}"));
        return;
    }
    if p.outline_open {
        if p.outline.is_none() {
            p.outline = Some(p.doc.lock().unwrap_or_else(|e| e.into_inner()).outline());
        }
        egui::Panel::left("viewer-pdf-outline").resizable(true).default_size(220.0).show_inside(ui, |ui| {
            let marks = p.outline.as_deref().unwrap_or(&[]);
            if marks.is_empty() {
                ui.label(RichText::new("Pas de signets").weak());
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for mark in marks {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0 * mark.level as f32);
                        let go = ui.add(egui::Button::new(&mark.title).frame(false).wrap_mode(egui::TextWrapMode::Truncate));
                        if go.clicked() {
                            if let Some(Target::Page { page, y }) = &mark.target {
                                p.view.go_to(page + 1, y.unwrap_or(0.0));
                                p.grid = false;
                            }
                        }
                    });
                }
            });
        });
    }
    if p.grid && pages > 0 {
        // a click on a thumbnail goes back to the pages, at that page, one at a time
        let mut chosen = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let mut d = p.doc.lock().unwrap_or_else(|e| e.into_inner());
                for n in 1..=pages {
                    let options = ThumbnailOptions { size: p.thumb_size, selected: n == p.view.page(), label: true, dark: p.view.dark() };
                    if pdf_view::pdf_thumbnail(ui, &mut d, n, &options).unwrap_or(false) {
                        chosen = Some(n);
                    }
                }
            });
        });
        if let Some(n) = chosen {
            p.view.set_page(n);
            p.view.set_continuous(false);
            p.grid = false;
        }
        return;
    }
    let background = if ui.visuals().dark_mode { egui::Color32::from_rgb(0x4a, 0x4b, 0x50) } else { egui::Color32::from_rgb(0xd4, 0xd5, 0xd9) };
    pdf_view::pdf_view(ui, &mut p.view, &PdfViewOptions { background: Some(background), ..Default::default() });
}

fn pdf_bar(ui: &mut egui::Ui, p: &mut PdfBody) {
    let PdfBody { doc, view, query, password, grid, thumb_size, outline_open, .. } = p;
    let (loading, needs_password, error) = {
        let mut d = doc.lock().unwrap_or_else(|e| e.into_inner());
        (d.loading(), d.needs_password(), d.error())
    };
    if loading || error.is_some() {
        return;
    }
    if needs_password {
        ui.separator();
        let edit = ui.add(egui::TextEdit::singleline(password).password(true).hint_text("Mot de passe").desired_width(140.0));
        if (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button("Ouvrir").clicked() {
            doc.lock().unwrap_or_else(|e| e.into_inner()).unlock(password);
            password.clear();
            view.set_doc(Some(doc.clone()));
        }
        return;
    }
    ui.separator();
    ui.toggle_value(grid, "▦").on_hover_text(if *grid { "Pages" } else { "Miniatures" });
    ui.toggle_value(outline_open, "☰").on_hover_text("Signets");
    if *grid {
        let pages = doc.lock().unwrap_or_else(|e| e.into_inner()).pages();
        ui.label(RichText::new(format!("{pages} pages")).weak());
        ui.add(egui::Slider::new(thumb_size, 80.0..=320.0).text("Taille"));
        return;
    }
    ui.separator();
    if ui.button("◀").on_hover_text("Page précédente").clicked() {
        view.previous_page();
    }
    ui.label(format!("{} / {}", view.page(), view.pages()));
    if ui.button("▶").on_hover_text("Page suivante").clicked() {
        view.next_page();
    }
    ui.separator();
    if ui.button("−").on_hover_text("Réduire").clicked() {
        view.set_zoom(view.zoom() / 1.25);
    }
    ui.label(format!("{:.0} %", view.zoom() * 100.0));
    if ui.button("+").on_hover_text("Agrandir").clicked() {
        view.set_zoom(view.zoom() * 1.25);
    }
    if ui.selectable_label(view.fit() == "width", "Largeur").on_hover_text("Ajuster à la largeur").clicked() {
        let _ = view.set_fit("width");
    }
    if ui.selectable_label(view.fit() == "page", "Page").on_hover_text("Ajuster à la page").clicked() {
        let _ = view.set_fit("page");
    }
    if ui.button("⟳").on_hover_text("Pivoter de 90°").clicked() {
        view.rotate(90);
    }
    let hand = view.tool() == "hand";
    if ui.selectable_label(!hand, "Ⓘ").on_hover_text("Sélectionner du texte").clicked() {
        let _ = view.set_tool("select");
    }
    if ui.selectable_label(hand, "✋").on_hover_text("Déplacer la page").clicked() {
        let _ = view.set_tool("hand");
    }
    let mut continuous = view.continuous();
    if ui.toggle_value(&mut continuous, "↕").on_hover_text("Défilement continu").changed() {
        view.set_continuous(continuous);
    }
    let mut dark = view.dark();
    if ui.toggle_value(&mut dark, "☾").on_hover_text("Pages sombres").changed() {
        view.set_dark(dark);
    }
    if view.busy() {
        ui.label(RichText::new("…").weak());
    }
    ui.separator();
    let edit = ui.add(egui::TextEdit::singleline(query).hint_text("Rechercher dans le PDF").desired_width(160.0));
    if edit.changed() {
        view.search(query, false);
    }
    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        if ui.input(|i| i.modifiers.shift) { view.previous_match() } else { view.next_match() }
        edit.request_focus();
    }
    if !query.is_empty() {
        if view.matches() > 0 {
            ui.label(format!("{} / {}{}", view.match_index(), view.matches(), if view.searching() { "+" } else { "" }));
        } else {
            ui.label(RichText::new(if view.searching() { "recherche…" } else { "aucun résultat" }).weak());
        }
        if ui.small_button("▲").on_hover_text("Résultat précédent").clicked() {
            view.previous_match();
        }
        if ui.small_button("▼").on_hover_text("Résultat suivant").clicked() {
            view.next_match();
        }
    }
    if !view.selected_text().is_empty() && ui.button("Copier").on_hover_text("Copier la sélection").clicked() {
        view.copy_selection();
    }
}
