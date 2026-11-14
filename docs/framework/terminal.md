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

**Usage:**

```python
term = Terminal(cwd="/tmp")
python = Terminal(shell="python3", args=["-q"])

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

- `Terminal.text()` (method): The text of the screen and of the history, without the colors
- `Terminal.title` (attribute): The title given by the shell, usually the running command or the current folder; an empty

  string when it gave none


## `terminal(term, **kwargs)`

Shows a terminal. It takes all the space available, or the height given, and its number of
columns and rows follows its size. Click it to give it the keyboard: while it has the focus the
keys go to the shell and key_pressed() returns False (unless when_typing=True).

Without a background the terminal is transparent: it shows the sheet of the pane or of the
panel it is in. The 16 colors of the shell follow the light or dark theme.

**Args**

- `term` (Terminal): the state of the terminal

**Keyword args**

- `background` (tuple | str): the color behind the text
- `foreground` (tuple | str): the color of the text, the one of the theme by default
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
