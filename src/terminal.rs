//! A terminal emulator widget: a shell in a pseudo terminal (`alacritty_terminal`) drawn with egui.
//! The terminal itself is in `pyegui-core`; this file is its Python side.

use std::collections::HashMap;
use std::path::PathBuf;

use pyegui_core::terminal::{self as core, Scheme};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::style::{extract_color, kwarg_color};
use crate::{current_ui, kwarg, UI};

pub(crate) use pyegui_core::terminal::terminal_focused;

fn scheme_from_dict(dict: &Bound<'_, PyDict>) -> PyResult<Scheme> {
    let mut scheme = Scheme::default();
    for (key, value) in dict.iter() {
        let key: String = key.extract()?;
        let color = |value: &Bound<'_, PyAny>| -> PyResult<Option<eframe::egui::Color32>> {
            if value.is_none() { Ok(None) } else { extract_color(value).map(Some) }
        };
        match key.as_str() {
            "background" => scheme.background = color(&value)?,
            "foreground" => scheme.foreground = color(&value)?,
            "cursor" => scheme.cursor = color(&value)?,
            "selection" => scheme.selection = color(&value)?,
            "palette" => {
                let colors: Vec<Bound<'_, PyAny>> = value.extract()?;
                if colors.len() > 16 {
                    return Err(PyValueError::new_err("palette has 16 colors at most"));
                }
                for (index, value) in colors.iter().enumerate() {
                    scheme.palette[index] = color(value)?;
                }
            }
            _ => {
                return Err(PyValueError::new_err(format!(
                    "unknown terminal color '{key}': background, foreground, cursor, selection or palette"
                )))
            }
        }
    }
    Ok(scheme)
}

/// The state of a terminal: a shell running in a pseudo terminal, with its screen, its history and
/// its selection. Create it once, outside the update function, and draw it with terminal().
///
/// The shell starts the first time the terminal is drawn, with the size it is given. It is a real
/// terminal: colors, cursor movements and full screen programs (vim, htop) work. The text is
/// selected with the mouse and copied with Cmd+C (Ctrl+C on Windows and Linux when there is a
/// selection), pasted with Cmd+V or Ctrl+V, and the wheel scrolls the history.
///
/// Args:
///     cwd (str): the folder the shell starts in, the current one by default
///
///     shell (str): the program to run, the shell of the user by default
///
///     args (list[str]): the arguments of the program
///
///     env (dict[str, str]): environment variables added to those of the app
///
///     scrollback (int): number of lines kept in the history, 10000 by default
///
///     font_size (float): size of the monospace font, 13 by default
///
///     light (dict): the colors with the light theme, see set_colors()
///
///     dark (dict): the colors with the dark theme, see set_colors()
///
///     font (str): path of a monospace font file (.ttf, .otf) to draw with instead of the one of
///         egui, e.g. a Nerd Font for the icons of a prompt; take its "Mono" variant, whose icons
///         are one cell wide. The characters it does not have come from the font of egui
///
/// Usage::
///
///     term = Terminal(cwd="/tmp")
///     python = Terminal(shell="python3", args=["-q"])
///     nerd = Terminal(font=str(Path.home() / "Library/Fonts/HackNerdFontMono-Regular.ttf"))
///
///     # colors that follow the theme of the app
///     term = Terminal(
///         light=dict(background="#fafafa", foreground="#383a42"),
///         dark=dict(background="#282c34", foreground="#abb2bf", cursor="#528bff"),
///     )
///
///     def update_func(ctx):
///         terminal(term)
///
///     # in a dock
///     dock = Dock(DockContainer("vertical", [
///         DockPane("Editor", editor),
///         DockPane("Terminal", lambda: terminal(term)),
///     ]))
#[pyclass]
pub(crate) struct Terminal {
    inner: core::Terminal,
}

#[pymethods]
impl Terminal {
    #[new]
    #[pyo3(signature = (cwd=None, shell=None, args=None, env=None, scrollback=10000, font_size=13.0, font=None, light=None, dark=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        cwd: Option<PathBuf>,
        shell: Option<String>,
        args: Option<Vec<String>>,
        env: Option<HashMap<String, String>>,
        scrollback: usize,
        font_size: f32,
        font: Option<String>,
        light: Option<&Bound<'_, PyDict>>,
        dark: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        Ok(Terminal {
            inner: core::Terminal::new(
                cwd,
                shell,
                args.unwrap_or_default(),
                env.unwrap_or_default(),
                scrollback,
                font_size,
                font,
                light.map(scheme_from_dict).transpose()?.unwrap_or_default(),
                dark.map(scheme_from_dict).transpose()?.unwrap_or_default(),
            ),
        })
    }

    /// Size of the monospace font
    #[getter]
    fn get_font_size(&self) -> f32 {
        self.inner.font_size
    }

    #[setter]
    fn set_font_size(&mut self, size: f32) {
        self.inner.font_size = size;
    }

    /// Path of the font file drawn with, or None for the monospace font of egui
    #[getter]
    fn get_font(&self) -> Option<String> {
        self.inner.font.clone()
    }

    #[setter]
    fn set_font(&mut self, font: Option<String>) {
        self.inner.font = font;
    }

    /// Sets the colors of the terminal with the light theme, with the dark one, or both. The
    /// terminal takes those of the theme shown, at once: a change of theme changes them too.
    /// A color not given is the one of the theme of egui; the background of terminal(...) and
    /// its foreground win over these.
    ///
    /// Keyword args:
    ///     light (dict): the colors with the light theme; {} goes back to those of egui
    ///
    ///     dark (dict): the colors with the dark theme; {} goes back to those of egui
    ///
    /// Each dict takes (a color is (r, g, b), (r, g, b, a) or "#rrggbb[aa]"):
    ///
    ///     background: behind the text, the sheet of the pane by default (transparent)
    ///
    ///     foreground: the text
    ///
    ///     cursor: the cursor, the foreground by default
    ///
    ///     selection: behind the selected text
    ///
    ///     palette (list): the 16 colors of the shell, black, red, green, yellow, blue, magenta,
    ///         cyan, white then their bright versions; a shorter list or None in it keeps the
    ///         others
    ///
    /// Example::
    ///
    ///     term.set_colors(
    ///         light=dict(background="#fdf6e3", foreground="#657b83"),
    ///         dark=dict(background="#002b36", foreground="#839496", palette=[
    ///             "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
    ///             "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
    ///         ]),
    ///     )
    #[pyo3(signature = (light=None, dark=None))]
    fn set_colors(&mut self, light: Option<&Bound<'_, PyDict>>, dark: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        if let Some(light) = light {
            self.inner.light = scheme_from_dict(light)?;
        }
        if let Some(dark) = dark {
            self.inner.dark = scheme_from_dict(dark)?;
        }
        Ok(())
    }

    /// Writes a text to the shell, as if it was typed. Before the shell has started, the text is
    /// kept and written when it starts.
    fn send(&mut self, text: &str) {
        self.inner.send(text);
    }

    /// Runs a command: writes it to the shell followed by the Enter key
    fn run(&mut self, command: &str) {
        self.inner.run(command);
    }

    /// Gives the keyboard focus to the terminal the next time it is drawn
    fn focus(&mut self) {
        self.inner.focus();
    }

    /// Stops the shell and starts a new one the next time the terminal is drawn
    ///
    /// Keyword args:
    ///     cwd (str): the folder the new shell starts in, the same as before by default
    #[pyo3(signature = (cwd=None))]
    fn restart(&mut self, cwd: Option<PathBuf>) {
        self.inner.restart(cwd);
    }

    /// The text of the screen and of the history, without the colors
    fn text(&self) -> String {
        self.inner.text()
    }

    /// The title given by the shell, usually the running command or the current folder; an empty
    /// string when it gave none
    #[getter]
    fn title(&self) -> String {
        self.inner.title.clone()
    }

    /// The selected text, or None
    #[getter]
    fn selection(&self) -> Option<String> {
        self.inner.selection()
    }

    /// True while the shell runs. False before the terminal is drawn for the first time and once
    /// the shell has exited.
    #[getter]
    fn running(&self) -> bool {
        self.inner.running()
    }

    /// True once the shell has exited. Enter in the terminal, or restart(), starts a new one.
    #[getter]
    fn exited(&self) -> bool {
        self.inner.exited
    }

    /// The exit code of the shell, None while it runs or when it was killed by a signal
    #[getter]
    fn exit_code(&self) -> Option<i32> {
        self.inner.exit_code
    }

    /// True when the terminal had the keyboard focus the last time it was drawn
    #[getter]
    fn has_focus(&self) -> bool {
        self.inner.focused
    }
}

/// Shows a terminal. It takes all the space available, or the height given, and its number of
/// columns and rows follows its size. Click it to give it the keyboard: while it has the focus the
/// keys go to the shell and key_pressed() returns False (unless when_typing=True).
///
/// Without a background the terminal is transparent: it shows the sheet of the pane or of the
/// panel it is in. The 16 colors of the shell follow the light or dark theme, and so do the
/// colors given by Terminal(light=..., dark=...) or term.set_colors().
///
/// Args:
///     term (Terminal): the state of the terminal
///
/// Keyword args:
///     background (tuple | str): the color behind the text, before the one of Terminal(light=,
///         dark=)
///
///     foreground (tuple | str): the color of the text, before the one of Terminal(light=,
///         dark=), the one of the theme by default
///
///     height (float): the height, the available one by default
///
///     padding (float): the space around the text, 6 by default
///
///     opacity (float): 0 to 1, makes the terminal translucent: the background (the one given, else
///         the one of the theme) and the colored cells are drawn with this opacity, so the sheet
///         or the backdrop of the window shows through; the text stays opaque. Opaque by default
///         when a background is given, transparent when not
///
/// Example::
///
///     term = Terminal()
///     dock = Dock(DockContainer("vertical", [
///         DockPane("Notes", lambda: label("...")),
///         DockPane("Terminal", lambda: terminal(term)),
///     ]))
///
///     def update_func(ctx):
///         if button_clicked("List the files"):
///             term.run("ls -l")
///         dock_area(dock)
///
///     # a dark terminal whatever the theme
///     terminal(term, background="#1e1e1e", foreground="#d4d4d4")
///
///     # the same, translucent
///     terminal(term, background="#1e1e1e", opacity=0.6)
#[pyfunction]
#[pyo3(signature = (term, **kwargs))]
unsafe fn terminal(term: &mut Terminal, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let mut options = core::TerminalOptions::new();
    options.background = kwarg_color(kwargs, "background")?;
    options.foreground = kwarg_color(kwargs, "foreground")?;
    options.opacity = kwarg::<f32>(kwargs, "opacity")?;
    options.height = kwarg::<f32>(kwargs, "height")?;
    if let Some(padding) = kwarg::<f32>(kwargs, "padding")? {
        options.padding = padding;
    }
    // the font file of the terminal once egui has it, the monospace font of egui until then
    options.font_family = match &term.inner.font {
        Some(path) => crate::file_font_family(ui.ctx(), path)?,
        None => None,
    };
    core::show(ui, &mut term.inner, options).map_err(PyRuntimeError::new_err)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Terminal>()?;
    m.add_function(wrap_pyfunction!(terminal, m)?)?;
    Ok(())
}
