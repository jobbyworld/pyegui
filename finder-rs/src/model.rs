//! What the views show: sorting, filtering, the history of the folders and the texts of the columns.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use pyegui_core::watcher::Entry;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SortKey {
    Name,
    Modified,
    Size,
    Kind,
}

/// The folders visited, with a position for back and forward
#[derive(Default)]
pub struct History {
    folders: Vec<PathBuf>,
    index: usize,
}

impl History {
    pub fn new(start: PathBuf) -> Self {
        History { folders: vec![start], index: 0 }
    }

    pub fn current(&self) -> &Path {
        &self.folders[self.index]
    }

    /// Goes to a folder: what was forward is forgotten. The same folder twice is one visit
    pub fn visit(&mut self, folder: PathBuf) {
        if folder == self.current() {
            return;
        }
        self.folders.truncate(self.index + 1);
        self.folders.push(folder);
        self.index += 1;
    }

    pub fn can_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.folders.len()
    }

    pub fn back(&mut self) {
        if self.can_back() {
            self.index -= 1;
        }
    }

    pub fn forward(&mut self) {
        if self.can_forward() {
            self.index += 1;
        }
    }
}

/// "Folder", "PNG image"... from the extension
pub fn kind_of(e: &Entry) -> String {
    if e.is_dir {
        return if e.path.extension().is_some_and(|x| x == "app") { "Application".into() } else { "Folder".into() };
    }
    let ext = e.path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
    let name = match ext.as_str() {
        "" => return "Document".into(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg" | "heic" | "tiff" => "image",
        "md" | "txt" | "rst" | "log" => "text",
        "pdf" => "document",
        "py" | "rs" | "js" | "ts" | "c" | "h" | "cpp" | "java" | "go" | "sh" | "toml" | "json" | "yaml" | "yml" => "source",
        "zip" | "gz" | "tar" | "7z" | "rar" | "dmg" => "archive",
        "mp3" | "wav" | "flac" | "m4a" => "audio",
        "mp4" | "mov" | "mkv" | "avi" => "video",
        _ => "file",
    };
    format!("{} {name}", ext.to_uppercase())
}

pub fn is_image(path: &Path) -> bool {
    let ext = path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg")
}

pub fn is_text(path: &Path) -> bool {
    let ext = path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(
        ext.as_str(),
        "md" | "txt" | "rst" | "log" | "py" | "rs" | "js" | "ts" | "c" | "h" | "cpp" | "java" | "go" | "sh" | "toml" | "json" | "yaml" | "yml" | "css" | "html" | "xml" | "csv" | "ini" | "cfg" | "feature"
    )
}

pub fn human_size(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{size} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn format_date(time: Option<SystemTime>) -> String {
    match time {
        Some(t) => chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M").to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_forgets_what_was_forward() {
        let mut h = History::new("/a".into());
        h.visit("/b".into());
        h.visit("/c".into());
        h.back();
        h.back();
        assert_eq!(h.current(), Path::new("/a"));
        h.visit("/d".into());
        assert!(!h.can_forward());
        h.back();
        assert_eq!(h.current(), Path::new("/a"));
    }

    #[test]
    fn sizes_are_human() {
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1500), "1.5 KB");
    }
}
