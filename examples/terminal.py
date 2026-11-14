"""A terminal in a dock: a sidebar of files, a viewer and, below, a dock with two terminals.

    python examples/terminal.py [folder]

The sidebar lists the files of a folder (this repository by default). A click shows the file in the
viewer. The dock at the bottom holds a shell started in the folder and a Python prompt: they are real
terminals (colors, vim, htop). The buttons of the top bar write commands to the shell.

Click a terminal to type in it. Select with the mouse, copy with Cmd+C and paste with Cmd+V (Ctrl on
Windows and Linux), scroll the history with the wheel.
"""

import sys
from pathlib import Path

from pyegui import *

FOLDER = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent).resolve()

files = sorted(p for p in FOLDER.iterdir() if p.is_file() and not p.name.startswith("."))
current = Str(str(files[0]) if files else "")
started = Bool(False)
cache = {"path": None, "text": Str("")}

# the terminals are states: created once, outside the update function
shell = Terminal(cwd=str(FOLDER))
python = Terminal(cwd=str(FOLDER), shell=sys.executable, args=["-q"])


def text():
    """the text of the selected file, read again only when another one is selected"""
    if current.value != cache["path"]:
        cache["path"] = current.value
        try:
            cache["text"].value = Path(current.value).read_text(encoding="utf-8")[:200_000]
        except (OSError, UnicodeDecodeError):
            cache["text"].value = "(not a text file)"
    return cache["text"]


def sidebar():
    weak(FOLDER.name)

    def list_files():
        for path in files:
            if selectable_label_clicked(current.value == str(path), path.name):
                current.value = str(path)

    scroll_area_vertical(list_files)


def topbar():
    def buttons():
        if button_clicked("List the files"):
            shell.run("ls -l")
        if button_clicked("Git status"):
            shell.run("git status")
        if current.value and button_clicked("Count the lines"):
            shell.run(f"wc -l '{Path(current.value).name}'")
        if button_clicked("New shell"):
            shell.restart()
        weak(shell.title)

    horizontal(buttons)


def viewer():
    scroll_area_both(lambda: code(text().value))


dock = Dock(
    DockContainer(
        "vertical",
        [
            DockPane("Viewer", viewer, hide_tab=True),
            DockContainer(
                "tabs",
                [
                    DockPane("Terminal", lambda: terminal(shell)),
                    DockPane("Python", lambda: terminal(python)),
                ],
            ),
        ],
    ),
    id="terminal_example",
    tab_bar_height=26,
)
layout = dock.save()
layout["shares"] = [3.0, 2.0]
dock.load(layout)


def update_func(ctx):
    if not started.value:
        ctx.set_system_theme()
        shell.focus()
        started.value = True

    sidebar_panel(sidebar, default_size=200, min_size=140)
    topbar_panel(topbar)
    dock_area(dock)


if __name__ == "__main__":
    run_native(
        "Terminal",
        update_func,
        inner_width=1100,
        inner_height=720,
        fullsize_content=True,
        layout=dict(gap=8, rounding=12),
    )
