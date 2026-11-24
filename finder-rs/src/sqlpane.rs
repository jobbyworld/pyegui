//! The "Base SQL" pane: the tables of the database of the tags and of the index, the rows of the one
//! picked or of any query, to see what the apps record. The base is opened read-only: nothing can be
//! changed from here (same pane as the one of `examples/finder.py`).

use std::time::Instant;

use eframe::egui::{self, RichText};
use rusqlite::{types::ValueRef, Connection, OpenFlags};

use crate::tagstore::default_path;

const LIMIT: usize = 200;

#[derive(Default)]
pub struct SqlState {
    table: usize,
    query: String,
    /// the table the rows were read for (`None`: read again)
    loaded: Option<usize>,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    error: String,
    more: bool,
    ms: f64,
}

fn open() -> rusqlite::Result<Connection> {
    let db = Connection::open_with_flags(default_path(), OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    Ok(db)
}

fn tables(db: &Connection) -> Vec<String> {
    let Ok(mut statement) = db.prepare("SELECT name FROM sqlite_master WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' ORDER BY name") else { return vec![] };
    statement.query_map([], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

fn cell(value: ValueRef) -> String {
    match value {
        ValueRef::Null => "NULL".into(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Text(t) => String::from_utf8_lossy(t).replace('\n', " ").chars().take(300).collect(),
        ValueRef::Blob(b) => format!("<{} octets>", b.len()),
    }
}

impl SqlState {
    fn run(&mut self, db: &Connection) {
        let started = Instant::now();
        self.headers.clear();
        self.rows.clear();
        self.error.clear();
        self.more = false;
        let result: rusqlite::Result<()> = (|| {
            let mut statement = db.prepare(&self.query)?;
            let columns = statement.column_count();
            self.headers = statement.column_names().into_iter().map(String::from).collect();
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                if self.rows.len() == LIMIT {
                    self.more = true;
                    break;
                }
                self.rows.push((0..columns).map(|i| row.get_ref(i).map(cell).unwrap_or_default()).collect());
            }
            Ok(())
        })();
        if let Err(e) = result {
            self.error = e.to_string();
        }
        self.ms = started.elapsed().as_secs_f64() * 1000.0;
    }
}

pub fn pane(state: &mut SqlState, ui: &mut egui::Ui) {
    let db = match open() {
        Ok(db) => db,
        Err(e) => {
            ui.colored_label(egui::Color32::from_rgb(239, 68, 68), e.to_string());
            return;
        }
    };
    let names = tables(&db);
    if names.is_empty() {
        ui.weak("La base est vide");
        return;
    }
    ui.label(RichText::new(default_path().display().to_string()).small().weak());
    state.table = state.table.min(names.len() - 1);
    let before = state.table;
    egui::ComboBox::from_label("Table").selected_text(&names[state.table]).show_ui(ui, |ui| {
        for (i, name) in names.iter().enumerate() {
            ui.selectable_value(&mut state.table, i, name);
        }
    });
    if before != state.table || state.loaded != Some(state.table) {
        state.loaded = Some(state.table);
        state.query = format!("SELECT * FROM \"{}\" LIMIT {LIMIT}", names[state.table]);
        state.run(&db);
    }
    ui.add(egui::TextEdit::singleline(&mut state.query).hint_text("SELECT ...").desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
    ui.horizontal(|ui| {
        let name = names[state.table].clone();
        if ui.button("Exécuter").clicked() {
            state.run(&db);
        }
        if ui.button("Compter").clicked() {
            state.query = format!("SELECT COUNT(*) AS lignes FROM \"{name}\"");
            state.run(&db);
        }
        if ui.button("Schéma").clicked() {
            state.query = format!("PRAGMA table_info(\"{name}\")");
            state.run(&db);
        }
    });
    if !state.error.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(239, 68, 68), &state.error);
        return;
    }
    ui.weak(format!("{}{} lignes · {:.0} ms", state.rows.len(), if state.more { "+" } else { "" }, state.ms));
    egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
        egui::Grid::new("sql-rows").striped(true).show(ui, |ui| {
            for h in &state.headers {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for row in &state.rows {
                for value in row {
                    ui.label(RichText::new(value).monospace().small());
                }
                ui.end_row();
            }
        });
    });
}
