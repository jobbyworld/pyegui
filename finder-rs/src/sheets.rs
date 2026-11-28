//! The style sheets of `examples/style_sheets`, like `examples/finder.py`: the tokens of the sheet go to the
//! framework (`pyegui_core::sheet::apply_sheet`), and its "app" section is what the app reads for its own look
//! (the strokes of the boxes, the roundings, the fills under the mouse, the dialogs, the glass, the terminals)
//! through [`stroke`], [`color`], [`number`] and [`select_fill`], with the defaults of `APP_STYLE` of finder.py.
//!
//! Where the sheets are: a folder `style_sheets` next to the executable, `Contents/Resources/style_sheets` in the
//! macOS bundle, `FINDER_SHEETS` if set, `examples/style_sheets` of the repository the app was built in. Without
//! any of them the sheets that were in the repository at build time are embedded in the executable.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{self, Color32};
use pyegui_core::sheet::{self, StyleSheet};
use serde_json::Value;

/// The sheets of the repository, for an executable that is run without their folder
const EMBEDDED: [(&str, &str); 6] = [
    ("contraste", include_str!("../../examples/style_sheets/contraste.json")),
    ("finder", include_str!("../../examples/style_sheets/finder.json")),
    ("ocean", include_str!("../../examples/style_sheets/ocean.json")),
    ("papier", include_str!("../../examples/style_sheets/papier.json")),
    ("rose", include_str!("../../examples/style_sheets/rose.json")),
    ("terminal", include_str!("../../examples/style_sheets/terminal.json")),
];

/// The folder of the sheets, if one is found
pub fn folder() -> Option<PathBuf> {
    static FOLDER: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOLDER
        .get_or_init(|| {
            let mut candidates: Vec<PathBuf> = Vec::new();
            if let Some(dir) = std::env::var_os("FINDER_SHEETS") {
                candidates.push(PathBuf::from(dir));
            }
            if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
                candidates.push(exe_dir.join("style_sheets"));
                candidates.push(exe_dir.join("../Resources/style_sheets"));
            }
            candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/style_sheets"));
            candidates.into_iter().find(|d| d.is_dir())
        })
        .clone()
}

/// The names of the sheets, sorted
pub fn names() -> Vec<String> {
    let found = folder().map(sheet::list).unwrap_or_default();
    if found.is_empty() {
        EMBEDDED.iter().map(|(n, _)| n.to_string()).collect()
    } else {
        found
    }
}

/// A sheet by its name (read once), None when it is missing or not valid
fn load(name: &str) -> Option<Arc<StyleSheet>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<StyleSheet>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .entry(name.to_owned())
        .or_insert_with(|| {
            let from_folder = folder().map(|d| d.join(format!("{name}.json"))).filter(|p| p.is_file());
            let sheet = match from_folder {
                Some(path) => StyleSheet::load(&path),
                None => EMBEDDED.iter().find(|(n, _)| *n == name).map(|(_, text)| StyleSheet::parse(text)).unwrap_or_else(|| Err("unknown sheet".into())),
            };
            match sheet {
                Ok(s) => Some(Arc::new(s)),
                Err(e) => {
                    eprintln!("style sheet {name}: {e}");
                    None
                }
            }
        })
        .clone()
}

static CURRENT: Mutex<Option<Arc<StyleSheet>>> = Mutex::new(None);

/// The sheet the app is drawn with: "" is none
pub fn set_current(name: &str) {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = if name.is_empty() { None } else { load(name) };
}

pub fn current() -> Option<Arc<StyleSheet>> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Gives the tokens of the current sheet to the framework (after the accent and the glass are set in the
/// styles of both themes). The tokens of a previous sheet are forgotten, also when there is none now.
pub fn apply(ctx: &egui::Context) {
    match current() {
        Some(s) => {
            for path in sheet::apply_sheet(ctx, &s) {
                eprintln!("style sheet {}: unknown or invalid token {path}", s.name);
            }
        }
        None => sheet::reset_tokens(),
    }
}

/// A value of the "app" section of the current sheet for a theme
fn value(name: &str, dark: bool) -> Option<Value> {
    current().and_then(|s| sheet::app_value(&s, dark, name))
}

/// A stroke of the look of the app: (width, color), none without a sheet
pub fn stroke(name: &str, dark: bool) -> (f32, Color32) {
    value(name, dark).and_then(|v| sheet::stroke(&v).ok()).map(|s| (s.width, s.color)).unwrap_or((0.0, Color32::TRANSPARENT))
}

/// A color of the look of the app, as the sheet gives it (None without one)
pub fn color(name: &str, dark: bool) -> Option<Color32> {
    value(name, dark).and_then(|v| sheet::color(&v).ok())
}

/// A number of the look of the app: a rounding, a margin, a gap, a glass level, a blur (None when the sheet has none)
pub fn try_number(name: &str, dark: bool) -> Option<f32> {
    value(name, dark).and_then(|v| v.as_f64()).map(|v| v as f32)
}

/// A text of the look of the app: a mode, a path (None when the sheet has none)
pub fn text(name: &str, dark: bool) -> Option<String> {
    value(name, dark).and_then(|v| v.as_str().map(str::to_string))
}

/// A number of the look of the app, 0 when the sheet has none
pub fn number(name: &str, dark: bool) -> f32 {
    try_number(name, dark).unwrap_or(0.0)
}

/// The fill of an item picked or dragged over: the one of the sheet, else the accent at 25 %
pub fn select_fill(accent: Color32, dark: bool) -> Color32 {
    color("select_fill", dark).unwrap_or(Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 0x40))
}

/// The fill of the dialogs, the one of the sheet (None without one)
pub fn dialog_fill(dark: bool) -> Option<Color32> {
    color("dialog_fill", dark)
}

/// The colors of the terminals (light, dark) that the sheet gives under "terminal", None for the default ones
pub fn terminal(dark: bool) -> Option<Value> {
    value("terminal", dark).filter(Value::is_object)
}

/// The fill a region of the window takes from the sheet (`layout.<region>.sheet`, then `layout.sheet`)
pub fn layout_sheet(region: &str, dark: bool) -> Option<Color32> {
    sheet::layout_sheet(region, dark)
}
