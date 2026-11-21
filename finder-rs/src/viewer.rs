//! The document viewer that takes the place of the folder: markdown (rendered, or its source), source
//! code, images and PDF files (page, zoom, search, text selection). It reads the file again when it
//! changes on the disk, keeping its place.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText};
use pyegui_core::markdown::{self, MarkdownOptions, TableFit};
use pyegui_core::pdf_view::{self, PdfDocument, PdfView, PdfViewOptions};
use pyegui_core::watcher::{lock, Core};

use crate::model;

pub enum Body {
    Markdown { text: String, source: bool },
    Code { text: String, lang: String },
    Image,
    Pdf { doc: Arc<Mutex<PdfDocument>>, view: Box<PdfView>, query: String, password: String },
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
            Body::Pdf { doc, view: Box::new(view), query: String::new(), password: String::new() }
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
            Body::Pdf { doc, view, .. } => {
                let new = open_pdf(&self.path);
                view.replace_doc(Some(new.clone()));
                *doc = new;
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
    ui.horizontal(|ui| {
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
            Body::Pdf { doc, view, query, password } => pdf_bar(ui, doc, view, query, password),
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
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
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
        Body::Pdf { doc, view, .. } => {
            let (needs_password, error) = {
                let mut d = doc.lock().unwrap_or_else(|e| e.into_inner());
                (d.needs_password(), d.error())
            };
            if needs_password {
                ui.label("Ce PDF est protégé par un mot de passe.");
            } else if let Some(error) = error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            } else {
                pdf_view::pdf_view(ui, view, &PdfViewOptions::default());
            }
        }
    }
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) && !ui.ctx().egui_wants_keyboard_input() {
        close = true;
    }
    close
}

fn pdf_bar(ui: &mut egui::Ui, doc: &Arc<Mutex<PdfDocument>>, view: &mut PdfView, query: &mut String, password: &mut String) {
    let needs_password = doc.lock().unwrap_or_else(|e| e.into_inner()).needs_password();
    if needs_password {
        ui.separator();
        let edit = ui.add(egui::TextEdit::singleline(password).password(true).hint_text("Mot de passe").desired_width(140.0));
        if (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button("Ouvrir").clicked() {
            let ok = doc.lock().unwrap_or_else(|e| e.into_inner()).unlock(password);
            if ok {
                password.clear();
            }
        }
        return;
    }
    ui.separator();
    if ui.button("◀").clicked() {
        view.previous_page();
    }
    ui.label(format!("{} / {}", view.page(), view.pages()));
    if ui.button("▶").clicked() {
        view.next_page();
    }
    ui.separator();
    if ui.button("−").clicked() {
        view.set_zoom(view.zoom() / 1.25);
    }
    ui.label(format!("{:.0} %", view.zoom() * 100.0));
    if ui.button("+").clicked() {
        view.set_zoom(view.zoom() * 1.25);
    }
    if ui.selectable_label(view.fit() == "width", "Largeur").clicked() {
        let _ = view.set_fit("width");
    }
    if ui.selectable_label(view.fit() == "page", "Page").clicked() {
        let _ = view.set_fit("page");
    }
    if ui.button("⟳").on_hover_text("Pivoter").clicked() {
        view.rotate(90);
    }
    let mut dark = view.dark();
    if ui.toggle_value(&mut dark, "☾").on_hover_text("Pages sombres").changed() {
        view.set_dark(dark);
    }
    let hand = view.tool() == "hand";
    if ui.selectable_label(hand, "✋").on_hover_text("Main").clicked() {
        let _ = view.set_tool(if hand { "select" } else { "hand" });
    }
    ui.separator();
    let edit = ui.add(egui::TextEdit::singleline(query).hint_text("Rechercher").desired_width(140.0));
    if edit.changed() {
        view.search(query, false);
    }
    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        if ui.input(|i| i.modifiers.shift) { view.previous_match() } else { view.next_match() }
        edit.request_focus();
    }
    if !query.is_empty() {
        ui.label(if view.searching() { "…".to_owned() } else { format!("{}/{}", view.match_index(), view.matches()) });
        if ui.small_button("▲").clicked() {
            view.previous_match();
        }
        if ui.small_button("▼").clicked() {
            view.next_match();
        }
    }
}
