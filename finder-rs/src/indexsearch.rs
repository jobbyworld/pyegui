//! The "Index" pane: a search in the SQLite index of the Finder (`tags.db`, tables `files` and `pdf_info`)
//! instead of a walk of the disk. It finds the files and folders that an indexation recorded, with the same
//! criteria as the "Recherche" pane (name without case or accents, kind, size, date) and the ones of the
//! PDF files read into `pdf_info`: pages, encryption, rights, title, author, keywords, producer, version.
//! The query is built from the criteria with bound values (nothing typed goes into the SQL text) and runs on
//! a read-only connection, so it is fast and cannot change the base.

use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui::{self, RichText};
use rusqlite::{functions::FunctionFlags, types::Value, Connection, OpenFlags};

use crate::app::FinderApp;
use crate::model;
use crate::search::{fold, Kind};
use crate::tagstore::{default_path, key};
use pyegui_core::meta::MetaType;

/// The most rows shown; the query asks for one more to know that there are others
const LIMIT: usize = 1000;
const IMAGE_EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg"];

/// What a criterion on a meta property asks of the value
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MetaOp {
    /// the property has a value (own or inherited)
    Has,
    /// the property has no value
    Missing,
    /// a text contains the typed text (case and accents ignored)
    Contains,
    /// the typed number, the typed day or the option (its id) is the value
    Equals,
    /// a number or a date from the typed one (included)
    AtLeast,
    /// a number or a date up to the typed one (included)
    AtMost,
    Yes,
    No,
}

impl MetaOp {
    pub fn label(self, ty: MetaType) -> &'static str {
        match (self, ty) {
            (MetaOp::Has, _) => "a une valeur",
            (MetaOp::Missing, _) => "est vide",
            (MetaOp::Contains, _) => "contient",
            (MetaOp::Equals, MetaType::Choice) => "est",
            (MetaOp::Equals, _) => "=",
            (MetaOp::AtLeast, MetaType::Date) => "à partir du",
            (MetaOp::AtLeast, _) => "≥",
            (MetaOp::AtMost, MetaType::Date) => "jusqu'au",
            (MetaOp::AtMost, _) => "≤",
            (MetaOp::Yes, _) => "oui",
            (MetaOp::No, _) => "non",
        }
    }

    /// The operators that mean something for a type
    pub fn for_type(ty: MetaType) -> &'static [MetaOp] {
        use MetaOp::*;
        match ty {
            MetaType::Text | MetaType::LongText => &[Contains, Equals, Has, Missing],
            MetaType::Number | MetaType::Date => &[Equals, AtLeast, AtMost, Has, Missing],
            MetaType::Bool => &[Yes, No, Has, Missing],
            MetaType::Choice => &[Equals, Has, Missing],
            MetaType::Formula => &[],
        }
    }

    fn needs_value(self) -> bool {
        matches!(self, MetaOp::Contains | MetaOp::Equals | MetaOp::AtLeast | MetaOp::AtMost)
    }
}

/// A criterion on a meta property: the value is the own one of the item or the one of its nearest folder that
/// has one (what the "Propriétés" pane shows)
#[derive(Clone)]
pub struct MetaCriterion {
    pub def: i64,
    pub ty: MetaType,
    pub op: MetaOp,
    /// typed text, or the id of the option of a choice
    pub text: String,
}

/// A yes / no / either criterion
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tri {
    Any,
    Yes,
    No,
}

impl Tri {
    fn label(self) -> &'static str {
        match self {
            Tri::Any => "Indifférent",
            Tri::Yes => "Oui",
            Tri::No => "Non",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Name,
    Size,
    Modified,
    Pages,
}

impl Sort {
    fn label(self) -> &'static str {
        match self {
            Sort::Name => "Nom",
            Sort::Size => "Taille",
            Sort::Modified => "Date de modification",
            Sort::Pages => "Pages (PDF)",
        }
    }
}

#[derive(Clone)]
pub struct IndexFilters {
    pub query: String,
    /// search the whole index, not only the folder shown
    pub whole_index: bool,
    pub kind: Kind,
    pub min_mb: f32,
    pub max_mb: f32,
    /// modified in the last N days, 0 for any date
    pub days: u32,
    // the PDF files: any of these keeps only the PDF files read into `pdf_info`
    pub min_pages: u32,
    pub max_pages: u32,
    pub encrypted: Tri,
    pub needs_password: Tri,
    /// the PDF files that could not be read
    pub unreadable: Tri,
    pub no_copy: bool,
    pub no_print: bool,
    pub no_modify: bool,
    pub title: String,
    pub author: String,
    /// in the keywords or the subject
    pub keywords: String,
    /// in the creator or the producer
    pub producer: String,
    pub version: String,
    /// created (date of the PDF) in the last N days, 0 for any date
    pub created_days: u32,
    pub sort: Sort,
    pub descending: bool,
    /// the criteria on the meta properties (all of them must hold)
    pub meta: Vec<MetaCriterion>,
}

impl Default for IndexFilters {
    fn default() -> Self {
        IndexFilters {
            query: String::new(),
            whole_index: false,
            kind: Kind::All,
            min_mb: 0.0,
            max_mb: 0.0,
            days: 0,
            min_pages: 0,
            max_pages: 0,
            encrypted: Tri::Any,
            needs_password: Tri::Any,
            unreadable: Tri::Any,
            no_copy: false,
            no_print: false,
            no_modify: false,
            title: String::new(),
            author: String::new(),
            keywords: String::new(),
            producer: String::new(),
            version: String::new(),
            created_days: 0,
            sort: Sort::Name,
            descending: false,
            meta: Vec::new(),
        }
    }
}

impl IndexFilters {
    /// Whether a criterion on the content of the PDF files is set (the result is then only PDF files)
    fn pdf_criteria(&self) -> bool {
        self.min_pages > 0
            || self.max_pages > 0
            || self.encrypted != Tri::Any
            || self.needs_password != Tri::Any
            || self.unreadable != Tri::Any
            || self.no_copy
            || self.no_print
            || self.no_modify
            || [&self.title, &self.author, &self.keywords, &self.producer, &self.version].iter().any(|t| !t.trim().is_empty())
            || self.created_days > 0
    }
}

/// A row of the result
#[derive(Clone, Debug)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub parent: String,
    pub is_dir: bool,
    pub size: Option<i64>,
    pub modified: Option<f64>,
    pub pages: Option<i64>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub encrypted: bool,
    pub error: Option<String>,
}

/// What the index holds, to tell how complete a search can be
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub entries: i64,
    pub pdfs: i64,
    pub pdfs_read: i64,
}

#[derive(Default)]
pub struct IndexSearch {
    pub filters: IndexFilters,
    results: Vec<Row>,
    more: bool,
    ran: bool,
    error: String,
    sql: String,
    ms: f64,
    stats: Option<Stats>,
    /// the folder of the last search, `None` for the whole index
    scope: Option<PathBuf>,
}

/// `text` as a LIKE pattern body: the characters that mean something to LIKE are escaped (escape character `\`)
fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// The query and its values for the criteria. `scope` is the folder (and its sub-folders) to search, `now` the
/// time for the "in the last N days" criteria, in seconds since the epoch
pub fn build_query(f: &IndexFilters, scope: Option<&Path>, now: f64) -> (String, Vec<Value>) {
    let mut conditions: Vec<String> = vec!["f.name IS NOT NULL".into()];
    let mut values: Vec<Value> = Vec::new();
    // a name or a text criterion: case and accents ignored (the `fold` function registered on the connection)
    let contains = |conditions: &mut Vec<String>, values: &mut Vec<Value>, columns: &[&str], text: &str| {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let pattern = format!("%{}%", escape(&fold(text)));
        let any: Vec<String> = columns.iter().map(|c| format!("fold({c}) LIKE ? ESCAPE '\\'")).collect();
        conditions.push(format!("({})", any.join(" OR ")));
        for _ in columns {
            values.push(Value::Text(pattern.clone()));
        }
    };
    contains(&mut conditions, &mut values, &["f.name"], &f.query);
    if let Some(scope) = scope {
        let folder = key(scope);
        let separator = std::path::MAIN_SEPARATOR;
        conditions.push("(f.path = ? OR f.path LIKE ? ESCAPE '\\')".into());
        values.push(Value::Text(folder.clone()));
        values.push(Value::Text(format!("{}%", escape(&format!("{}{separator}", folder.trim_end_matches(separator))))));
    }
    let pdf_only = f.pdf_criteria();
    match f.kind {
        Kind::All => {}
        Kind::Files => conditions.push("COALESCE(f.is_dir, 0) = 0".into()),
        Kind::Folders => conditions.push("f.is_dir = 1".into()),
        Kind::Pdf => conditions.push("COALESCE(f.is_dir, 0) = 0 AND LOWER(f.name) LIKE '%.pdf'".into()),
        Kind::Image => {
            let any: Vec<String> = IMAGE_EXTENSIONS.iter().map(|e| format!("LOWER(f.name) LIKE '%.{e}'")).collect();
            conditions.push(format!("(COALESCE(f.is_dir, 0) = 0 AND ({}))", any.join(" OR ")));
        }
    }
    // a size is the one of a file: the folders are left out when it is asked for
    if f.min_mb > 0.0 {
        conditions.push("COALESCE(f.is_dir, 0) = 0 AND f.size >= ?".into());
        values.push(Value::Integer((f.min_mb * 1_000_000.0) as i64));
    }
    if f.max_mb > 0.0 {
        conditions.push("COALESCE(f.is_dir, 0) = 0 AND f.size <= ?".into());
        values.push(Value::Integer((f.max_mb * 1_000_000.0) as i64));
    }
    if f.days > 0 {
        conditions.push("f.modified >= ?".into());
        values.push(Value::Real(now - f64::from(f.days) * 86_400.0));
    }
    if pdf_only {
        conditions.push("p.path IS NOT NULL".into());
    }
    if f.min_pages > 0 {
        conditions.push("p.pages >= ?".into());
        values.push(Value::Integer(i64::from(f.min_pages)));
    }
    if f.max_pages > 0 {
        conditions.push("p.pages <= ?".into());
        values.push(Value::Integer(i64::from(f.max_pages)));
    }
    let flag = |conditions: &mut Vec<String>, column: &str, tri: Tri| match tri {
        Tri::Any => {}
        Tri::Yes => conditions.push(format!("p.{column} = 1")),
        Tri::No => conditions.push(format!("COALESCE(p.{column}, 0) = 0")),
    };
    flag(&mut conditions, "encrypted", f.encrypted);
    flag(&mut conditions, "needs_password", f.needs_password);
    match f.unreadable {
        Tri::Any => {}
        Tri::Yes => conditions.push("(p.error IS NOT NULL AND p.error != '')".into()),
        Tri::No => conditions.push("(p.error IS NULL OR p.error = '')".into()),
    }
    for (on, column) in [(f.no_copy, "can_copy"), (f.no_print, "can_print"), (f.no_modify, "can_modify")] {
        if on {
            conditions.push(format!("p.{column} = 0"));
        }
    }
    contains(&mut conditions, &mut values, &["p.title"], &f.title);
    contains(&mut conditions, &mut values, &["p.author"], &f.author);
    contains(&mut conditions, &mut values, &["p.keywords", "p.subject"], &f.keywords);
    contains(&mut conditions, &mut values, &["p.producer", "p.creator"], &f.producer);
    if !f.version.trim().is_empty() {
        conditions.push("p.pdf_version LIKE ? ESCAPE '\\'".into());
        values.push(Value::Text(format!("%{}%", escape(f.version.trim()))));
    }
    if f.created_days > 0 {
        conditions.push("p.created >= ?".into());
        values.push(Value::Real(now - f64::from(f.created_days) * 86_400.0));
    }
    meta_conditions(f, now, &mut conditions, &mut values);
    let direction = if f.descending { "DESC" } else { "ASC" };
    let order = match f.sort {
        Sort::Name => format!("fold(f.name) {direction}"),
        Sort::Size => format!("f.size {direction}"),
        Sort::Modified => format!("f.modified {direction}"),
        // the files without a page count (not PDF, not read) come last whichever the direction
        Sort::Pages => format!("(p.pages IS NULL), p.pages {direction}"),
    };
    let sql = format!(
        "SELECT f.path, f.name, f.parent, COALESCE(f.is_dir, 0), f.size, f.modified, p.pages, p.title, p.author, COALESCE(p.encrypted, 0), p.error\n\
         FROM files f LEFT JOIN pdf_info p ON p.path = f.path\n\
         WHERE {}\n\
         ORDER BY {order}, f.path\n\
         LIMIT {}",
        conditions.join("\n  AND "),
        LIMIT + 1
    );
    (sql, values)
}

/// The value of a property for the row `f`: its own, or the one of the nearest folder above that has one
const META_VALUE: &str = "(SELECT m.value FROM meta_values m WHERE m.def_id = ? AND (m.path = f.path OR substr(f.path, 1, length(m.path) + 1) = m.path || ?) ORDER BY length(m.path) DESC LIMIT 1)";

/// The conditions of the criteria on the meta properties. A criterion whose typed value is not a number or a day
/// is left out.
fn meta_conditions(f: &IndexFilters, now: f64, conditions: &mut Vec<String>, values: &mut Vec<Value>) {
    let today = chrono::DateTime::from_timestamp(now as i64, 0).map_or_else(|| chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(), |d| d.date_naive());
    for c in &f.meta {
        let text = c.text.trim();
        if c.op.needs_value() && text.is_empty() {
            continue;
        }
        let separator = std::path::MAIN_SEPARATOR.to_string();
        let mut params: Vec<Value> = vec![Value::Integer(c.def), Value::Text(separator)];
        let value = META_VALUE;
        let condition = match c.op {
            MetaOp::Has => format!("{value} IS NOT NULL"),
            MetaOp::Missing => format!("{value} IS NULL"),
            MetaOp::Yes => format!("{value} = '1'"),
            MetaOp::No => format!("{value} = '0'"),
            MetaOp::Contains => {
                params.push(Value::Text(format!("%{}%", escape(&fold(text)))));
                format!("fold({value}) LIKE ? ESCAPE '\\'")
            }
            MetaOp::Equals | MetaOp::AtLeast | MetaOp::AtMost => {
                let compare = match c.op {
                    MetaOp::AtLeast => ">=",
                    MetaOp::AtMost => "<=",
                    _ => "=",
                };
                match c.ty {
                    MetaType::Number | MetaType::Date => {
                        let n = if c.ty == MetaType::Date {
                            pyegui_core::date_input::parse_date(text, today).map(|d| d.and_time(chrono::NaiveTime::MIN).and_utc().timestamp() as f64)
                        } else {
                            text.replace(',', ".").parse::<f64>().ok().filter(|n| n.is_finite())
                        };
                        let Some(n) = n else { continue };
                        params.push(Value::Real(n));
                        format!("CAST({value} AS REAL) {compare} ?")
                    }
                    MetaType::Choice => {
                        let Ok(id) = text.parse::<i64>() else { continue };
                        params.push(Value::Text(format!("%,{id},%")));
                        // one id (`3`) or a list (`[3,5]`)
                        format!("(',' || replace(replace(replace({value}, '[', ''), ']', ''), ' ', '') || ',') LIKE ?")
                    }
                    _ => {
                        params.push(Value::Text(text.to_owned()));
                        format!("{value} = ?")
                    }
                }
            }
        };
        conditions.push(condition);
        values.extend(params);
    }
}

/// The function `fold(text)` of the queries: lower case without accents, like the "Recherche" pane
fn register_fold(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function("fold", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|s| fold(&s))))
}

/// Runs the query of the criteria: the rows (at most `LIMIT`), whether there are more, and the SQL
pub fn run(db: &Connection, filters: &IndexFilters, scope: Option<&Path>, now: f64) -> rusqlite::Result<(Vec<Row>, bool, String)> {
    register_fold(db)?;
    let (sql, values) = build_query(filters, scope, now);
    let mut statement = db.prepare(&sql)?;
    let mut rows: Vec<Row> = statement
        .query_map(rusqlite::params_from_iter(values.iter()), |r| {
            Ok(Row {
                path: PathBuf::from(r.get::<_, String>(0)?),
                name: r.get(1)?,
                parent: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                is_dir: r.get::<_, i64>(3)? != 0,
                size: r.get(4)?,
                modified: r.get(5)?,
                pages: r.get(6)?,
                title: r.get(7)?,
                author: r.get(8)?,
                encrypted: r.get::<_, i64>(9)? != 0,
                error: r.get::<_, Option<String>>(10)?.filter(|e| !e.is_empty()),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let more = rows.len() > LIMIT;
    rows.truncate(LIMIT);
    Ok((rows, more, sql))
}

fn read_stats(db: &Connection) -> rusqlite::Result<Stats> {
    let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0));
    Ok(Stats {
        entries: count("SELECT COUNT(*) FROM files WHERE name IS NOT NULL")?,
        pdfs: count("SELECT COUNT(*) FROM files WHERE name IS NOT NULL AND COALESCE(is_dir, 0) = 0 AND LOWER(name) LIKE '%.pdf'")?,
        pdfs_read: count("SELECT COUNT(*) FROM files f JOIN pdf_info p ON p.path = f.path WHERE f.name IS NOT NULL")?,
    })
}

fn open() -> rusqlite::Result<Connection> {
    let db = Connection::open_with_flags(default_path(), OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    Ok(db)
}

impl IndexSearch {
    /// Reads the index and runs the search for the criteria
    fn search(&mut self, folder: &Path) {
        let started = Instant::now();
        self.ran = true;
        self.error.clear();
        self.results.clear();
        self.more = false;
        let scope = (!self.filters.whole_index).then(|| folder.to_path_buf());
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
        let outcome = open().and_then(|db| {
            self.stats = read_stats(&db).ok();
            run(&db, &self.filters, scope.as_deref(), now)
        });
        match outcome {
            Ok((rows, more, sql)) => {
                self.results = rows;
                self.more = more;
                self.sql = sql;
            }
            Err(e) => {
                self.error = format!("{e} (l'index est absent, vide ou ancien : indexez un dossier)");
            }
        }
        self.scope = scope;
        self.ms = started.elapsed().as_secs_f64() * 1000.0;
    }
}

/// The line under a result: what the index knows of the item
fn details(row: &Row) -> String {
    let mut parts: Vec<String> = Vec::new();
    if row.is_dir {
        parts.push("Dossier".into());
    } else if let Some(size) = row.size {
        parts.push(model::human_size(size.max(0) as u64));
    }
    if let Some(pages) = row.pages {
        parts.push(format!("{pages} page{}", if pages > 1 { "s" } else { "" }));
    }
    let who = [row.author.as_deref(), row.title.as_deref()].iter().flatten().filter(|t| !t.is_empty()).map(|t| t.to_string()).collect::<Vec<_>>().join(" — ");
    if !who.is_empty() {
        parts.push(who);
    }
    if row.encrypted {
        parts.push("chiffré".into());
    }
    if let Some(error) = &row.error {
        parts.push(format!("illisible : {error}"));
    }
    parts.join("  ·  ")
}

fn tri_combo(ui: &mut egui::Ui, id: &str, value: &mut Tri) {
    egui::ComboBox::from_id_salt(id).selected_text(value.label()).show_ui(ui, |ui| {
        for t in [Tri::Any, Tri::Yes, Tri::No] {
            ui.selectable_value(value, t, t.label());
        }
    });
}

pub fn pane(app: &mut FinderApp, ui: &mut egui::Ui) {
    let folder = app.folder().to_path_buf();
    let folder_name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| folder.to_string_lossy().into_owned());
    let mut go = false;
    // the name, the scope and the buttons stay at the top: the result is under them, whatever the criteria open
    {
        let f = &mut app.index_search.filters;
        let edit = ui.add(egui::TextEdit::singleline(&mut f.query).hint_text("Nom contient…").desired_width(f32::INFINITY));
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            go = true;
        }
        ui.checkbox(&mut f.whole_index, "Tout l'index (sinon le dossier affiché et ses sous-dossiers)");
    }
    ui.horizontal_wrapped(|ui| {
        let label = if app.index_search.filters.whole_index { "Chercher dans l'index".to_string() } else { format!("Chercher dans « {folder_name} »") };
        if ui.button(label).clicked() {
            go = true;
        }
        if ui.button("Effacer les critères").clicked() {
            let (whole, sort) = (app.index_search.filters.whole_index, app.index_search.filters.sort);
            app.index_search.filters = IndexFilters { whole_index: whole, sort, ..Default::default() };
        }
        if ui.add_enabled(!app.is_indexing(), egui::Button::new(format!("Indexer « {folder_name} »"))).on_hover_text("Enregistre le dossier et ses PDF (pages, métadonnées) dans l'index").clicked() {
            app.index_folder(folder.clone());
        }
    });
    {
        let f = &mut app.index_search.filters;
        egui::CollapsingHeader::new("Critères").id_salt("index-general").default_open(false).show(ui, |ui| {
            egui::Grid::new("index-grid").num_columns(2).show(ui, |ui| {
                ui.label("Type");
                egui::ComboBox::from_id_salt("index-kind").selected_text(f.kind.label()).show_ui(ui, |ui| {
                    for k in [Kind::All, Kind::Files, Kind::Folders, Kind::Pdf, Kind::Image] {
                        ui.selectable_value(&mut f.kind, k, k.label());
                    }
                });
                ui.end_row();
                ui.label("Taille min (Mo)");
                ui.add(egui::DragValue::new(&mut f.min_mb).range(0.0..=100_000.0).speed(0.1));
                ui.end_row();
                ui.label("Taille max (Mo)");
                ui.add(egui::DragValue::new(&mut f.max_mb).range(0.0..=100_000.0).speed(0.1));
                ui.end_row();
                ui.label("Modifié depuis (jours)");
                ui.add(egui::DragValue::new(&mut f.days).range(0..=36_500));
                ui.end_row();
                ui.label("Trier par");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("index-sort").selected_text(f.sort.label()).show_ui(ui, |ui| {
                        for s in [Sort::Name, Sort::Size, Sort::Modified, Sort::Pages] {
                            ui.selectable_value(&mut f.sort, s, s.label());
                        }
                    });
                    ui.checkbox(&mut f.descending, "décroissant");
                });
                ui.end_row();
            });
        });
        // a dot in the title tells that a PDF criterion is set while the header is closed
        let title = if f.pdf_criteria() { "Critères PDF  ●" } else { "Critères PDF" };
        egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt("index-pdf").default_open(false).show(ui, |ui| {
            ui.weak("Un critère ci-dessous ne garde que les PDF lus par l'indexation.");
            egui::Grid::new("index-pdf-grid").num_columns(2).show(ui, |ui| {
                ui.label("Pages min");
                ui.add(egui::DragValue::new(&mut f.min_pages).range(0..=100_000));
                ui.end_row();
                ui.label("Pages max");
                ui.add(egui::DragValue::new(&mut f.max_pages).range(0..=100_000));
                ui.end_row();
                ui.label("Chiffré");
                tri_combo(ui, "index-encrypted", &mut f.encrypted);
                ui.end_row();
                ui.label("Mot de passe requis");
                tri_combo(ui, "index-password", &mut f.needs_password);
                ui.end_row();
                ui.label("Illisible");
                tri_combo(ui, "index-unreadable", &mut f.unreadable);
                ui.end_row();
                ui.label("Créé depuis (jours)");
                ui.add(egui::DragValue::new(&mut f.created_days).range(0..=36_500));
                ui.end_row();
                ui.label("Version PDF");
                ui.add(egui::TextEdit::singleline(&mut f.version).hint_text("1.7").desired_width(120.0));
                ui.end_row();
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut f.no_copy, "Copie interdite");
                ui.checkbox(&mut f.no_print, "Impression interdite");
                ui.checkbox(&mut f.no_modify, "Modification interdite");
            });
            for (text, hint) in [(&mut f.title, "Titre contient…"), (&mut f.author, "Auteur contient…"), (&mut f.keywords, "Mots-clés ou sujet contient…"), (&mut f.producer, "Créé avec (créateur ou producteur) contient…")] {
                let edit = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(f32::INFINITY));
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = true;
                }
            }
        });
    }
    // the meta properties: one row per criterion, all of them must hold
    {
        let defs = app.tags.as_ref().map(|s| s.meta_defs()).unwrap_or_default();
        let f = &mut app.index_search.filters;
        let title = if f.meta.is_empty() { "Propriétés".to_string() } else { format!("Propriétés  ● {}", f.meta.len()) };
        egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt("index-meta").default_open(false).show(ui, |ui| {
            let searchable: Vec<_> = defs.iter().filter(|d| d.ty != MetaType::Formula).collect();
            if searchable.is_empty() {
                ui.weak("Aucune propriété définie (voir le gestionnaire des propriétés).");
                return;
            }
            ui.weak("La valeur est celle de l'élément ou, à défaut, celle de son dossier le plus proche.");
            let mut remove = None;
            for (i, c) in f.meta.iter_mut().enumerate() {
                let Some(def) = defs.iter().find(|d| d.id == c.def) else {
                    remove = Some(i);
                    continue;
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&def.name).strong());
                    egui::ComboBox::from_id_salt(("index-meta-op", i)).selected_text(c.op.label(c.ty)).show_ui(ui, |ui| {
                        for op in MetaOp::for_type(c.ty) {
                            ui.selectable_value(&mut c.op, *op, op.label(c.ty));
                        }
                    });
                    if c.op.needs_value() {
                        if c.ty == MetaType::Choice {
                            let shown = c.text.parse::<i64>().ok().and_then(|id| def.config.label_of(id)).unwrap_or("…").to_owned();
                            egui::ComboBox::from_id_salt(("index-meta-option", i)).selected_text(shown).show_ui(ui, |ui| {
                                for o in &def.config.options {
                                    ui.selectable_value(&mut c.text, o.id.to_string(), &o.label);
                                }
                            });
                        } else {
                            let hint = if c.ty == MetaType::Date { "15/03/2025, 3 derniers mois…" } else { "" };
                            let edit = ui.add(egui::TextEdit::singleline(&mut c.text).hint_text(hint).desired_width(140.0));
                            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                go = true;
                            }
                        }
                    }
                    if ui.small_button("✕").on_hover_text("Retirer ce critère").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                f.meta.remove(i);
            }
            ui.menu_button("+ Ajouter un critère", |ui| {
                for d in &searchable {
                    if ui.button(&d.name).clicked() {
                        let op = MetaOp::for_type(d.ty)[0];
                        f.meta.push(MetaCriterion { def: d.id, ty: d.ty, op, text: String::new() });
                        ui.close();
                    }
                }
            });
        });
    }
    if go {
        app.index_search.search(&folder);
    }
    let state = &app.index_search;
    if let Some(s) = state.stats {
        let missing = s.pdfs - s.pdfs_read;
        ui.weak(format!("Index : {} éléments, {} PDF dont {} lus{}", s.entries, s.pdfs, s.pdfs_read, if missing > 0 { format!(" ({missing} à lire : indexez leur dossier)") } else { String::new() }));
    }
    if !state.error.is_empty() {
        ui.colored_label(ui.visuals().error_fg_color, &state.error);
    } else if state.ran {
        let scope = state.scope.as_ref().map_or("tout l'index".to_string(), |s| format!("« {} »", s.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        ui.label(format!("{}{} résultat(s) dans {scope}, {:.0} ms", state.results.len(), if state.more { "+" } else { "" }, state.ms));
        if state.more {
            ui.weak(format!("Seuls les {LIMIT} premiers sont montrés : précisez les critères."));
        }
        egui::CollapsingHeader::new("SQL de la recherche").id_salt("index-sql").show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(&state.sql).monospace().small()).wrap());
            if ui.small_button("Copier").clicked() {
                ui.ctx().copy_text(state.sql.clone());
            }
        });
    }
    ui.separator();
    let results = &app.index_search.results;
    let mut reveal: Option<PathBuf> = None;
    let mut open: Option<PathBuf> = None;
    // The pane is already in a scroll area of the dock: only the cards in view are drawn, the others are spaces
    const ROW: f32 = 74.0;
    let step = ROW + ui.spacing().item_spacing.y;
    let top = ui.cursor().top();
    let clip = ui.clip_rect();
    let first = (((clip.top() - top) / step).floor().max(0.0) as usize).min(results.len());
    let last = ((((clip.bottom() - top) / step).ceil().max(0.0) as usize) + 1).min(results.len());
    ui.add_space(first as f32 * step);
    let now = crate::cards::now();
    for row in &results[first..last] {
        let card = crate::cards::Card {
            path: &row.path,
            is_dir: row.is_dir,
            name: &row.name,
            date: row.modified.map(|m| crate::cards::human_date(m, now)).unwrap_or_default(),
            lines: vec![row.parent.clone(), details(row)],
        };
        let response = crate::cards::show(ui, &card, false, Some(ROW));
        if response.clicked() {
            reveal = Some(row.path.clone());
        }
        if response.double_clicked() {
            open = Some(row.path.clone());
        }
    }
    ui.add_space((results.len() - last) as f32 * step);
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

    const NOW: f64 = 1_800_000_000.0;
    const DAY: f64 = 86_400.0;

    /// An index in memory: three PDF files read, a text file and a folder
    fn index() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(crate::tagstore::SCHEMA).unwrap();
        crate::tagstore::add_index_columns(&db);
        let file = |path: &str, name: &str, parent: &str, is_dir: i64, size: i64, modified: f64| {
            db.execute("INSERT INTO files (path, name, parent, is_dir, size, modified) VALUES (?, ?, ?, ?, ?, ?)", rusqlite::params![path, name, parent, is_dir, size, modified]).unwrap();
        };
        file("/a", "a", "/", 1, 0, NOW);
        file("/a/École.pdf", "École.pdf", "/a", 0, 5_000_000, NOW - 2.0 * DAY);
        file("/a/notes.txt", "notes.txt", "/a", 0, 100, NOW - 400.0 * DAY);
        file("/a/50%_off.pdf", "50%_off.pdf", "/a", 0, 2_000, NOW);
        file("/ab/Rapport.pdf", "Rapport.pdf", "/ab", 0, 9_000_000, NOW - 40.0 * DAY);
        file("/ab/Secret.pdf", "Secret.pdf", "/ab", 0, 1_000, NOW);
        let pdf = |path: &str, pages: i64, encrypted: i64, can_copy: i64, title: &str, author: &str, producer: &str, version: &str, created: f64| {
            db.execute(
                "INSERT INTO pdf_info (path, pages, encrypted, needs_password, can_copy, can_print, can_modify, title, author, producer, pdf_version, created, error) VALUES (?, ?, ?, 0, ?, 1, 1, ?, ?, ?, ?, ?, NULL)",
                rusqlite::params![path, pages, encrypted, can_copy, title, author, producer, version, created],
            )
            .unwrap();
        };
        pdf("/a/École.pdf", 120, 0, 1, "Plan de l'école", "Éloïse Martin", "LibreOffice 7", "1.7", NOW - 3.0 * DAY);
        pdf("/a/50%_off.pdf", 2, 0, 1, "Promo", "Bob", "Word", "1.4", NOW - 500.0 * DAY);
        pdf("/ab/Rapport.pdf", 30, 0, 1, "Rapport annuel", "Alice", "Word", "1.5", NOW - 40.0 * DAY);
        pdf("/ab/Secret.pdf", 4, 1, 0, "Secret", "Alice", "Acrobat", "1.7", NOW);
        db.execute("INSERT INTO pdf_info (path, error) VALUES ('/ab/Secret.pdf.bad', 'not a pdf')", []).unwrap();
        db
    }

    fn names(db: &Connection, f: &IndexFilters) -> Vec<String> {
        run(db, f, None, NOW).unwrap().0.into_iter().map(|r| r.name).collect()
    }

    #[test]
    fn the_name_is_found_without_case_or_accents() {
        let db = index();
        let f = IndexFilters { query: "ECOLE".into(), ..Default::default() };
        assert_eq!(names(&db, &f), ["École.pdf"]);
    }

    #[test]
    fn like_characters_typed_are_not_patterns() {
        let db = index();
        assert_eq!(names(&db, &IndexFilters { query: "50%".into(), ..Default::default() }), ["50%_off.pdf"]);
        assert_eq!(names(&db, &IndexFilters { query: "_".into(), ..Default::default() }), ["50%_off.pdf"]);
    }

    #[test]
    fn the_pages_of_the_pdf_files_bound_the_result() {
        let db = index();
        assert_eq!(names(&db, &IndexFilters { min_pages: 25, ..Default::default() }), ["École.pdf", "Rapport.pdf"]);
        assert_eq!(names(&db, &IndexFilters { min_pages: 3, max_pages: 30, ..Default::default() }), ["Rapport.pdf", "Secret.pdf"]);
    }

    #[test]
    fn a_pdf_criterion_leaves_out_the_other_files() {
        let db = index();
        let rows = names(&db, &IndexFilters { created_days: 36_000, ..Default::default() });
        assert!(!rows.contains(&"notes.txt".to_string()) && !rows.contains(&"a".to_string()));
    }

    #[test]
    fn encryption_rights_and_text_criteria_combine() {
        let db = index();
        assert_eq!(names(&db, &IndexFilters { encrypted: Tri::Yes, ..Default::default() }), ["Secret.pdf"]);
        assert_eq!(names(&db, &IndexFilters { no_copy: true, ..Default::default() }), ["Secret.pdf"]);
        assert_eq!(names(&db, &IndexFilters { author: "alice".into(), encrypted: Tri::No, ..Default::default() }), ["Rapport.pdf"]);
        assert_eq!(names(&db, &IndexFilters { author: "eloise".into(), ..Default::default() }), ["École.pdf"]);
        assert_eq!(names(&db, &IndexFilters { producer: "word".into(), version: "1.5".into(), ..Default::default() }), ["Rapport.pdf"]);
    }

    #[test]
    fn size_and_date_apply_to_the_files() {
        let db = index();
        assert_eq!(names(&db, &IndexFilters { min_mb: 4.0, ..Default::default() }), ["École.pdf", "Rapport.pdf"]);
        let recent = names(&db, &IndexFilters { days: 10, kind: Kind::Files, ..Default::default() });
        assert_eq!(recent, ["50%_off.pdf", "École.pdf", "Secret.pdf"]);
        assert_eq!(names(&db, &IndexFilters { kind: Kind::Folders, ..Default::default() }), ["a"]);
    }

    #[test]
    fn sorting_by_pages_puts_the_unread_last() {
        let db = index();
        let f = IndexFilters { sort: Sort::Pages, descending: true, kind: Kind::Files, ..Default::default() };
        let rows = names(&db, &f);
        assert_eq!(&rows[..4], ["École.pdf", "Rapport.pdf", "Secret.pdf", "50%_off.pdf"]);
        assert_eq!(rows[4], "notes.txt");
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_scope_does_not_take_the_folders_that_share_its_prefix() {
        let db = index();
        let (rows, _, _) = run(&db, &IndexFilters { kind: Kind::Pdf, ..Default::default() }, Some(Path::new("/a")), NOW).unwrap();
        let names: Vec<String> = rows.into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["50%_off.pdf", "École.pdf"]);
    }

    #[test]
    fn more_rows_than_the_limit_are_reported() {
        let db = index();
        db.execute_batch("WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 1100) INSERT INTO files (path, name, parent, is_dir, size, modified) SELECT '/z/' || i, 'f' || i, '/z', 0, 1, 0 FROM n").unwrap();
        let (rows, more, _) = run(&db, &IndexFilters::default(), None, NOW).unwrap();
        assert_eq!(rows.len(), LIMIT);
        assert!(more);
    }

    #[test]
    fn a_meta_property_is_read_on_the_item_or_its_nearest_folder() {
        let db = index();
        db.execute_batch(
            "INSERT INTO meta_defs (id, name, type, config) VALUES (1, 'Client', 'text', '{}'), (2, 'Pages', 'number', '{}'), (3, 'Statut', 'choice', '{\"multiple\":true}'), (4, 'Revu', 'bool', '{}');
             INSERT INTO meta_values (path, def_id, value) VALUES ('/a', 1, 'Écoles du Sud'), ('/a/notes.txt', 1, 'Autre'), ('/ab/Rapport.pdf', 2, '12'),
                ('/ab/Rapport.pdf', 3, '[2,5]'), ('/ab/Secret.pdf', 3, '5'), ('/a', 4, '1'), ('/a/notes.txt', 4, '0');",
        )
        .unwrap();
        let c = |def, ty, op, text: &str| MetaCriterion { def, ty, op, text: text.into() };
        let with = |m: Vec<MetaCriterion>| IndexFilters { meta: m, kind: Kind::Files, ..Default::default() };
        // inherited from /a, but the own value of notes.txt wins
        assert_eq!(names(&db, &with(vec![c(1, MetaType::Text, MetaOp::Contains, "ecole")])), ["50%_off.pdf", "École.pdf"]);
        assert_eq!(names(&db, &with(vec![c(1, MetaType::Text, MetaOp::Equals, "Autre")])), ["notes.txt"]);
        assert_eq!(names(&db, &with(vec![c(2, MetaType::Number, MetaOp::AtLeast, "10")])), ["Rapport.pdf"]);
        assert_eq!(names(&db, &with(vec![c(2, MetaType::Number, MetaOp::AtMost, "10")])), Vec::<String>::new());
        assert_eq!(names(&db, &with(vec![c(3, MetaType::Choice, MetaOp::Equals, "5")])), ["Rapport.pdf", "Secret.pdf"]);
        assert_eq!(names(&db, &with(vec![c(3, MetaType::Choice, MetaOp::Equals, "2")])), ["Rapport.pdf"]);
        assert_eq!(names(&db, &with(vec![c(4, MetaType::Bool, MetaOp::Yes, "")])), ["50%_off.pdf", "École.pdf"]);
        assert_eq!(names(&db, &with(vec![c(4, MetaType::Bool, MetaOp::No, "")])), ["notes.txt"]);
        assert_eq!(names(&db, &with(vec![c(1, MetaType::Text, MetaOp::Missing, "")])), ["Rapport.pdf", "Secret.pdf"]);
        // two criteria must both hold; an empty value leaves the criterion out
        assert_eq!(names(&db, &with(vec![c(3, MetaType::Choice, MetaOp::Equals, "5"), c(2, MetaType::Number, MetaOp::Has, "")])), ["Rapport.pdf"]);
        assert_eq!(names(&db, &with(vec![c(1, MetaType::Text, MetaOp::Contains, "  ")])).len(), 5);
    }
}
