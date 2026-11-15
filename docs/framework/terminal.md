# Terminal

A `Terminal` is a shell running in a pseudo terminal, drawn by `terminal`: a real terminal (colors, cursor, full screen programs, history, selection, copy and paste) to put in a `DockPane` or anywhere else. Create it once, outside the update function; the shell starts the first time it is drawn.

## `Terminal`


The state of a terminal: a shell running in a pseudo terminal, with its screen, its history and
its selection. Create it once, outside the update function, and draw it with terminal().

The shell starts the first time the terminal is drawn, with the size it is given. It is a real
terminal: colors, cursor movements and full screen programs (vim, htop) work. The text is
selected with the mouse and copied with Cmd+C (Ctrl+C on Windows and Linux when there is a
selection), pasted with Cmd+V or Ctrl+V, and the wheel scrolls the history.

**Args**

- `cwd` (str): the folder the shell starts in, the current one by default
- `shell` (str): the program to run, the shell of the user by default
- `args` (list[str]): the arguments of the program
- `env` (dict[str, str]): environment variables added to those of the app
- `scrollback` (int): number of lines kept in the history, 10000 by default
- `font_size` (float): size of the monospace font, 13 by default
- `light` (dict): the colors with the light theme, see set_colors()
- `dark` (dict): the colors with the dark theme, see set_colors()
- `font` (str): path of a monospace font file (.ttf, .otf) to draw with instead of the one of egui, e.g. a Nerd Font for the icons of a prompt; take its "Mono" variant, whose icons are one cell wide. The characters it does not have come from the font of egui

**Usage:**

```python
term = Terminal(cwd="/tmp")
python = Terminal(shell="python3", args=["-q"])
nerd = Terminal(font=str(Path.home() / "Library/Fonts/HackNerdFontMono-Regular.ttf"))

# colors that follow the theme of the app
term = Terminal(
    light=dict(background="#fafafa", foreground="#383a42"),
    dark=dict(background="#282c34", foreground="#abb2bf", cursor="#528bff"),
)

def update_func(ctx):
    terminal(term)

# in a dock
dock = Dock(DockContainer("vertical", [
    DockPane("Editor", editor),
    DockPane("Terminal", lambda: terminal(term)),
]))
```


- `Terminal.exit_code` (attribute): The exit code of the shell, None while it runs or when it was killed by a signal
- `Terminal.exited` (attribute): True once the shell has exited. Enter in the terminal, or restart(), starts a new one.
- `Terminal.focus()` (method): Gives the keyboard focus to the terminal the next time it is drawn
- `Terminal.font` (attribute): Path of the font file drawn with, or None for the monospace font of egui
- `Terminal.font_size` (attribute): Size of the monospace font
- `Terminal.has_focus` (attribute): True when the terminal had the keyboard focus the last time it was drawn
- `Terminal.restart(cwd=None)` (method): Stops the shell and starts a new one the next time the terminal is drawn


  **Keyword args**

  - `cwd` (str): the folder the new shell starts in, the same as before by default

- `Terminal.run(command)` (method): Runs a command: writes it to the shell followed by the Enter key
- `Terminal.running` (attribute): True while the shell runs. False before the terminal is drawn for the first time and once

  the shell has exited.

- `Terminal.selection` (attribute): The selected text, or None
- `Terminal.send(text)` (method): Writes a text to the shell, as if it was typed. Before the shell has started, the text is

  kept and written when it starts.

- `Terminal.set_colors(light=None, dark=None)` (method): Sets the colors of the terminal with the light theme, with the dark one, or both. The

  terminal takes those of the theme shown, at once: a change of theme changes them too.
  A color not given is the one of the theme of egui; the background of terminal(...) and
  its foreground win over these.

  **Keyword args**

  - `light` (dict): the colors with the light theme; {} goes back to those of egui
  - `dark` (dict): the colors with the dark theme; {} goes back to those of egui

  Each dict takes (a color is (r, g, b), (r, g, b, a) or "#rrggbb[aa]"):

      background: behind the text, the sheet of the pane by default (transparent)

      foreground: the text

      cursor: the cursor, the foreground by default

      selection: behind the selected text

      palette (list): the 16 colors of the shell, black, red, green, yellow, blue, magenta,
          cyan, white then their bright versions; a shorter list or None in it keeps the
          others

  **Example:**

  ```python
  term.set_colors(
      light=dict(background="#fdf6e3", foreground="#657b83"),
      dark=dict(background="#002b36", foreground="#839496", palette=[
          "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
          "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
      ]),
  )
  ```

- `Terminal.text()` (method): The text of the screen and of the history, without the colors
- `Terminal.title` (attribute): The title given by the shell, usually the running command or the current folder; an empty

  string when it gave none


## `terminal(term, **kwargs)`

Shows a terminal. It takes all the space available, or the height given, and its number of
columns and rows follows its size. Click it to give it the keyboard: while it has the focus the
keys go to the shell and key_pressed() returns False (unless when_typing=True).

Without a background the terminal is transparent: it shows the sheet of the pane or of the
panel it is in. The 16 colors of the shell follow the light or dark theme, and so do the
colors given by Terminal(light=..., dark=...) or term.set_colors().

**Args**

- `term` (Terminal): the state of the terminal

**Keyword args**

- `background` (tuple | str): the color behind the text, before the one of Terminal(light=, dark=)
- `foreground` (tuple | str): the color of the text, before the one of Terminal(light=, dark=), the one of the theme by default
- `height` (float): the height, the available one by default
- `padding` (float): the space around the text, 6 by default

**Example:**

```python
term = Terminal()
dock = Dock(DockContainer("vertical", [
    DockPane("Notes", lambda: label("...")),
    DockPane("Terminal", lambda: terminal(term)),
]))

def update_func(ctx):
    if button_clicked("List the files"):
        term.run("ls -l")
    dock_area(dock)

# a dark terminal whatever the theme
terminal(term, background="#1e1e1e", foreground="#d4d4d4")
```
