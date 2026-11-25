//! The debug log: events of the app (files recorded in the SQL base, PDF metadata read, mermaid diagrams
//! rendered...) written to the terminal (stderr) and to a file, only when it was turned on with `enable`.
//!
//! Off by default: `dlog!` costs one atomic read then. A line is `HH:MM:SS.mmm [category] message`.

use std::fmt::Display;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static ENABLED: AtomicBool = AtomicBool::new(false);
static FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// Turns the log on: the lines go to stderr and, with `path`, are appended to that file (its folder is
/// created). Calling it again changes the file.
pub fn enable(path: Option<&Path>) -> std::io::Result<()> {
    let file = match path {
        Some(p) => {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir)?;
            }
            Some(std::fs::OpenOptions::new().create(true).append(true).open(p)?)
        }
        None => None,
    };
    *FILE.lock().unwrap_or_else(|e| e.into_inner()) = file;
    ENABLED.store(true, Ordering::Relaxed);
    log("debug", format_args!("log started (pid {})", std::process::id()));
    Ok(())
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// The default file of the log of an app: `<folder of the user data>/<app>/debug.log`
pub fn default_path(app: &str) -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default();
    home.join(".local").join("share").join(app).join("debug.log")
}

/// Writes one line, whatever thread calls it. Does nothing while the log is off.
pub fn log(category: &str, message: impl Display) {
    if !enabled() {
        return;
    }
    let line = format!("{} [{}] {}\n", chrono::Local::now().format("%H:%M:%S%.3f"), category, message);
    // one write call per line so that the lines of two threads do not mix
    let _ = std::io::stderr().write_all(line.as_bytes());
    if let Some(file) = FILE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        let _ = file.write_all(line.as_bytes());
    }
}

/// `dlog!("sqlite", "{} files in {}", n, folder)`: formats only when the log is on
#[macro_export]
macro_rules! dlog {
    ($category:expr, $($arg:tt)*) => {
        if $crate::debuglog::enabled() {
            $crate::debuglog::log($category, format_args!($($arg)*));
        }
    };
}
