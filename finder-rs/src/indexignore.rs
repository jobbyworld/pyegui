//! What the indexation of a folder leaves out: the hidden entries and the folders named in a list
//! (`.git`, `node_modules`, `Local Settings`...), kept between two runs.
//!
//! An entry of the list is a name (matched on the name of a folder, case ignored, a `*` at the start
//! and/or the end stands for any text: `*.app`, `cache*`) or, if it holds a separator, the start of a path
//! (`/Volumes/Backup`: everything under it).

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct IgnoreRules {
    /// the entries whose name starts with a dot (and on Windows the hidden and system ones are not read: a dot is enough)
    pub hidden: bool,
    pub names: Vec<String>,
}

impl Default for IgnoreRules {
    fn default() -> Self {
        let names = [
            // the settings and the caches of the systems and of the tools
            "Local Settings", "AppData", "Application Data", "Library", "$RECYCLE.BIN", "System Volume Information",
            "node_modules", "__pycache__", "target", "venv", "site-packages", "*.app", "*.photoslibrary",
        ];
        IgnoreRules { hidden: true, names: names.iter().map(|n| n.to_string()).collect() }
    }
}

impl IgnoreRules {
    /// Whether the indexation leaves out this entry (a file or a folder) found while it walks
    pub fn ignores(&self, path: &Path, name: &str, is_dir: bool) -> bool {
        if self.hidden && name.starts_with('.') {
            return true;
        }
        if !is_dir {
            return false;
        }
        let name_l = name.to_lowercase();
        let path_s = path.to_string_lossy();
        self.names.iter().any(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return false;
            }
            if entry.contains('/') || entry.contains('\\') {
                let prefix = entry.trim_end_matches(['/', '\\']);
                return path_s.strip_prefix(prefix).is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '\\']));
            }
            glob(&entry.to_lowercase(), &name_l)
        })
    }

    /// Adds an entry (trimmed, once)
    pub fn add(&mut self, entry: &str) {
        let entry = entry.trim();
        if !entry.is_empty() && !self.names.iter().any(|n| n.eq_ignore_ascii_case(entry)) {
            self.names.push(entry.to_string());
        }
    }
}

/// `*` at the start and/or the end of the pattern stands for any text
fn glob(pattern: &str, name: &str) -> bool {
    match (pattern.strip_prefix('*'), pattern.strip_suffix('*')) {
        (Some(_), Some(_)) if pattern.len() > 1 => name.contains(pattern.trim_matches('*')),
        (Some(rest), _) => name.ends_with(rest),
        (_, Some(rest)) => name.starts_with(rest),
        _ => name == pattern,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn rules(names: &[&str]) -> IgnoreRules {
        IgnoreRules { hidden: false, names: names.iter().map(|n| n.to_string()).collect() }
    }

    #[test]
    fn hidden_entries_follow_the_option() {
        let on = IgnoreRules { hidden: true, names: vec![] };
        assert!(on.ignores(&PathBuf::from("/a/.git"), ".git", true));
        assert!(on.ignores(&PathBuf::from("/a/.env"), ".env", false));
        assert!(!rules(&[]).ignores(&PathBuf::from("/a/.git"), ".git", true));
    }

    #[test]
    fn names_ignore_the_case_and_only_folders() {
        let r = rules(&["Local Settings"]);
        assert!(r.ignores(&PathBuf::from("/u/local settings"), "local settings", true));
        assert!(!r.ignores(&PathBuf::from("/u/Local Settings"), "Local Settings", false));
        assert!(!r.ignores(&PathBuf::from("/u/Local"), "Local", true));
    }

    #[test]
    fn stars_and_paths() {
        let r = rules(&["*.app", "cache*", "*tmp*", "/Volumes/Backup"]);
        assert!(r.ignores(&PathBuf::from("/Applications/X.app"), "X.app", true));
        assert!(r.ignores(&PathBuf::from("/a/cache-1"), "cache-1", true));
        assert!(r.ignores(&PathBuf::from("/a/my_tmp_dir"), "my_tmp_dir", true));
        assert!(r.ignores(&PathBuf::from("/Volumes/Backup"), "Backup", true));
        assert!(r.ignores(&PathBuf::from("/Volumes/Backup/x"), "x", true));
        assert!(!r.ignores(&PathBuf::from("/Volumes/Backups"), "Backups", true));
    }
}
