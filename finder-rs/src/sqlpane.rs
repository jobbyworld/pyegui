//! The "Base SQL" pane: the tables of the database of the tags and of the index, the rows of the one
//! picked or of any query, to see what the apps record. The base is opened read-only: nothing can be
//! changed from here (same pane as the one of `examples/finder.py`).

use std::time::Instant;

use eframe::egui::{self, RichText};
use pyegui_core::columns::{ColumnsState, DataType};
use pyegui_core::table::{self, TableOptions};
use rusqlite::{types::ValueRef, Connection, OpenFlags};

use crate::tagstore::default_path;

const LIMIT: usize = 200;
/// the columns of the base that hold a date (seconds since the epoch): shown as YYYY-MM-DD HH:MM:SS
const DATE_COLUMNS: [&str; 8] = ["last_opened", "modified", "last_seen", "last_full_scan", "created", "read_at", "updated", "mtime"];
/// a column with at most this many different values is filtered by choice
const CHOICE_MAX: usize = 12;

pub struct SqlState {
    table: usize,
    query: String,
    /// the table the rows were read for (`None`: read again)
    loaded: Option<usize>,
    headers: Vec<String>,
    types: Vec<DataType>,
    rows: Vec<Vec<String>>,
    /// sort, filters and widths of the columns of the result (new for each query)
    columns: ColumnsState,
    error: String,
    more: bool,
    ms: f64,
}

impl Default for SqlState {
    fn default() -> Self {
        SqlState {
            table: 0,
            query: String::new(),
            loaded: None,
            headers: vec![],
            types: vec![],
            rows: vec![],
            columns: ColumnsState::new(""),
            error: String::new(),
            more: false,
            ms: 0.0,
        }
    }
}

/// A value read from the base, before it is decoded for the cell
enum Raw {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(usize),
}

impl Raw {
    fn number(&self) -> Option<f64> {
        match self {
            Raw::Int(i) => Some(*i as f64),
            Raw::Real(f) => Some(*f),
            _ => None,
        }
    }
}

fn raw(value: ValueRef) -> Raw {
    match value {
        ValueRef::Null => Raw::Null,
        ValueRef::Integer(i) => Raw::Int(i),
        ValueRef::Real(f) => Raw::Real(f),
        ValueRef::Text(t) => Raw::Text(String::from_utf8_lossy(t).replace('\n', " ").chars().take(300).collect()),
        ValueRef::Blob(b) => Raw::Blob(b.len()),
    }
}

/// The type of a column from what it holds: a date (a known column of seconds), a number, a choice among
/// a few values, else a text
fn type_of(header: &str, values: &[&Raw]) -> DataType {
    let present: Vec<&&Raw> = values.iter().filter(|v| !matches!(***v, Raw::Null)).collect();
    if present.is_empty() {
        return DataType::Text;
    }
    let numbers: Vec<f64> = present.iter().filter_map(|v| v.number()).collect();
    if numbers.len() == present.len() {
        let date = DATE_COLUMNS.contains(&header.to_lowercase().as_str()) && numbers.iter().all(|&n| n > 1e8 && n < 4e9);
        return if date { DataType::Date } else { DataType::Number };
    }
    let mut distinct: Vec<String> = present.iter().map(|v| cell(v, DataType::Text)).collect();
    distinct.sort();
    distinct.dedup();
    if distinct.len() <= CHOICE_MAX { DataType::Choice } else { DataType::Text }
}

fn cell(value: &Raw, ty: DataType) -> String {
    match value {
        Raw::Null => "NULL".into(),
        Raw::Int(i) if ty == DataType::Date => date(*i as f64),
        Raw::Real(f) if ty == DataType::Date => date(*f),
        Raw::Int(i) => i.to_string(),
        Raw::Real(f) => f.to_string(),
        Raw::Text(t) => t.clone(),
        Raw::Blob(n) => format!("<{n} octets>"),
    }
}

/// Seconds since the epoch as a local date and time
fn date(seconds: f64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_opt(seconds as i64, 0).single().map_or_else(|| seconds.to_string(), |t| t.format("%Y-%m-%d %H:%M:%S").to_string())
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

impl SqlState {
    fn run(&mut self, db: &Connection) {
        let started = Instant::now();
        self.headers.clear();
        self.types.clear();
        self.rows.clear();
        self.columns = ColumnsState::new("");
        self.error.clear();
        self.more = false;
        let result: rusqlite::Result<()> = (|| {
            let mut statement = db.prepare(&self.query)?;
            let columns = statement.column_count();
            self.headers = statement.column_names().into_iter().map(String::from).collect();
            let mut rows = statement.query([])?;
            let mut got: Vec<Vec<Raw>> = vec![];
            while let Some(row) = rows.next()? {
                if got.len() == LIMIT {
                    self.more = true;
                    break;
                }
                got.push((0..columns).map(|i| row.get_ref(i).map(raw).unwrap_or(Raw::Null)).collect());
            }
            self.types = (0..columns).map(|c| type_of(&self.headers[c], &got.iter().map(|r| &r[c]).collect::<Vec<_>>())).collect();
            self.rows = got.iter().map(|r| r.iter().zip(&self.types).map(|(v, t)| cell(v, *t)).collect()).collect();
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
    if !state.headers.is_empty() {
        let defaults = vec![table::DEFAULT_WIDTH; state.headers.len()];
        let options = TableOptions { id: "sql-rows".into(), ..Default::default() };
        table::show(ui, &mut state.columns, &state.headers, &state.types, &defaults, &state.rows, None, &options);
    }
}
