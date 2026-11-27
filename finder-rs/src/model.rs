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

/// The item that gets the selection once `gone` are removed from `items`: the one after the last of them, else the
/// one before the first (None when nothing is left)
pub fn next_after(items: &[PathBuf], gone: &[PathBuf]) -> Option<PathBuf> {
    let gone: std::collections::HashSet<&PathBuf> = gone.iter().collect();
    let last = items.iter().rposition(|p| gone.contains(p));
    let first = items.iter().position(|p| gone.contains(p));
    last.and_then(|i| items[i + 1..].iter().find(|p| !gone.contains(p)))
        .or_else(|| first.and_then(|i| items[..i].iter().rev().find(|p| !gone.contains(p))))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn the_selection_goes_to_the_next_item_after_a_delete() {
        let items = paths(&["a", "b", "c", "d"]);
        assert_eq!(next_after(&items, &paths(&["b"])), Some(PathBuf::from("c")));
        // several items: the one after the last of them, even when they are not together
        assert_eq!(next_after(&items, &paths(&["a", "c"])), Some(PathBuf::from("d")));
        // the last item goes: the one before the first
        assert_eq!(next_after(&items, &paths(&["d"])), Some(PathBuf::from("c")));
        assert_eq!(next_after(&items, &paths(&["c", "d"])), Some(PathBuf::from("b")));
        // everything goes
        assert_eq!(next_after(&items, &items), None);
        // nothing of the list goes
        assert_eq!(next_after(&items, &paths(&["x"])), None);
    }

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
