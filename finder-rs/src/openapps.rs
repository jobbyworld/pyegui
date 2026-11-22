//! "Ouvrir dans Zed / Typora / Claude": detection of the installed application (macOS, Windows,
//! Linux) and the command that opens the targets (the same as `zed_command`, `typora_command`,
//! `open_in_claude` of examples/finder.py).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// A macOS application in /Applications or ~/Applications
pub fn app_installed(name: &str) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let mut bases = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        bases.push(PathBuf::from(home).join("Applications"));
    }
    bases.iter().any(|b| b.join(format!("{name}.app")).exists())
}

/// The first of these programs found in the PATH
fn which(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) { &["", ".exe", ".cmd", ".bat"] } else { &[""] };
    for dir in std::env::split_paths(&path) {
        for name in names {
            for ext in exts {
                let candidate = dir.join(format!("{name}{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// A command and its fixed arguments
pub type Launcher = Vec<String>;

fn strings(items: &[&str]) -> Launcher {
    items.iter().map(|s| s.to_string()).collect()
}

fn zed_command_uncached() -> Option<Launcher> {
    if app_installed("Zed") {
        return Some(strings(&["open", "-a", "Zed"])); // never blocks, the CLI is often not installed
    }
    which(&["zed", "zeditor"]).map(|p| vec![p.to_string_lossy().into_owned()])
}

fn typora_command_uncached() -> Option<Launcher> {
    if app_installed("Typora") {
        return Some(strings(&["open", "-a", "Typora"]));
    }
    let mut candidates: Vec<PathBuf> = which(&["typora"]).into_iter().collect();
    if cfg!(windows) {
        for base in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(dir) = std::env::var_os(base) {
                candidates.push(Path::new(&dir).join("Programs/Typora/Typora.exe"));
                candidates.push(Path::new(&dir).join("Typora/Typora.exe"));
            }
        }
    }
    candidates.into_iter().find(|p| p.exists()).map(|p| vec![p.to_string_lossy().into_owned()])
}

/// The command that opens files in Zed, None when it is not installed (looked up once)
pub fn zed_command() -> Option<&'static Launcher> {
    static CACHE: OnceLock<Option<Launcher>> = OnceLock::new();
    CACHE.get_or_init(zed_command_uncached).as_ref()
}

/// The command that opens files in Typora, None when it is not installed (looked up once)
pub fn typora_command() -> Option<&'static Launcher> {
    static CACHE: OnceLock<Option<Launcher>> = OnceLock::new();
    CACHE.get_or_init(typora_command_uncached).as_ref()
}

/// Claude Desktop: on macOS when installed, on Windows always (as `finder.py`)
pub fn claude_available() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| cfg!(windows) || app_installed("Claude"))
}

pub fn launch(command: &Launcher, targets: &[PathBuf]) {
    if let Some((program, args)) = command.split_first() {
        let _ = Command::new(program).args(args).args(targets).spawn();
    }
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|e| matches!(e.to_string_lossy().to_lowercase().as_str(), "md" | "markdown"))
}

/// `claude://code/new?folder=...` or `claude://cowork/new?folder=...`
pub fn claude_url(mode: &str, folder: &Path) -> String {
    let mut out = String::new();
    for b in folder.to_string_lossy().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    format!("claude://{mode}/new?folder={out}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_is_percent_encoded() {
        assert_eq!(claude_url("code", Path::new("/a b/é")), "claude://code/new?folder=%2Fa%20b%2F%C3%A9");
    }
}
