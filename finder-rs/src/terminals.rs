//! The terminals, two panes of the dock: a shell that follows the folder of the Finder, and a Python prompt
//! (the newest Python found: since 3.13 its prompt has colors, since 3.14 it colors the code).

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, FontFamily};

use pyegui_core::terminal::{self, Scheme, Terminal, TerminalOptions};

pub struct Terminals {
    pub shell: Terminal,
    pub python: Terminal,
    pub follow: bool,
    shell_folder: PathBuf,
    /// the family of the Nerd Font installed, if any
    pub font_family: Option<FontFamily>,
}

fn hex(text: &str) -> Option<Color32> {
    let t = text.trim_start_matches('#');
    let v = u32::from_str_radix(t, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn scheme(background: &str, foreground: &str, cursor: &str, selection: &str, palette: [&str; 16]) -> Scheme {
    let mut s = Scheme { background: hex(background), foreground: hex(foreground), cursor: hex(cursor), selection: hex(selection), ..Default::default() };
    for (i, c) in palette.iter().enumerate() {
        s.palette[i] = hex(c);
    }
    s
}

/// The colors of a terminal given by a style sheet: {"background", "foreground", "cursor", "selection", "palette"}.
/// What the sheet does not give is the color of the theme of egui, like `Terminal.set_colors` of finder.py.
fn scheme_of(value: &serde_json::Value) -> Scheme {
    let color = |key: &str| value.get(key).and_then(|v| pyegui_core::sheet::color(v).ok());
    let mut s = Scheme { background: color("background"), foreground: color("foreground"), cursor: color("cursor"), selection: color("selection"), ..Default::default() };
    if let Some(palette) = value.get("palette").and_then(|p| p.as_array()) {
        for (i, c) in palette.iter().take(16).enumerate() {
            s.palette[i] = pyegui_core::sheet::color(c).ok();
        }
    }
    s
}

/// One Light
fn light() -> Scheme {
    scheme("#fafafa", "#383a42", "#526fff", "#e5e5e6", [
        "#383a42", "#e45649", "#50a14f", "#c18401", "#4078f2", "#a626a4", "#0184bc", "#a0a1a7",
        "#696c77", "#e45649", "#50a14f", "#c18401", "#4078f2", "#a626a4", "#0184bc", "#fafafa",
    ])
}

/// One Dark
fn dark() -> Scheme {
    scheme("#282c34", "#abb2bf", "#528bff", "#3e4451", [
        "#3f4451", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#d7dae0",
        "#4f5666", "#be5046", "#98c379", "#d19a66", "#61afef", "#c678dd", "#56b6c2", "#e6e6e6",
    ])
}

fn newest_python() -> Option<String> {
    let extra = ["/opt/homebrew/bin", "/usr/local/bin", "/Library/Frameworks/Python.framework/Versions/Current/bin", "/usr/bin"];
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).chain(extra.iter().map(PathBuf::from)).collect();
    let exe = if cfg!(windows) { ".exe" } else { "" };
    for minor in (8..=20).rev() {
        for dir in &dirs {
            let candidate = dir.join(format!("python3.{minor}{exe}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    // the plain name, found by the system
    Some(if cfg!(windows) { "python".to_owned() } else { "python3".to_owned() })
}

/// An installed Nerd Font, its Mono variant (icons one cell wide), for the icons of the prompt
pub fn nerd_font() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut folders = vec![PathBuf::from("/Library/Fonts")];
    if let Some(home) = home {
        folders.push(home.join("Library/Fonts"));
        folders.push(home.join(".local/share/fonts"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        folders.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
    }
    for folder in folders {
        let Ok(read) = std::fs::read_dir(folder) else { continue };
        let mut found: Vec<PathBuf> = read
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                name.contains("NerdFontMono-Regular") && (name.ends_with(".ttf") || name.ends_with(".otf"))
            })
            .collect();
        found.sort();
        if let Some(first) = found.into_iter().next() {
            return Some(first);
        }
    }
    None
}

pub fn cd_command(folder: &Path) -> String {
    if cfg!(windows) {
        // PowerShell, the shell of the terminal on Windows (`cd /d` is the syntax of cmd.exe); the quotes
        // of the path are doubled and nothing in it is read as a wildcard
        format!("Set-Location -LiteralPath '{}'", folder.to_string_lossy().replace('\'', "''"))
    } else {
        // the leading space keeps it out of the history
        format!(" cd '{}'", folder.to_string_lossy().replace('\'', "'\\''"))
    }
}

impl Terminals {
    pub fn new(start: &Path, font_path: Option<&Path>) -> Self {
        let font = font_path.map(|p| p.to_string_lossy().into_owned());
        let make = |shell: Option<String>, args: Vec<String>| {
            Terminal::new(Some(start.to_path_buf()), shell, args, Default::default(), 10_000, 13.0, font.clone(), light(), dark())
        };
        Terminals {
            shell: make(None, vec![]),
            python: make(newest_python(), vec!["-q".to_owned()]),
            follow: true,
            shell_folder: start.to_path_buf(),
            // the family "nerd" is always defined (the font chosen, the Nerd Font installed, else the monospace one)
            font_family: Some(FontFamily::Name("nerd".into())),
        }
    }

    /// The colors of the terminals: the ones of the style sheet (its "app" section, key "terminal"), else One Light
    /// and One Dark
    pub fn apply_sheet(&mut self) {
        let light = crate::sheets::terminal(false).map(|v| scheme_of(&v)).unwrap_or_else(light);
        let dark = crate::sheets::terminal(true).map(|v| scheme_of(&v)).unwrap_or_else(dark);
        for term in [&mut self.shell, &mut self.python] {
            term.light = light.clone();
            term.dark = dark.clone();
        }
    }

    /// The shell follows the folder of the Finder while the option is on
    pub fn follow(&mut self, folder: &Path) {
        if self.follow && folder != self.shell_folder {
            self.shell_folder = folder.to_path_buf();
            self.shell.run(&cd_command(folder));
        }
    }

    fn options(&self, opacity: f32) -> TerminalOptions {
        let mut options = TerminalOptions::new();
        options.opacity = Some(opacity);
        options.font_family = self.font_family.clone();
        options
    }

    /// The pane of the shell: the terminal alone (its commands are the actions of the "Terminal" category)
    pub fn shell_ui(&mut self, ui: &mut egui::Ui, opacity: f32) {
        let options = self.options(opacity);
        if let Err(error) = terminal::show(ui, &mut self.shell, options) {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }

    /// The pane of the Python prompt
    pub fn python_ui(&mut self, ui: &mut egui::Ui, opacity: f32) {
        let options = self.options(opacity);
        if let Err(error) = terminal::show(ui, &mut self.python, options) {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }

    /// `cd` to the folder shown, once
    pub fn cd_here(&mut self, folder: &Path) {
        self.shell.run(&cd_command(folder));
        self.shell_folder = folder.to_path_buf();
    }

    /// A new shell in the folder shown
    pub fn restart_shell(&mut self, folder: &Path) {
        self.shell.restart(Some(folder.to_path_buf()));
        self.shell_folder = folder.to_path_buf();
    }
}
