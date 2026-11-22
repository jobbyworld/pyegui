//! The actions: one registry read by the Actions pane, the context menu and the keyboard, so an
//! action is declared once (the same idea as `ACTIONS` of examples/finder.py).

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use crate::app::{FinderApp, ViewMode};

pub struct Action {
    pub category: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub shortcut: Option<KeyboardShortcut>,
    /// the keyboard also has this one, shown nowhere
    pub enabled: fn(&FinderApp) -> bool,
    pub run: fn(&mut FinderApp, &egui::Context),
    /// shown in the context menu of an item
    pub in_menu: bool,
}

const fn cmd(key: Key) -> Option<KeyboardShortcut> {
    Some(KeyboardShortcut::new(Modifiers::COMMAND, key))
}

fn always(_: &FinderApp) -> bool {
    true
}
fn has_selection(app: &FinderApp) -> bool {
    !app.selected.is_empty()
}
fn one_selected(app: &FinderApp) -> bool {
    app.selected.len() == 1
}
fn can_paste(app: &FinderApp) -> bool {
    app.clipboard.as_ref().is_some_and(|(paths, _)| !paths.is_empty())
}

pub static ACTIONS: &[Action] = &[
    Action { category: "Fichier", label: "Ouvrir", icon: "↗", shortcut: cmd(Key::ArrowDown), enabled: has_selection, in_menu: true, run: |app, _| {
        for path in app.selected_paths() { app.open(&path); }
    }},
    Action { category: "Fichier", label: "Nouvelle fenêtre", icon: "◫", shortcut: cmd(Key::N), enabled: always, in_menu: false, run: |app, _| app.new_window = Some(app.folder().to_path_buf()) },
    Action { category: "Fichier", label: "Ouvrir dans une nouvelle fenêtre", icon: "◫", shortcut: None, enabled: |a| a.selected.len() == 1 && a.selected.iter().all(|p| p.is_dir()), in_menu: true, run: |app, _| {
        if let Some(p) = app.selected_paths().first() { app.new_window = Some(p.clone()); }
    }},
    Action { category: "Fichier", label: "Nouveau dossier", icon: "＋", shortcut: Some(KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::N)), enabled: always, in_menu: false, run: |app, _| app.new_folder() },
    Action { category: "Fichier", label: "Renommer", icon: "✎", shortcut: Some(KeyboardShortcut::new(Modifiers::NONE, Key::F2)), enabled: one_selected, in_menu: true, run: |app, _| {
        if let Some(p) = app.selected_paths().first() { app.start_rename(p.clone()); }
    }},
    Action { category: "Fichier", label: "Dupliquer", icon: "⧉", shortcut: cmd(Key::D), enabled: has_selection, in_menu: true, run: |app, _| {
        let folder = app.folder().to_path_buf();
        app.transfer.start(app.selected_paths(), folder, false);
    }},
    Action { category: "Fichier", label: "Mettre à la corbeille", icon: "🗑", shortcut: Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::Backspace)), enabled: has_selection, in_menu: true, run: |app, _| app.ask_delete() },
    Action { category: "Édition", label: "Copier", icon: "⎘", shortcut: cmd(Key::C), enabled: has_selection, in_menu: true, run: |app, _| {
        app.clipboard = Some((app.selected_paths(), false));
    }},
    Action { category: "Édition", label: "Couper", icon: "✂", shortcut: cmd(Key::X), enabled: has_selection, in_menu: true, run: |app, _| {
        app.clipboard = Some((app.selected_paths(), true));
    }},
    Action { category: "Édition", label: "Coller", icon: "📋", shortcut: cmd(Key::V), enabled: can_paste, in_menu: true, run: |app, _| app.paste() },
    Action { category: "Édition", label: "Tout sélectionner", icon: "▣", shortcut: cmd(Key::A), enabled: always, in_menu: false, run: |app, _| app.select_all() },
    Action { category: "Outils", label: "Exporter en PDF", icon: "⇩", shortcut: None, enabled: |a| a.markdown_target().is_some(), in_menu: true, run: |app, _| {
        if let Some(source) = app.markdown_target() { export_markdown_pdf(source); }
    }},
    Action { category: "Chemin", label: "Copier le chemin", icon: "⌘", shortcut: Some(KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::C)), enabled: has_selection, in_menu: true, run: |app, ctx| {
        let text = app.selected_paths().iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join("\n");
        ctx.copy_text(text);
    }},
    Action { category: "Chemin", label: "Afficher dans le gestionnaire", icon: "⌕", shortcut: None, enabled: has_selection, in_menu: true, run: |app, _| {
        if let Some(p) = app.selected_paths().first() { reveal(p); }
    }},
    Action { category: "Navigation", label: "Précédent", icon: "◀", shortcut: cmd(Key::OpenBracket), enabled: |a| a.history.can_back(), in_menu: false, run: |app, _| app.back() },
    Action { category: "Navigation", label: "Suivant", icon: "▶", shortcut: cmd(Key::CloseBracket), enabled: |a| a.history.can_forward(), in_menu: false, run: |app, _| app.forward() },
    Action { category: "Navigation", label: "Dossier parent", icon: "▲", shortcut: cmd(Key::ArrowUp), enabled: |a| a.folder().parent().is_some(), in_menu: false, run: |app, _| app.parent() },
    Action { category: "Affichage", label: "Liste", icon: "☰", shortcut: cmd(Key::Num1), enabled: always, in_menu: false, run: |app, _| app.view = ViewMode::List },
    Action { category: "Affichage", label: "Icônes", icon: "▦", shortcut: cmd(Key::Num2), enabled: always, in_menu: false, run: |app, _| app.view = ViewMode::Icons },
    Action { category: "Affichage", label: "Arbre", icon: "🗂", shortcut: cmd(Key::Num3), enabled: always, in_menu: false, run: |app, _| app.view = ViewMode::Tree },
    Action { category: "Affichage", label: "Fichiers cachés", icon: "◌", shortcut: Some(KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Period)), enabled: always, in_menu: false, run: |app, _| app.show_hidden = !app.show_hidden },
    Action { category: "Affichage", label: "Barre latérale", icon: "◧", shortcut: cmd(Key::B), enabled: always, in_menu: false, run: |app, _| app.toggle_sidebar() },
    Action { category: "Affichage", label: "Terminal", icon: ">_", shortcut: cmd(Key::T), enabled: always, in_menu: false, run: |app, _| app.toggle_terminal() },
    Action { category: "Affichage", label: "Volets", icon: "◨", shortcut: cmd(Key::J), enabled: always, in_menu: false, run: |app, _| app.toggle_dock() },
];

pub fn enabled_flags(app: &FinderApp) -> Vec<bool> {
    ACTIONS.iter().map(|a| (a.enabled)(app)).collect()
}

/// The shortcuts pressed this frame run their action. Not while a text field has the focus
pub fn run_shortcuts(app: &mut FinderApp, ctx: &egui::Context) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    for action in ACTIONS {
        if let Some(shortcut) = &action.shortcut {
            if ctx.input_mut(|i| i.consume_shortcut(shortcut)) && (action.enabled)(app) {
                (action.run)(app, ctx);
            }
        }
    }
}

/// The items of the context menu of an item; returns the index of the action chosen
pub fn menu(ui: &mut egui::Ui, enabled: &[bool]) -> Option<usize> {
    let mut chosen = None;
    for (index, action) in ACTIONS.iter().enumerate().filter(|(_, a)| a.in_menu) {
        let mut button = egui::Button::new(format!("{}  {}", action.icon, action.label));
        if let Some(shortcut) = &action.shortcut {
            button = button.shortcut_text(ui.ctx().format_shortcut(shortcut));
        }
        if ui.add_enabled(enabled[index], button).clicked() {
            chosen = Some(index);
            ui.close();
        }
    }
    chosen
}

pub fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
}

/// Puts a file in the trash of the system (macOS: through the Finder, so that Put Back works).
/// Elsewhere the file is removed for good: the caller asks first.
pub fn trash(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"Finder\" to delete POSIX file \"{}\"",
            path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"")
        );
        let out = std::process::Command::new("osascript").arg("-e").arg(script).output().map_err(|e| e.to_string())?;
        if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if meta.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }.map_err(|e| e.to_string())
    }
}

/// "name.pdf", or "name copy.pdf", "name copy 2.pdf"... when it exists: a file is never overwritten
fn unique_pdf(source: &Path) -> PathBuf {
    let base = source.with_extension("pdf");
    if !base.exists() {
        return base;
    }
    let stem = base.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    (1..)
        .map(|n| base.with_file_name(if n == 1 { format!("{stem} copy.pdf") } else { format!("{stem} copy {n}.pdf") }))
        .find(|p| !p.exists())
        .unwrap()
}

/// Writes the PDF of a markdown file next to it, in a thread (the window never waits). The watcher of the
/// folder shows the new file.
fn export_markdown_pdf(source: PathBuf) {
    use pyegui_core::markdown_pdf::{build, Bar, Cover, Geo, Options, Theme};
    let out = unique_pdf(&source);
    std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let text = std::fs::read_to_string(&source).map_err(|e| e.to_string())?;
            let mm = 72.0 / 25.4;
            let options = Options {
                theme: Theme::parse("light").unwrap(),
                geo: Geo { w: 210.0 * mm, h: 297.0 * mm, margin: 20.0 * mm },
                title: None,
                author: None,
                cover: Some(Cover::default()),
                toc: Some(("Table des matières".to_string(), 3)),
                bookmarks: true,
                header: None,
                footer: Some(Bar { center: "{page} / {pages}".into(), ..Default::default() }),
                base_dir: source.parent().map(Path::to_path_buf),
            };
            let (bytes, _, _) = build(&text, &options)?;
            std::fs::write(&out, bytes).map_err(|e| e.to_string())
        };
        if let Err(error) = run() {
            eprintln!("markdown_to_pdf: {}: {error}", source.display());
        }
    });
}

/// "untitled folder", "untitled folder 2"... the first that does not exist
pub fn unique_folder(parent: &Path) -> PathBuf {
    let base = parent.join("dossier sans titre");
    if !base.exists() {
        return base;
    }
    (2..).map(|n| parent.join(format!("dossier sans titre {n}"))).find(|p| !p.exists()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_folder_never_takes_an_existing_name() {
        let dir = std::env::temp_dir().join(format!("finder-rs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = unique_folder(&dir);
        std::fs::create_dir(&first).unwrap();
        let second = unique_folder(&dir);
        assert_ne!(first, second);
        assert!(second.to_string_lossy().ends_with("dossier sans titre 2"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_exported_pdf_never_takes_an_existing_name() {
        let dir = std::env::temp_dir().join(format!("finder-rs-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("a.md");
        assert_eq!(unique_pdf(&md), dir.join("a.pdf"));
        std::fs::write(dir.join("a.pdf"), b"").unwrap();
        assert_eq!(unique_pdf(&md), dir.join("a copy.pdf"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_shortcut_is_used_once() {
        let mut seen = Vec::new();
        for a in ACTIONS {
            if let Some(s) = a.shortcut {
                assert!(!seen.contains(&s), "{} shares its shortcut", a.label);
                seen.push(s);
            }
        }
    }
}
