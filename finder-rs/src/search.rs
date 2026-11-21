//! The advanced search pane: names (case and accents ignored), kind, size, date and, for the PDF
//! files, the number of pages. The folders are walked by a thread, the results appear as they are found.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use eframe::egui::{self, RichText};
use pyegui_core::watcher::Entry;

use crate::app::FinderApp;
use crate::model;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    All,
    Files,
    Folders,
    Pdf,
    Image,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::All => "Tout",
            Kind::Files => "Fichiers",
            Kind::Folders => "Dossiers",
            Kind::Pdf => "PDF",
            Kind::Image => "Images",
        }
    }
}

#[derive(Clone)]
pub struct Filters {
    pub query: String,
    pub kind: Kind,
    pub min_mb: f32,
    pub max_mb: f32,
    /// modified in the last N days, 0 for any date
    pub days: u32,
    /// PDF only: at least this number of pages, 0 for any
    pub min_pages: u32,
    pub hidden: bool,
}

impl Default for Filters {
    fn default() -> Self {
        Filters { query: String::new(), kind: Kind::All, min_mb: 0.0, max_mb: 0.0, days: 0, min_pages: 0, hidden: false }
    }
}

/// Lower case without the accents of the Latin letters, to compare names
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            other => other,
        })
        .collect()
}

fn matches(entry: &Entry, filters: &Filters, needle: &str, now: SystemTime) -> bool {
    if !filters.hidden && entry.hidden {
        return false;
    }
    let is_pdf = entry.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    let kind_ok = match filters.kind {
        Kind::All => true,
        Kind::Files => !entry.is_dir,
        Kind::Folders => entry.is_dir,
        Kind::Pdf => is_pdf,
        Kind::Image => model::is_image(&entry.path),
    };
    if !kind_ok || !(needle.is_empty() || fold(&entry.name).contains(needle)) {
        return false;
    }
    if !entry.is_dir {
        let mb = entry.size as f32 / 1_000_000.0;
        if mb < filters.min_mb || (filters.max_mb > 0.0 && mb > filters.max_mb) {
            return false;
        }
    }
    if filters.days > 0 {
        let limit = now - Duration::from_secs(u64::from(filters.days) * 86_400);
        if entry.modified.is_none_or(|m| m < limit) {
            return false;
        }
    }
    if filters.min_pages > 0 {
        if !is_pdf {
            return false;
        }
        let info = pyegui_core::pdf::info_of_path(&entry.path.to_string_lossy());
        if info.pages.unwrap_or(0) < filters.min_pages as usize {
            return false;
        }
    }
    true
}

#[derive(Default)]
pub struct Search {
    pub filters: Filters,
    cancel: Option<Arc<AtomicBool>>,
    results: Arc<Mutex<Vec<Entry>>>,
    scanned: Arc<AtomicUsize>,
    done: Arc<AtomicBool>,
    pub scope: Option<PathBuf>,
}

const MAX_RESULTS: usize = 5000;

impl Search {
    pub fn running(&self) -> bool {
        self.cancel.is_some() && !self.done.load(Ordering::Relaxed)
    }

    pub fn stop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn start(&mut self, scope: &Path, ctx: &egui::Context) {
        self.stop();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        self.results = Arc::new(Mutex::new(Vec::new()));
        self.scanned = Arc::new(AtomicUsize::new(0));
        self.done = Arc::new(AtomicBool::new(false));
        self.scope = Some(scope.to_path_buf());
        let (results, scanned, done) = (self.results.clone(), self.scanned.clone(), self.done.clone());
        let filters = self.filters.clone();
        let scope = scope.to_path_buf();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let needle = fold(filters.query.trim());
            let now = SystemTime::now();
            let mut stack = vec![scope];
            let mut last_repaint = std::time::Instant::now();
            while let Some(folder) = stack.pop() {
                if cancel.load(Ordering::Relaxed) || results.lock().map_or(true, |r| r.len() >= MAX_RESULTS) {
                    break;
                }
                let Ok(read) = std::fs::read_dir(&folder) else { continue };
                for item in read.flatten() {
                    let path = item.path();
                    let Some(entry) = pyegui_core::watcher::entry(&path) else { continue };
                    scanned.fetch_add(1, Ordering::Relaxed);
                    if entry.is_dir && !item.file_type().is_ok_and(|t| t.is_symlink()) && (filters.hidden || !entry.hidden) {
                        stack.push(path.clone());
                    }
                    if matches(&entry, &filters, &needle, now) {
                        if let Ok(mut r) = results.lock() {
                            r.push(entry);
                        }
                    }
                    if last_repaint.elapsed() > Duration::from_millis(150) {
                        ctx.request_repaint();
                        last_repaint = std::time::Instant::now();
                    }
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }
            done.store(true, Ordering::Relaxed);
            ctx.request_repaint();
        });
    }
}

pub fn pane(app: &mut FinderApp, ui: &mut egui::Ui) {
    let folder = app.folder().to_path_buf();
    let ctx = ui.ctx().clone();
    let mut go = false;
    {
        let f = &mut app.search_state.filters;
        let edit = ui.add(egui::TextEdit::singleline(&mut f.query).hint_text("Nom contient…").desired_width(f32::INFINITY));
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            go = true;
        }
        egui::ComboBox::from_id_salt("search-kind").selected_text(f.kind.label()).show_ui(ui, |ui| {
            for k in [Kind::All, Kind::Files, Kind::Folders, Kind::Pdf, Kind::Image] {
                ui.selectable_value(&mut f.kind, k, k.label());
            }
        });
        egui::Grid::new("search-grid").num_columns(2).show(ui, |ui| {
            ui.label("Taille min (Mo)");
            ui.add(egui::DragValue::new(&mut f.min_mb).range(0.0..=100_000.0).speed(0.1));
            ui.end_row();
            ui.label("Taille max (Mo)");
            ui.add(egui::DragValue::new(&mut f.max_mb).range(0.0..=100_000.0).speed(0.1));
            ui.end_row();
            ui.label("Modifié depuis (jours)");
            ui.add(egui::DragValue::new(&mut f.days).range(0..=36_500));
            ui.end_row();
            ui.label("PDF: pages min");
            ui.add(egui::DragValue::new(&mut f.min_pages).range(0..=100_000));
            ui.end_row();
        });
        ui.checkbox(&mut f.hidden, "Inclure les fichiers cachés");
    }
    ui.horizontal(|ui| {
        if ui.button(format!("Chercher dans « {} »", folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())).clicked() {
            go = true;
        }
        if app.search_state.running() && ui.button("Arrêter").clicked() {
            app.search_state.stop();
        }
    });
    if go {
        app.search_state.start(&folder, &ctx);
    }
    let results: Vec<Entry> = app.search_state.results.lock().map(|r| r.clone()).unwrap_or_default();
    let state = if app.search_state.running() {
        format!("… {} examinés", app.search_state.scanned.load(Ordering::Relaxed))
    } else {
        format!("{} résultat(s), {} examinés", results.len(), app.search_state.scanned.load(Ordering::Relaxed))
    };
    ui.weak(state);
    ui.separator();
    let mut reveal: Option<PathBuf> = None;
    let mut open: Option<PathBuf> = None;
    egui::ScrollArea::vertical().show_rows(ui, 38.0, results.len(), |ui, range| {
        for entry in &results[range] {
            let response = ui.add(egui::Button::new(RichText::new(&entry.name)).frame(false).min_size(egui::vec2(ui.available_width(), 0.0)));
            ui.weak(entry.path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
            if response.clicked() {
                reveal = Some(entry.path.clone());
            }
            if response.double_clicked() {
                open = Some(entry.path.clone());
            }
        }
    });
    if let Some(path) = reveal {
        app.reveal_path(&path);
    }
    if let Some(path) = open {
        app.open(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_compared_without_case_or_accents() {
        assert_eq!(fold("Écoles Été"), "ecoles ete");
        assert!(fold("Façade.PDF").contains(&fold("facade")));
    }

    #[test]
    fn filters_combine() {
        let entry = Entry { path: "/a/Rapport.pdf".into(), name: "Rapport.pdf".into(), is_dir: false, size: 3_000_000, modified: Some(SystemTime::now()), hidden: false };
        let now = SystemTime::now();
        let f = Filters { query: "rapp".into(), kind: Kind::Pdf, min_mb: 1.0, ..Default::default() };
        assert!(matches(&entry, &f, &fold(&f.query), now));
        let f = Filters { query: "rapp".into(), kind: Kind::Folders, ..Default::default() };
        assert!(!matches(&entry, &f, &fold(&f.query), now));
        let f = Filters { max_mb: 1.0, ..Default::default() };
        assert!(!matches(&entry, &f, "", now));
    }
}
