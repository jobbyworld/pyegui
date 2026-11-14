//! A terminal emulator widget: a shell in a pseudo terminal (`alacritty_terminal`) drawn with egui.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, CursorShape, NamedColor};
use eframe::egui::{self, Color32, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::style::kwarg_color;
use crate::{current_ui, kwarg, UI};

/// The egui id of the terminal that had the keyboard focus when it was last drawn (0 for none).
/// `key_pressed` reads it, so the shortcuts of the app do not fire while the user types in a shell.
static FOCUSED_TERMINAL: AtomicU64 = AtomicU64::new(0);

/// True when a terminal has the keyboard focus
pub(crate) fn terminal_focused(ctx: &egui::Context) -> bool {
    let id = FOCUSED_TERMINAL.load(Ordering::Relaxed);
    id != 0 && ctx.memory(|m| m.focused()).map(|focused| focused.value()) == Some(id)
}

/// Receives the events of the terminal from its reading thread and wakes the GUI up
#[derive(Clone)]
struct Listener {
    events: Arc<Mutex<Vec<TermEvent>>>,
    ctx: egui::Context,
}

impl EventListener for Listener {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::Wakeup | TermEvent::MouseCursorDirty | TermEvent::CursorBlinkingChange => {}
            event => {
                if let Ok(mut events) = self.events.lock() {
                    events.push(event);
                }
            }
        }
        self.ctx.request_repaint();
    }
}

struct GridSize {
    columns: usize,
    lines: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// A running shell: the pseudo terminal, its reading thread and the grid of cells
struct Session {
    term: Arc<FairMutex<Term<Listener>>>,
    sender: EventLoopSender,
    events: Arc<Mutex<Vec<TermEvent>>>,
    columns: usize,
    lines: usize,
}

impl Session {
    fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        let bytes = bytes.into();
        if !bytes.is_empty() {
            let _ = self.sender.send(Msg::Input(bytes));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
    }
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
/// Usage::
///
///     term = Terminal(cwd="/tmp")
///     python = Terminal(shell="python3", args=["-q"])
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
    cwd: Option<PathBuf>,
    shell: Option<String>,
    args: Vec<String>,
    env: HashMap<String, String>,
    scrollback: usize,
    /// Size of the monospace font
    #[pyo3(get, set)]
    font_size: f32,
    session: Option<Session>,
    /// what was sent before the shell started
    pending: Vec<u8>,
    title: String,
    exit_code: Option<i32>,
    exited: bool,
    error: Option<String>,
    focus: Option<bool>,
    focused: bool,
    /// the terminal had the focus when the window lost it: it takes it back with the window
    refocus: bool,
    scroll_rest: f32,
    /// the button held in an application that reads the mouse
    mouse_down: bool,
}

impl Terminal {
    fn start(&mut self, ctx: &egui::Context, columns: usize, lines: usize, cell: Vec2) -> std::io::Result<()> {
        let mut env = self.env.clone();
        env.entry("TERM".to_owned()).or_insert_with(|| "xterm-256color".to_owned());
        env.entry("COLORTERM".to_owned()).or_insert_with(|| "truecolor".to_owned());
        // an app started from the Dock or a shortcut has no locale: the shell would not read UTF-8
        if cfg!(unix) && ["LANG", "LC_ALL", "LC_CTYPE"].iter().all(|name| std::env::var_os(name).is_none()) {
            env.entry("LANG".to_owned()).or_insert_with(|| "en_US.UTF-8".to_owned());
        }
        let options = tty::Options {
            shell: self.shell.clone().map(|program| tty::Shell::new(program, self.args.clone())),
            working_directory: self.cwd.clone(),
            drain_on_exit: true,
            env,
            ..Default::default()
        };
        let size = window_size(columns, lines, cell);
        let pty = tty::new(&options, size, 0)?;
        let events = Arc::new(Mutex::new(Vec::new()));
        let listener = Listener { events: events.clone(), ctx: ctx.clone() };
        let config = Config { scrolling_history: self.scrollback, ..Default::default() };
        let term = Arc::new(FairMutex::new(Term::new(config, &GridSize { columns, lines }, listener.clone())));
        let event_loop = EventLoop::new(term.clone(), listener, pty, true, false)?;
        let sender = event_loop.channel();
        event_loop.spawn();
        let session = Session { term, sender, events, columns, lines };
        session.write(std::mem::take(&mut self.pending));
        self.session = Some(session);
        self.exited = false;
        self.exit_code = None;
        Ok(())
    }

    /// Applies what the shell asked for since the last frame
    fn drain_events(&mut self, ctx: &egui::Context, cell: Vec2) {
        let Some(session) = &self.session else { return };
        let events = match session.events.lock() {
            Ok(mut events) => std::mem::take(&mut *events),
            Err(_) => return,
        };
        for event in events {
            match event {
                TermEvent::Title(title) => self.title = title,
                TermEvent::ResetTitle => self.title.clear(),
                TermEvent::PtyWrite(text) => session.write(text.into_bytes()),
                TermEvent::ClipboardStore(_, text) => ctx.copy_text(text),
                TermEvent::ColorRequest(index, format) => {
                    let color = session.term.lock().colors()[index].unwrap_or_default();
                    session.write(format(color).into_bytes());
                }
                TermEvent::TextAreaSizeRequest(format) => {
                    let size = window_size(session.columns, session.lines, cell);
                    session.write(format(size).into_bytes());
                }
                TermEvent::ChildExit(status) => {
                    self.exit_code = status.code();
                    self.exited = true;
                }
                TermEvent::Exit => self.exited = true,
                _ => {}
            }
        }
    }
}

#[pymethods]
impl Terminal {
    #[new]
    #[pyo3(signature = (cwd=None, shell=None, args=None, env=None, scrollback=10000, font_size=13.0))]
    fn new(
        cwd: Option<PathBuf>,
        shell: Option<String>,
        args: Option<Vec<String>>,
        env: Option<HashMap<String, String>>,
        scrollback: usize,
        font_size: f32,
    ) -> Self {
        Terminal {
            cwd,
            shell,
            args: args.unwrap_or_default(),
            env: env.unwrap_or_default(),
            scrollback,
            font_size,
            session: None,
            pending: Vec::new(),
            title: String::new(),
            exit_code: None,
            exited: false,
            error: None,
            focus: None,
            focused: false,
            refocus: false,
            scroll_rest: 0.0,
            mouse_down: false,
        }
    }

    /// Writes a text to the shell, as if it was typed. Before the shell has started, the text is
    /// kept and written when it starts.
    fn send(&mut self, text: &str) {
        match &self.session {
            Some(session) => session.write(text.as_bytes().to_vec()),
            None => self.pending.extend_from_slice(text.as_bytes()),
        }
    }

    /// Runs a command: writes it to the shell followed by the Enter key
    fn run(&mut self, command: &str) {
        self.send(command);
        self.send("\r");
    }

    /// Gives the keyboard focus to the terminal the next time it is drawn
    fn focus(&mut self) {
        self.focus = Some(true);
    }

    /// Stops the shell and starts a new one the next time the terminal is drawn
    ///
    /// Keyword args:
    ///     cwd (str): the folder the new shell starts in, the same as before by default
    #[pyo3(signature = (cwd=None))]
    fn restart(&mut self, cwd: Option<PathBuf>) {
        if cwd.is_some() {
            self.cwd = cwd;
        }
        self.session = None;
        self.pending.clear();
        self.title.clear();
        self.exited = false;
        self.exit_code = None;
        self.error = None;
    }

    /// The text of the screen and of the history, without the colors
    fn text(&self) -> String {
        let Some(session) = &self.session else { return String::new() };
        let term = session.term.lock();
        let start = Point::new(term.topmost_line(), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        term.bounds_to_string(start, end).trim_end().to_owned()
    }

    /// The title given by the shell, usually the running command or the current folder; an empty
    /// string when it gave none
    #[getter]
    fn title(&self) -> String {
        self.title.clone()
    }

    /// The selected text, or None
    #[getter]
    fn selection(&self) -> Option<String> {
        let text = self.session.as_ref()?.term.lock().selection_to_string()?;
        (!text.is_empty()).then_some(text)
    }

    /// True while the shell runs. False before the terminal is drawn for the first time and once
    /// the shell has exited.
    #[getter]
    fn running(&self) -> bool {
        self.session.is_some() && !self.exited
    }

    /// True once the shell has exited. Enter in the terminal, or restart(), starts a new one.
    #[getter]
    fn exited(&self) -> bool {
        self.exited
    }

    /// The exit code of the shell, None while it runs or when it was killed by a signal
    #[getter]
    fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// True when the terminal had the keyboard focus the last time it was drawn
    #[getter]
    fn has_focus(&self) -> bool {
        self.focused
    }
}

fn window_size(columns: usize, lines: usize, cell: Vec2) -> WindowSize {
    WindowSize {
        num_lines: lines as u16,
        num_cols: columns as u16,
        cell_width: cell.x.round().max(1.0) as u16,
        cell_height: cell.y.round().max(1.0) as u16,
    }
}

// colors

const DARK_PALETTE: [u32; 16] = [
    0x000000, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5, 0x666666, 0xf14c4c, 0x23d18b,
    0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xffffff,
];

const LIGHT_PALETTE: [u32; 16] = [
    0x000000, 0xcd3131, 0x00a33f, 0x949800, 0x0451a5, 0xbc05bc, 0x0598bc, 0x555555, 0x666666, 0xcd3131, 0x14a514,
    0xb5a000, 0x0451a5, 0xbc05bc, 0x0598bc, 0xa5a5a5,
];

struct Palette {
    dark: bool,
    foreground: Color32,
    /// what is behind the text, used when the colors are inverted
    backdrop: Color32,
}

impl Palette {
    fn indexed(&self, index: usize) -> Color32 {
        let hex = |value: u32| Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8);
        match index {
            0..=15 => hex(if self.dark { DARK_PALETTE[index] } else { LIGHT_PALETTE[index] }),
            16..=231 => {
                let level = |n: usize| if n == 0 { 0 } else { (55 + 40 * n) as u8 };
                let index = index - 16;
                Color32::from_rgb(level(index / 36), level(index / 6 % 6), level(index % 6))
            }
            _ => {
                let gray = (8 + 10 * (index.min(255) - 232)) as u8;
                Color32::from_gray(gray)
            }
        }
    }

    /// The color of a cell, None for the default background
    fn resolve(&self, color: AnsiColor, term: &Term<Listener>) -> Option<Color32> {
        let index = match color {
            AnsiColor::Spec(rgb) => return Some(Color32::from_rgb(rgb.r, rgb.g, rgb.b)),
            AnsiColor::Indexed(index) => index as usize,
            AnsiColor::Named(name) => name as usize,
        };
        if let Some(rgb) = term.colors()[index] {
            return Some(Color32::from_rgb(rgb.r, rgb.g, rgb.b));
        }
        const DIM_BLACK: usize = NamedColor::DimBlack as usize;
        const DIM_WHITE: usize = NamedColor::DimWhite as usize;
        const BACKGROUND: usize = NamedColor::Background as usize;
        const DIM_FOREGROUND: usize = NamedColor::DimForeground as usize;
        match index {
            0..=255 => Some(self.indexed(index)),
            BACKGROUND => None,
            DIM_BLACK..=DIM_WHITE => Some(self.indexed(index - DIM_BLACK).gamma_multiply(0.66)),
            DIM_FOREGROUND => Some(self.foreground.gamma_multiply(0.66)),
            _ => Some(self.foreground),
        }
    }
}

// keyboard

/// The bytes a key sends to the shell, None when the key is not for it
fn key_bytes(key: Key, modifiers: Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    let ctrl = modifiers.ctrl;
    let alt = modifiers.alt;
    let shift = modifiers.shift;
    // xterm: 1 + shift + 2 alt + 4 ctrl
    let modifier = 1 + shift as u8 + 2 * alt as u8 + 4 * ctrl as u8;
    let csi = |code: &str, end: char| -> Vec<u8> {
        if modifier == 1 {
            if code == "1" {
                format!("\x1b[{end}").into_bytes()
            } else {
                format!("\x1b[{code}{end}").into_bytes()
            }
        } else {
            format!("\x1b[{code};{modifier}{end}").into_bytes()
        }
    };
    let cursor = |end: char| -> Vec<u8> {
        if modifier == 1 && mode.contains(TermMode::APP_CURSOR) {
            format!("\x1bO{end}").into_bytes()
        } else {
            csi("1", end)
        }
    };
    let bytes = match key {
        Key::Enter => b"\r".to_vec(),
        Key::Tab if shift => b"\x1b[Z".to_vec(),
        Key::Tab => b"\t".to_vec(),
        Key::Backspace if alt => b"\x1b\x7f".to_vec(),
        Key::Backspace if ctrl => b"\x08".to_vec(),
        Key::Backspace => b"\x7f".to_vec(),
        Key::Escape => b"\x1b".to_vec(),
        // Option+arrows move by word, like in the terminals of macOS
        Key::ArrowLeft if alt && !ctrl && !shift => b"\x1bb".to_vec(),
        Key::ArrowRight if alt && !ctrl && !shift => b"\x1bf".to_vec(),
        Key::ArrowUp => cursor('A'),
        Key::ArrowDown => cursor('B'),
        Key::ArrowRight => cursor('C'),
        Key::ArrowLeft => cursor('D'),
        Key::Home => cursor('H'),
        Key::End => cursor('F'),
        Key::Insert => csi("2", '~'),
        Key::Delete => csi("3", '~'),
        Key::PageUp => csi("5", '~'),
        Key::PageDown => csi("6", '~'),
        Key::F1 => csi("1", 'P').replace_csi_with_ss3(modifier),
        Key::F2 => csi("1", 'Q').replace_csi_with_ss3(modifier),
        Key::F3 => csi("1", 'R').replace_csi_with_ss3(modifier),
        Key::F4 => csi("1", 'S').replace_csi_with_ss3(modifier),
        Key::F5 => csi("15", '~'),
        Key::F6 => csi("17", '~'),
        Key::F7 => csi("18", '~'),
        Key::F8 => csi("19", '~'),
        Key::F9 => csi("20", '~'),
        Key::F10 => csi("21", '~'),
        Key::F11 => csi("23", '~'),
        Key::F12 => csi("24", '~'),
        Key::Space if ctrl => vec![0],
        Key::OpenBracket if ctrl => vec![0x1b],
        Key::Backslash if ctrl => vec![0x1c],
        Key::CloseBracket if ctrl => vec![0x1d],
        Key::Minus if ctrl => vec![0x1f],
        key if ctrl => {
            let name = key.name().as_bytes();
            if name.len() == 1 && name[0].is_ascii_uppercase() {
                let code = name[0] - b'A' + 1;
                if alt {
                    vec![0x1b, code]
                } else {
                    vec![code]
                }
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some(bytes)
}

trait Ss3 {
    fn replace_csi_with_ss3(self, modifier: u8) -> Self;
}

impl Ss3 for Vec<u8> {
    /// F1 to F4 without modifier are `ESC O P`... and not `ESC [ P`
    fn replace_csi_with_ss3(mut self, modifier: u8) -> Self {
        if modifier == 1 {
            self[1] = b'O';
        }
        self
    }
}

/// The report of a mouse button for an application that reads the mouse
fn mouse_report(mode: TermMode, button: u8, pressed: bool, column: usize, line: usize) -> Vec<u8> {
    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if pressed { 'M' } else { 'm' };
        format!("\x1b[<{};{};{}{}", button, column + 1, line + 1, end).into_bytes()
    } else if column < 223 && line < 223 {
        let button = if pressed { button } else { 3 };
        vec![0x1b, b'[', b'M', 32 + button, 32 + 1 + column as u8, 32 + 1 + line as u8]
    } else {
        Vec::new()
    }
}

// drawing

#[derive(PartialEq, Clone, Copy)]
struct Look {
    fg: Color32,
    bg: Option<Color32>,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

/// Consecutive cells of a row drawn the same way
struct Run {
    row: usize,
    column: usize,
    cells: usize,
    text: String,
    look: Look,
    ascii: bool,
}

/// Shows a terminal. It takes all the space available, or the height given, and its number of
/// columns and rows follows its size. Click it to give it the keyboard: while it has the focus the
/// keys go to the shell and key_pressed() returns False (unless when_typing=True).
///
/// Without a background the terminal is transparent: it shows the sheet of the pane or of the
/// panel it is in. The 16 colors of the shell follow the light or dark theme.
///
/// Args:
///     term (Terminal): the state of the terminal
///
/// Keyword args:
///     background (tuple | str): the color behind the text
///
///     foreground (tuple | str): the color of the text, the one of the theme by default
///
///     height (float): the height, the available one by default
///
///     padding (float): the space around the text, 6 by default
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
#[pyfunction]
#[pyo3(signature = (term, **kwargs))]
unsafe fn terminal(term: &mut Terminal, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let background = kwarg_color(kwargs, "background")?;
    let foreground = kwarg_color(kwargs, "foreground")?;
    let padding = kwarg::<f32>(kwargs, "padding")?.unwrap_or(6.0);
    let font = FontId::monospace(term.font_size.max(4.0));
    let cell = ui.ctx().fonts_mut(|f| egui::vec2(f.glyph_width(&font, 'M'), f.row_height(&font)));
    if cell.x <= 0.0 || cell.y <= 0.0 {
        return Err(PyRuntimeError::new_err("the monospace font has no size"));
    }

    let mut size = ui.available_size();
    if let Some(height) = kwarg::<f32>(kwargs, "height")? {
        size.y = height;
    }
    // in a scroll area the available size has no end
    if !size.x.is_finite() || size.x > 10000.0 {
        size.x = 80.0 * cell.x + 2.0 * padding;
    }
    if !size.y.is_finite() || size.y > 10000.0 {
        size.y = 24.0 * cell.y + 2.0 * padding;
    }
    let (outer, response) = ui.allocate_exact_size(size.max(Vec2::splat(1.0)), Sense::click_and_drag());
    let rect = outer.shrink(padding);
    let columns = ((rect.width() / cell.x).floor() as usize).clamp(2, 1000);
    let lines = ((rect.height() / cell.y).floor() as usize).clamp(1, 1000);
    let ctx = ui.ctx().clone();

    if let Some(color) = background {
        ui.painter().rect_filled(outer, 0.0, color);
    }
    if !ui.is_rect_visible(outer) || rect.width() < cell.x || rect.height() < cell.y {
        return Ok(());
    }

    // the shell
    if term.session.is_none() && term.error.is_none() {
        if let Err(error) = term.start(&ctx, columns, lines, cell) {
            term.error = Some(format!("The shell did not start: {error}"));
        }
    }
    term.drain_events(&ctx, cell);
    let text_color = foreground.unwrap_or_else(|| ui.visuals().text_color());
    if let Some(error) = &term.error {
        ui.painter().text(rect.min, egui::Align2::LEFT_TOP, error, font, ui.visuals().error_fg_color);
        return Ok(());
    }

    // the focus
    let id = response.id;
    if response.clicked() || response.drag_started() || term.focus.take() == Some(true) {
        ui.memory_mut(|m| m.request_focus(id));
    }
    // egui drops the focus when the window is left: the terminal takes it back with the window
    let window_focused = ui.input(|i| i.focused);
    if !window_focused {
        term.refocus |= term.focused;
    } else if std::mem::take(&mut term.refocus) && ui.memory(|m| m.focused().is_none()) {
        ui.memory_mut(|m| m.request_focus(id));
    }
    let focused = ui.memory(|m| m.has_focus(id));
    term.focused = focused;
    if focused {
        FOCUSED_TERMINAL.store(id.value(), Ordering::Relaxed);
        // Tab, the arrows and Escape are for the shell, not for moving the focus
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true },
            )
        });
    }

    // Enter starts a new shell when the last one has exited
    if term.exited && focused && ui.input(|i| i.key_pressed(Key::Enter)) {
        term.restart(None);
        ctx.request_repaint();
        return Ok(());
    }
    let exited = term.exited;
    let Some(session) = term.session.as_mut() else { return Ok(()) };
    let mut grid = session.term.lock();

    if session.columns != columns || session.lines != lines {
        session.columns = columns;
        session.lines = lines;
        grid.resize(GridSize { columns, lines });
        let _ = session.sender.send(Msg::Resize(window_size(columns, lines, cell)));
    }
    let mode = *grid.mode();
    let mut output: Vec<u8> = Vec::new();

    // where the pointer is, in cells
    let cell_at = |pos: Pos2| -> (usize, usize, Side) {
        let x = ((pos.x - rect.left()) / cell.x).max(0.0);
        let y = ((pos.y - rect.top()) / cell.y).max(0.0);
        let side = if x.fract() < 0.5 { Side::Left } else { Side::Right };
        ((x as usize).min(columns - 1), (y as usize).min(lines - 1), side)
    };
    let offset = grid.grid().display_offset() as i32;
    let point_at = |pos: Pos2| -> (Point, Side) {
        let (column, line, side) = cell_at(pos);
        (Point::new(Line(line as i32 - offset), Column(column)), side)
    };

    // the mouse: for the application when it reads it (unless Shift is held), else for the selection
    let shift = ui.input(|i| i.modifiers.shift);
    let app_mouse = mode.intersects(TermMode::MOUSE_MODE) && !shift && !exited;
    let pointer = ui.input(|i| i.pointer.interact_pos());
    if app_mouse {
        let pressed = response.hovered() && ui.input(|i| i.pointer.primary_pressed());
        let released = ui.input(|i| i.pointer.primary_released());
        if let Some(pos) = pointer {
            let (column, line, _) = cell_at(pos);
            if pressed {
                session_mouse(&mut term.mouse_down, true);
                output.extend(mouse_report(mode, 0, true, column, line));
            } else if released && term.mouse_down {
                session_mouse(&mut term.mouse_down, false);
                output.extend(mouse_report(mode, 0, false, column, line));
            }
        }
    } else {
        term.mouse_down = false;
        if response.drag_started() {
            if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
                let (point, side) = point_at(pos);
                grid.selection = Some(Selection::new(SelectionType::Simple, point, side));
            }
        }
        if response.dragged() {
            if let (Some(pos), Some(selection)) = (pointer, grid.selection.as_mut()) {
                let (point, side) = point_at(pos);
                selection.update(point, side);
            }
            // dragging past an edge scrolls the history
            if let Some(pos) = pointer {
                if pos.y < rect.top() {
                    grid.scroll_display(Scroll::Delta(1));
                } else if pos.y > rect.bottom() {
                    grid.scroll_display(Scroll::Delta(-1));
                }
            }
        }
        if response.triple_clicked() || response.double_clicked() {
            if let Some(pos) = pointer {
                let kind = if response.triple_clicked() { SelectionType::Lines } else { SelectionType::Semantic };
                let (point, _) = point_at(pos);
                let mut selection = Selection::new(kind, point, Side::Left);
                selection.update(point, Side::Right);
                grid.selection = Some(selection);
            }
        } else if response.clicked() {
            grid.selection = None;
        }
    }

    // the wheel
    if response.hovered() {
        let delta = ui.input_mut(|i| std::mem::take(&mut i.smooth_scroll_delta).y);
        if delta != 0.0 {
            term.scroll_rest += delta / cell.y;
            let steps = term.scroll_rest.trunc() as i32;
            term.scroll_rest -= steps as f32;
            if steps != 0 {
                if app_mouse {
                    if let Some(pos) = pointer {
                        let (column, line, _) = cell_at(pos);
                        let button = if steps > 0 { 64 } else { 65 };
                        for _ in 0..steps.unsigned_abs().min(20) {
                            output.extend(mouse_report(mode, button, true, column, line));
                        }
                    }
                } else if mode.contains(TermMode::ALT_SCREEN) {
                    // a full screen program without mouse: the wheel is the arrows
                    let key = if steps > 0 { Key::ArrowUp } else { Key::ArrowDown };
                    if let Some(bytes) = key_bytes(key, Modifiers::NONE, mode) {
                        for _ in 0..steps.unsigned_abs().min(20) {
                            output.extend(&bytes);
                        }
                    }
                } else {
                    grid.scroll_display(Scroll::Delta(steps));
                }
            }
        }
    }

    // the keyboard
    if focused {
        let events = ui.input(|i| i.events.clone());
        let mac = cfg!(target_os = "macos");
        let mut typed = false;
        for event in events {
            match event {
                egui::Event::Copy => match grid.selection_to_string().filter(|text| !text.is_empty()) {
                    Some(text) => ctx.copy_text(text),
                    // Ctrl+C without a selection interrupts, where Ctrl is also the copy key
                    None if !mac => output.push(0x03),
                    None => {}
                },
                egui::Event::Cut if !mac => output.push(0x18),
                egui::Event::Paste(text) => {
                    if mode.contains(TermMode::BRACKETED_PASTE) {
                        output.extend(b"\x1b[200~");
                        output.extend(text.replace('\x1b', "").as_bytes());
                        output.extend(b"\x1b[201~");
                    } else {
                        output.extend(text.replace("\r\n", "\r").replace('\n', "\r").as_bytes());
                    }
                    typed = true;
                }
                egui::Event::Text(text) | egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
                    // Alt+letter is Escape then the letter; on macOS Option composes characters
                    if !mac && ui.input(|i| i.modifiers.alt && !i.modifiers.ctrl) {
                        output.push(0x1b);
                    }
                    output.extend(text.as_bytes());
                    typed = true;
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    if modifiers.mac_cmd {
                        continue;
                    }
                    if modifiers.shift && !mode.contains(TermMode::ALT_SCREEN) {
                        let scroll = match key {
                            Key::PageUp => Some(Scroll::PageUp),
                            Key::PageDown => Some(Scroll::PageDown),
                            Key::Home => Some(Scroll::Top),
                            Key::End => Some(Scroll::Bottom),
                            _ => None,
                        };
                        if let Some(scroll) = scroll {
                            grid.scroll_display(scroll);
                            continue;
                        }
                    }
                    if let Some(bytes) = key_bytes(key, modifiers, mode) {
                        output.extend(bytes);
                        typed = true;
                    }
                }
                _ => {}
            }
        }
        if typed && !exited {
            grid.scroll_display(Scroll::Bottom);
            grid.selection = None;
        }
    }

    // the cells
    let palette = Palette {
        dark: background.map_or(ui.visuals().dark_mode, |color| color.intensity() < 0.5),
        foreground: text_color,
        backdrop: background.unwrap_or_else(|| ui.visuals().panel_fill).to_opaque(),
    };
    let selection_color = ui.visuals().selection.bg_fill;
    let content = grid.renderable_content();
    let offset = content.display_offset as i32;
    let selection = content.selection;
    let cursor = content.cursor;
    let mut runs: Vec<Run> = Vec::new();
    for indexed in content.display_iter {
        let row = indexed.point.line.0 + offset;
        if row < 0 || row as usize >= lines {
            continue;
        }
        let cell_ref = indexed.cell;
        let flags = cell_ref.flags;
        if flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
            continue;
        }
        let mut fg = palette.resolve(cell_ref.fg, &grid).unwrap_or(palette.foreground);
        let mut bg = palette.resolve(cell_ref.bg, &grid);
        if flags.contains(Flags::INVERSE) {
            let back = bg.unwrap_or(palette.backdrop);
            bg = Some(fg);
            fg = back;
        }
        if flags.contains(Flags::DIM) {
            fg = fg.gamma_multiply(0.66);
        }
        if selection.is_some_and(|range| range.contains(indexed.point)) {
            bg = Some(selection_color);
        }
        let look = Look {
            fg,
            bg,
            bold: flags.contains(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            strike: flags.contains(Flags::STRIKEOUT),
        };
        let c = if flags.contains(Flags::HIDDEN) || cell_ref.c == '\t' { ' ' } else { cell_ref.c };
        let zero_width = cell_ref.zerowidth();
        let ascii = c.is_ascii() && zero_width.is_none();
        let width = if flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
        let row = row as usize;
        let column = indexed.point.column.0;
        // a character out of ASCII may come from another font with another width: it is placed alone
        if let Some(run) = runs.last_mut() {
            if ascii && run.ascii && run.row == row && run.column + run.cells == column && run.look == look {
                run.text.push(c);
                run.cells += 1;
                continue;
            }
        }
        if c == ' ' && bg.is_none() && !look.underline && !look.strike {
            continue;
        }
        let mut text = String::from(c);
        if let Some(marks) = zero_width {
            text.extend(marks);
        }
        runs.push(Run { row, column, cells: width, text, look, ascii });
    }

    let painter = ui.painter().with_clip_rect(outer.intersect(ui.clip_rect()));
    let cell_rect = |row: usize, column: usize, cells: usize| -> Rect {
        Rect::from_min_size(
            Pos2::new(rect.left() + column as f32 * cell.x, rect.top() + row as f32 * cell.y),
            egui::vec2(cells as f32 * cell.x, cell.y),
        )
    };
    for run in &runs {
        if let Some(bg) = run.look.bg {
            painter.rect_filled(cell_rect(run.row, run.column, run.cells), 0.0, bg);
        }
    }
    for run in &runs {
        if run.text.trim().is_empty() && !run.look.underline && !run.look.strike {
            continue;
        }
        let pos = cell_rect(run.row, run.column, run.cells).min;
        let look = run.look;
        if look.italic || look.underline || look.strike {
            let line = Stroke::new(1.0, look.fg);
            let format = egui::TextFormat {
                font_id: font.clone(),
                color: look.fg,
                italics: look.italic,
                underline: if look.underline { line } else { Stroke::NONE },
                strikethrough: if look.strike { line } else { Stroke::NONE },
                ..Default::default()
            };
            let galley = painter.layout_job(egui::text::LayoutJob::single_section(run.text.clone(), format));
            painter.galley(pos, galley, look.fg);
        } else {
            painter.text(pos, egui::Align2::LEFT_TOP, &run.text, font.clone(), look.fg);
        }
        if look.bold {
            // the monospace font has no bold: the text is drawn twice, slightly shifted
            painter.text(pos + egui::vec2(0.5, 0.0), egui::Align2::LEFT_TOP, &run.text, font.clone(), look.fg);
        }
    }

    // the cursor
    let cursor_row = cursor.point.line.0 + offset;
    if !exited && cursor.shape != CursorShape::Hidden && cursor_row >= 0 && (cursor_row as usize) < lines {
        let cursor_cell = &grid.grid()[cursor.point];
        let wide = cursor_cell.flags.contains(Flags::WIDE_CHAR);
        let area = cell_rect(cursor_row as usize, cursor.point.column.0, if wide { 2 } else { 1 });
        let color = palette.foreground;
        match cursor.shape {
            _ if !focused => {
                painter.rect_stroke(area.shrink(0.5), 0.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
            }
            CursorShape::Beam => {
                painter.rect_filled(Rect::from_min_size(area.min, egui::vec2(1.5, cell.y)), 0.0, color);
            }
            CursorShape::Underline => {
                painter.rect_filled(Rect::from_min_max(area.left_bottom() - egui::vec2(0.0, 2.0), area.max), 0.0, color);
            }
            _ => {
                painter.rect_filled(area, 0.0, color);
                if cursor_cell.c != ' ' {
                    let text = cursor_cell.c.to_string();
                    painter.text(area.min, egui::Align2::LEFT_TOP, text, font.clone(), palette.backdrop);
                }
            }
        }
        if focused {
            // where the input method shows what is being composed (accents, emoji, CJK)
            ui.output_mut(|o| {
                o.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Terminal,
                    rect: outer,
                    cursor_rect: area,
                    should_interrupt_composition: false,
                })
            });
        }
    }

    // the position in the history
    let history = grid.grid().history_size();
    if history > 0 && (offset > 0 || response.hovered()) {
        let total = (history + lines) as f32;
        let track = outer.shrink2(egui::vec2(2.0, 2.0));
        let height = (lines as f32 / total * track.height()).max(16.0).min(track.height());
        let top = track.top() + (history as f32 - offset as f32) / total * (track.height() - height).max(0.0)
            / (history as f32 / total).max(f32::EPSILON);
        let thumb = Rect::from_min_size(Pos2::new(track.right() - 4.0, top.min(track.bottom() - height)), egui::vec2(4.0, height));
        painter.rect_filled(thumb, 2.0, palette.foreground.gamma_multiply(0.3));
    }
    drop(grid);

    if exited {
        let code = term.exit_code.map_or(String::new(), |code| format!(" with code {code}"));
        let message = format!("The shell has exited{code}. Enter starts a new one.");
        let pos = Pos2::new(rect.left(), outer.bottom() - padding);
        let galley = painter.layout_no_wrap(message, font.clone(), ui.visuals().weak_text_color());
        let area = Rect::from_min_size(pos - egui::vec2(0.0, galley.size().y), galley.size()).expand(2.0);
        painter.rect_filled(area, 2.0, palette.backdrop);
        painter.galley(area.min + egui::vec2(2.0, 2.0), galley, ui.visuals().weak_text_color());
    } else if !output.is_empty() {
        if let Some(session) = &term.session {
            session.write(output);
        }
    }
    if response.hovered() {
        ctx.set_cursor_icon(if app_mouse { egui::CursorIcon::Default } else { egui::CursorIcon::Text });
    }
    Ok(())
}

fn session_mouse(state: &mut bool, down: bool) {
    *state = down;
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Terminal>()?;
    m.add_function(wrap_pyfunction!(terminal, m)?)?;
    Ok(())
}
